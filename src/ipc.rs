use std::{
    fs, io,
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
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
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let user = std::env::var("USER").unwrap_or_else(|_| "user".to_owned());
            std::env::temp_dir().join(format!("symbolis-{user}"))
        });
    fs::create_dir_all(&runtime_dir)?;
    Ok(runtime_dir.join("symbolis.sock"))
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
}
