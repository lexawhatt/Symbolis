use std::path::PathBuf;

use crate::preflight::LinuxSession;

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
    MissingFile(PathBuf),
    EmptyFileList,
}

impl std::fmt::Display for DragOutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DragOutError::Unsupported(reason) => write!(f, "{reason}"),
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
}

impl LinuxDragOutBackend {
    pub(crate) fn new(session: LinuxSession) -> Self {
        Self { session }
    }

    pub(crate) fn session_label(&self) -> &'static str {
        self.session.label()
    }
}

impl DragOutBackend for LinuxDragOutBackend {
    fn can_drag_files(&self) -> bool {
        false
    }

    fn begin_file_drag(
        &mut self,
        files: &[PathBuf],
        _preview: DragPreview,
    ) -> Result<(), DragOutError> {
        if files.is_empty() {
            return Err(DragOutError::EmptyFileList);
        }

        for file in files {
            if !file.exists() {
                return Err(DragOutError::MissingFile(file.clone()));
            }
        }

        Err(DragOutError::Unsupported(format!(
            "Native drag-out is not wired yet for {}. Use Copy file until the platform backend is implemented.",
            self.session.label()
        )))
    }
}
