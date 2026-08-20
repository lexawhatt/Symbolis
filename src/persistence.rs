use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

pub(crate) fn write_json_atomic<T: Serialize + ?Sized>(
    path: Option<&Path>,
    value: &T,
) -> io::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };

    let json = serde_json::to_string_pretty(value)?;
    write_atomic(path, json.as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = parent_dir(path);
    fs::create_dir_all(parent)?;

    let temp_path = create_temp_file(path, bytes)?;
    if let Err(err) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }

    sync_parent_dir(parent);
    Ok(())
}

fn create_temp_file(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("path has no file name: {}", path.display()),
            )
        })?;

    let parent = parent_dir(path);
    for attempt in 0..100 {
        let temp_path = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            unique_suffix(attempt)
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        };

        if let Err(err) = file.write_all(bytes).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&temp_path);
            return Err(err);
        }
        return Ok(temp_path);
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("could not create a temporary file for {}", path.display()),
    ))
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn unique_suffix(attempt: u32) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{nanos}-{attempt}")
}

fn sync_parent_dir(parent: &Path) {
    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct TestValue {
        value: &'static str,
    }

    #[test]
    fn json_write_replaces_existing_file_atomically() {
        let root = unique_test_dir();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(&path, r#"{"value":"old"}"#).unwrap();

        write_json_atomic(Some(&path), &TestValue { value: "new" }).unwrap();

        let saved = fs::read_to_string(&path).unwrap();
        fs::remove_dir_all(&root).unwrap();

        assert!(saved.contains("\"value\": \"new\""));
    }

    #[test]
    fn json_write_creates_parent_directory() {
        let root = unique_test_dir();
        let path = root.join("nested").join("state.json");

        write_json_atomic(Some(&path), &TestValue { value: "new" }).unwrap();

        let saved = fs::read_to_string(&path).unwrap();
        fs::remove_dir_all(&root).unwrap();

        assert!(saved.contains("\"value\": \"new\""));
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "symbolis-persistence-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
