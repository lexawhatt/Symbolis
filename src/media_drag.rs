use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::preflight::LinuxSession;

const DRAG_HELPER_ENV: &str = "SYMBOLIS_DRAG_HELPER";
const DRAGON_DROP_COMMAND: &str = "dragon-drop";
const DRAGON_COMMAND: &str = "dragon";

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct DragPreview {
    pub(crate) label: String,
    pub(crate) mime: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) enum DragOutError {
    Unsupported(String),
    LaunchFailed(String),
    MissingFile(PathBuf),
    EmptyFileList,
}

impl std::fmt::Display for DragOutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DragOutError::Unsupported(reason) => write!(f, "{reason}"),
            DragOutError::LaunchFailed(reason) => write!(f, "{reason}"),
            DragOutError::MissingFile(path) => write!(f, "file does not exist: {}", path.display()),
            DragOutError::EmptyFileList => write!(f, "file list is empty"),
        }
    }
}

impl std::error::Error for DragOutError {}

pub(crate) trait DragOutBackend {
    fn can_drag_files(&self) -> bool;
    #[allow(dead_code)]
    fn begin_file_drag(
        &mut self,
        files: &[PathBuf],
        preview: DragPreview,
    ) -> Result<(), DragOutError>;
}

pub(crate) struct LinuxDragOutBackend {
    session: LinuxSession,
    helper: LinuxDragHelper,
}

impl LinuxDragOutBackend {
    pub(crate) fn new(session: LinuxSession, helper: LinuxDragHelper) -> Self {
        Self { session, helper }
    }

    pub(crate) fn session_label(&self) -> &'static str {
        self.session.label()
    }

    pub(crate) fn helper_label(&self) -> String {
        self.helper.label()
    }
}

impl DragOutBackend for LinuxDragOutBackend {
    fn can_drag_files(&self) -> bool {
        true
    }

    fn begin_file_drag(
        &mut self,
        files: &[PathBuf],
        preview: DragPreview,
    ) -> Result<(), DragOutError> {
        validate_drag_files(files)?;
        self.helper.launch(files, &preview)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LinuxDragHelper {
    kind: LinuxDragHelperKind,
    command: PathBuf,
}

impl LinuxDragHelper {
    pub(crate) fn label(&self) -> String {
        format!("{} ({})", self.kind.label(), self.command.display())
    }

    #[allow(dead_code)]
    fn launch(&self, files: &[PathBuf], preview: &DragPreview) -> Result<(), DragOutError> {
        let mut command = Command::new(&self.command);

        match self.kind {
            LinuxDragHelperKind::DragonDrop => {
                command.arg("--and-exit").arg("--on-top");
                if files.len() > 1 {
                    command.arg("--all");
                }
            }
            LinuxDragHelperKind::Dragon => {
                command.arg("--and-exit");
                if files.len() > 1 {
                    command.arg("--all");
                }
            }
        }

        command
            .env("SYMBOLIS_DRAG_LABEL", &preview.label)
            .env("SYMBOLIS_DRAG_MIME", &preview.mime)
            .args(files);

        command.spawn().map(|_| ()).map_err(|err| {
            DragOutError::LaunchFailed(format!(
                "failed to start {} drag helper: {err}",
                self.kind.label()
            ))
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LinuxDragHelperKind {
    DragonDrop,
    Dragon,
}

impl LinuxDragHelperKind {
    fn label(self) -> &'static str {
        match self {
            LinuxDragHelperKind::DragonDrop => "dragon-drop",
            LinuxDragHelperKind::Dragon => "dragon",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MissingDragHelper {
    searched: Vec<String>,
}

impl MissingDragHelper {
    pub(crate) fn install_hint(&self) -> String {
        format!(
            "Install mwh/dragon drag source as `dragon-drop`, or set {DRAG_HELPER_ENV}=/path/to/dragon. Searched: {}",
            self.searched.join(", ")
        )
    }
}

impl fmt::Display for MissingDragHelper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Linux media drag helper is missing")
    }
}

pub(crate) fn detect_linux_drag_helper() -> Result<LinuxDragHelper, MissingDragHelper> {
    let mut searched = Vec::new();

    if let Some(path) = env::var_os(DRAG_HELPER_ENV).map(PathBuf::from) {
        searched.push(format!("{DRAG_HELPER_ENV}={}", path.display()));
        if is_executable_file(&path) {
            return Ok(LinuxDragHelper {
                kind: helper_kind_for_env_path(&path),
                command: path,
            });
        }
    }

    if let Some(path) = find_executable_in_path(DRAGON_DROP_COMMAND) {
        searched.push(DRAGON_DROP_COMMAND.to_owned());
        return Ok(LinuxDragHelper {
            kind: LinuxDragHelperKind::DragonDrop,
            command: path,
        });
    }
    searched.push(DRAGON_DROP_COMMAND.to_owned());

    if let Some(path) = find_executable_in_path(DRAGON_COMMAND) {
        if looks_like_mwh_dragon(&path) {
            searched.push(DRAGON_COMMAND.to_owned());
            return Ok(LinuxDragHelper {
                kind: LinuxDragHelperKind::Dragon,
                command: path,
            });
        }
    }
    searched.push(format!("{DRAGON_COMMAND} (mwh/dragon compatible)"));

    Err(MissingDragHelper { searched })
}

#[allow(dead_code)]
fn validate_drag_files(files: &[PathBuf]) -> Result<(), DragOutError> {
    if files.is_empty() {
        return Err(DragOutError::EmptyFileList);
    }

    for file in files {
        if !file.exists() {
            return Err(DragOutError::MissingFile(file.clone()));
        }
    }

    Ok(())
}

fn helper_kind_for_env_path(path: &Path) -> LinuxDragHelperKind {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == DRAGON_DROP_COMMAND)
    {
        LinuxDragHelperKind::DragonDrop
    } else {
        LinuxDragHelperKind::Dragon
    }
}

fn find_executable_in_path(name: &str) -> Option<PathBuf> {
    if name.contains(std::path::MAIN_SEPARATOR) {
        let path = PathBuf::from(name);
        return is_executable_file(&path).then_some(path);
    }

    env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(name))
        .find(|path| is_executable_file(path))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };

    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn looks_like_mwh_dragon(path: &Path) -> bool {
    let Ok(output) = Command::new(path).arg("--help").output() else {
        return false;
    };

    let mut text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    text.push_str(&String::from_utf8_lossy(&output.stderr).to_lowercase());
    text.contains("drag") && text.contains("drop") && text.contains("--and-exit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_drag_file_list() {
        assert!(matches!(
            validate_drag_files(&[]),
            Err(DragOutError::EmptyFileList)
        ));
    }

    #[test]
    fn rejects_missing_drag_file() {
        let missing = PathBuf::from("/definitely/not/a/symbolis/media/file.gif");
        assert!(matches!(
            validate_drag_files(&[missing]),
            Err(DragOutError::MissingFile(_))
        ));
    }

    #[test]
    fn env_path_named_dragon_drop_uses_dragon_drop_flags() {
        let helper = LinuxDragHelper {
            kind: helper_kind_for_env_path(Path::new("/tmp/dragon-drop")),
            command: PathBuf::from("/tmp/dragon-drop"),
        };
        assert_eq!(helper.kind, LinuxDragHelperKind::DragonDrop);
    }
}
