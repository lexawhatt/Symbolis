use std::path::{Path, PathBuf};

use arboard::Clipboard;

pub(crate) struct MediaClipboard {
    clipboard: Clipboard,
}

impl MediaClipboard {
    pub(crate) fn new() -> Result<Self, arboard::Error> {
        Clipboard::new().map(|clipboard| Self { clipboard })
    }

    pub(crate) fn copy_text(&mut self, text: impl Into<String>) -> Result<(), arboard::Error> {
        self.clipboard.set_text(text.into())
    }

    #[allow(dead_code)]
    pub(crate) fn copy_file_list(
        &mut self,
        files: &[PathBuf],
    ) -> Result<(), ClipboardDeliveryError> {
        validate_files(files)?;
        self.clipboard
            .set()
            .file_list(files)
            .map_err(ClipboardDeliveryError::Clipboard)
    }
}

#[derive(Debug)]
pub(crate) enum ClipboardDeliveryError {
    Clipboard(arboard::Error),
    EmptyFileList,
    MissingFile(PathBuf),
}

impl std::fmt::Display for ClipboardDeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipboardDeliveryError::Clipboard(err) => write!(f, "{err}"),
            ClipboardDeliveryError::EmptyFileList => write!(f, "file list is empty"),
            ClipboardDeliveryError::MissingFile(path) => {
                write!(f, "file does not exist: {}", path.display())
            }
        }
    }
}

impl std::error::Error for ClipboardDeliveryError {}

fn validate_files(files: &[PathBuf]) -> Result<(), ClipboardDeliveryError> {
    if files.is_empty() {
        return Err(ClipboardDeliveryError::EmptyFileList);
    }

    for file in files {
        if !path_exists(file) {
            return Err(ClipboardDeliveryError::MissingFile(file.clone()));
        }
    }

    Ok(())
}

fn path_exists(path: &Path) -> bool {
    path.exists()
}
