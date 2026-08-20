use std::{
    fs, io,
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use eframe::egui::Context;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IpcCommand {
    Toggle,
    ShowMain,
    ShowSymbols,
    ShowStickers,
    ShowGifs,
    Quit,
}

impl IpcCommand {
    const ALL: [Self; 6] = [
        Self::Toggle,
        Self::ShowMain,
        Self::ShowSymbols,
        Self::ShowStickers,
        Self::ShowGifs,
        Self::Quit,
    ];

    pub(crate) fn arg(self) -> &'static str {
        match self {
            IpcCommand::Toggle => "--toggle",
            IpcCommand::ShowMain => "--show-main",
            IpcCommand::ShowSymbols => "--show-symbols",
            IpcCommand::ShowStickers => "--show-stickers",
            IpcCommand::ShowGifs => "--show-gifs",
            IpcCommand::Quit => "--quit",
        }
    }

    pub(crate) fn from_arg(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|command| command.arg() == value)
    }

    fn wire(self) -> &'static str {
        match self {
            IpcCommand::Toggle => "toggle",
            IpcCommand::ShowMain => "show-main",
            IpcCommand::ShowSymbols => "show-symbols",
            IpcCommand::ShowStickers => "show-stickers",
            IpcCommand::ShowGifs => "show-gifs",
            IpcCommand::Quit => "quit",
        }
    }

    fn from_wire(value: &str) -> Option<Self> {
        let value = value.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|command| command.wire() == value)
    }
}

pub(crate) struct IpcServer {
    receiver: Receiver<IpcCommand>,
}

impl IpcServer {
    pub(crate) fn start(ctx: Context) -> io::Result<Self> {
        let path = socket_path()?;
        remove_stale_socket(&path);
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = String::new();
                        if stream.read_to_string(&mut buffer).is_ok()
                            && let Some(command) = IpcCommand::from_wire(&buffer)
                        {
                            let _ = tx.send(command);
                            ctx.request_repaint();
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(80));
                    }
                    Err(_) => break,
                }
            }
            let _ = fs::remove_file(path);
        });

        Ok(Self { receiver: rx })
    }

    pub(crate) fn try_recv(&self) -> Option<IpcCommand> {
        self.receiver.try_recv().ok()
    }
}

pub(crate) fn send_command(command: IpcCommand) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket_path()?)?;
    stream.write_all(command.wire().as_bytes())
}

pub(crate) fn socket_path() -> io::Result<PathBuf> {
    let runtime_dir = runtime_dir()?;
    Ok(runtime_dir.join("symbolis.sock"))
}

fn runtime_dir() -> io::Result<PathBuf> {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let runtime_dir = PathBuf::from(runtime_dir);
        fs::create_dir_all(&runtime_dir)?;
        return Ok(runtime_dir);
    }

    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_owned());
    let runtime_dir = std::env::temp_dir().join(format!("symbolis-{user}"));
    create_private_runtime_dir(&runtime_dir)?;
    Ok(runtime_dir)
}

fn create_private_runtime_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    builder.mode(0o700);
    builder.create(path)?;

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "IPC runtime directory cannot be a symlink: {}",
                path.display()
            ),
        ));
    }

    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("IPC runtime path is not a directory: {}", path.display()),
        ));
    }

    if metadata.uid() != current_uid()? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "IPC runtime directory is owned by another user: {}",
                path.display()
            ),
        ));
    }

    let mode = metadata.permissions().mode() & 0o777;
    if mode != 0o700 {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)?;
    }

    Ok(())
}

fn current_uid() -> io::Result<u32> {
    let status = fs::read_to_string("/proc/self/status")?;
    let uid_line = status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "missing Uid line in /proc/self/status",
            )
        })?;
    uid_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing real uid"))
        .and_then(|uid| {
            uid.parse::<u32>()
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
        })
}

fn remove_stale_socket(path: &Path) {
    if !path.exists() {
        return;
    }
    if UnixStream::connect(path).is_err() {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn maps_cli_args_to_commands() {
        for command in IpcCommand::ALL {
            assert_eq!(IpcCommand::from_arg(command.arg()), Some(command));
        }
        assert_eq!(IpcCommand::from_arg("--unknown"), None);
    }

    #[test]
    fn maps_wire_values_to_commands() {
        for command in IpcCommand::ALL {
            assert_eq!(IpcCommand::from_wire(command.wire()), Some(command));
        }
        assert_eq!(IpcCommand::from_wire("unknown"), None);
    }

    #[test]
    fn temp_runtime_dir_is_private() {
        let root = unique_test_dir();
        let path = root.join("runtime");

        create_private_runtime_dir(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(mode, 0o700);
    }

    #[test]
    fn temp_runtime_dir_rejects_symlink() {
        let root = unique_test_dir();
        let target = root.join("target");
        let link = root.join("runtime");
        fs::create_dir_all(&target).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let err = create_private_runtime_dir(&link).unwrap_err();

        fs::remove_dir_all(&root).unwrap();

        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("symbolis-ipc-test-{}-{nanos}", std::process::id()))
    }
}
