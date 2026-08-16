use std::{
    collections::HashSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum MediaKind {
    Gif,
    Sticker,
}

impl MediaKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            MediaKind::Gif => "GIF",
            MediaKind::Sticker => "Sticker",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum MediaFormat {
    Gif,
    Mp4,
    Png,
    Webp,
    Webm,
}

impl MediaFormat {
    pub(crate) fn label(self) -> &'static str {
        match self {
            MediaFormat::Gif => "gif",
            MediaFormat::Mp4 => "mp4",
            MediaFormat::Png => "png",
            MediaFormat::Webp => "webp",
            MediaFormat::Webm => "webm",
        }
    }

    pub(crate) fn mime(self) -> &'static str {
        match self {
            MediaFormat::Gif => "image/gif",
            MediaFormat::Mp4 => "video/mp4",
            MediaFormat::Png => "image/png",
            MediaFormat::Webp => "image/webp",
            MediaFormat::Webm => "video/webm",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MediaItem {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) path: PathBuf,
    pub(crate) kind: MediaKind,
    pub(crate) format: MediaFormat,
    pub(crate) size_bytes: u64,
    pub(crate) modified_at: i64,
    pub(crate) search_text: String,
}

impl MediaItem {
    pub(crate) fn from_path(path: &Path) -> io::Result<Option<Self>> {
        let Some(format) = media_format(path) else {
            return Ok(None);
        };
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() {
            return Ok(None);
        }

        let title = path
            .file_stem()
            .and_then(|name| name.to_str())
            .map(strip_content_hash_suffix)
            .unwrap_or("media")
            .replace(['_', '-'], " ");
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let kind = match format {
            MediaFormat::Gif | MediaFormat::Mp4 | MediaFormat::Webm => MediaKind::Gif,
            MediaFormat::Png | MediaFormat::Webp => MediaKind::Sticker,
        };
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default();
        let size_bytes = metadata.len();
        let search_text = format!(
            "{} {} {} {}",
            title.to_lowercase(),
            canonical.display(),
            kind.label().to_lowercase(),
            format.label()
        );

        Ok(Some(Self {
            id: stable_media_id(&canonical),
            title,
            path: canonical,
            kind,
            format,
            size_bytes,
            modified_at,
            search_text,
        }))
    }

    pub(crate) fn display_size(&self) -> String {
        if self.size_bytes >= 1024 * 1024 {
            format!("{:.1} MB", self.size_bytes as f32 / (1024.0 * 1024.0))
        } else if self.size_bytes >= 1024 {
            format!("{:.0} KB", self.size_bytes as f32 / 1024.0)
        } else {
            format!("{} B", self.size_bytes)
        }
    }
}

pub(crate) fn media_root() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("symbolis").join("media"))
}

pub(crate) fn media_index_path() -> Option<PathBuf> {
    media_root().map(|root| root.join("index.json"))
}

pub(crate) fn optimized_media_dir() -> Option<PathBuf> {
    media_root().map(|root| root.join("optimized"))
}

pub(crate) fn export_media_dir() -> Option<PathBuf> {
    media_root().map(|root| root.join("exports"))
}

pub(crate) fn recent_media_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("symbolis").join("recent_media.json"))
}

pub(crate) fn favorite_media_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("symbolis").join("favorite_media.json"))
}

pub(crate) fn default_media_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(root) = media_root() {
        paths.push(root.join("gifs"));
        paths.push(root.join("stickers"));
        paths.push(root.join("saved"));
        paths.push(root.join("optimized"));
    }

    if let Some(pictures) = dirs::picture_dir() {
        paths.push(pictures.join("GIFs"));
        paths.push(pictures.join("Stickers"));
    }

    paths
}

pub(crate) fn load_recent_media(path: Option<&Path>) -> Vec<MediaItem> {
    let Some(path) = path else {
        return Vec::new();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(items) = serde_json::from_str::<Vec<MediaItem>>(&content) else {
        return Vec::new();
    };

    items
        .into_iter()
        .filter(|item| item.path.exists())
        .take(48)
        .collect()
}

pub(crate) fn save_recent_media(path: Option<&Path>, items: &[MediaItem]) -> io::Result<()> {
    write_json(path, items)
}

pub(crate) fn load_favorite_media_ids(path: Option<&Path>) -> Vec<String> {
    let Some(path) = path else {
        return Vec::new();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&content).unwrap_or_default()
}

pub(crate) fn save_favorite_media_ids(path: Option<&Path>, ids: &[String]) -> io::Result<()> {
    write_json(path, ids)
}

pub(crate) fn save_media_index(path: Option<&Path>, items: &[MediaItem]) -> io::Result<()> {
    write_json(path, items)
}

pub(crate) fn save_media_as_webm(item: &MediaItem) -> Result<PathBuf, MediaTranscodeError> {
    if !matches!(item.format, MediaFormat::Gif | MediaFormat::Mp4) {
        return Err(MediaTranscodeError::UnsupportedFormat(item.format));
    }

    save_video_path_as_webm(&item.path)
}

pub(crate) fn store_media_file_for_library(path: &Path) -> Result<PathBuf, MediaTranscodeError> {
    let Some(format) = media_format(path) else {
        return Err(MediaTranscodeError::UnsupportedPath(path.to_path_buf()));
    };

    if !path.is_file() {
        return Err(MediaTranscodeError::UnsupportedPath(path.to_path_buf()));
    }

    match format {
        MediaFormat::Gif | MediaFormat::Mp4 => save_video_path_as_webm(path),
        MediaFormat::Webm => copy_file_to_optimized_storage(path, MediaFormat::Webm),
        MediaFormat::Png | MediaFormat::Webp => copy_file_to_saved_storage(path, format),
    }
}

fn save_video_path_as_webm(path: &Path) -> Result<PathBuf, MediaTranscodeError> {
    let dir = optimized_media_dir().ok_or(MediaTranscodeError::MissingStorageRoot)?;
    fs::create_dir_all(&dir)?;
    let output = content_addressed_storage_path(path, &dir, "webm")?;
    if output.exists() {
        return Ok(output);
    }

    run_ffmpeg(
        Command::new("ffmpeg")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-y")
            .arg("-i")
            .arg(path)
            .arg("-an")
            .arg("-c:v")
            .arg("libvpx-vp9")
            .arg("-b:v")
            .arg("0")
            .arg("-crf")
            .arg("36")
            .arg("-pix_fmt")
            .arg("yuva420p")
            .arg("-auto-alt-ref")
            .arg("0")
            .arg(&output),
    )?;
    Ok(output)
}

pub(crate) fn export_media_for_transfer(item: &MediaItem) -> Result<PathBuf, MediaTranscodeError> {
    match item.format {
        MediaFormat::Mp4 | MediaFormat::Webm => export_video_to_gif(item),
        MediaFormat::Gif | MediaFormat::Png | MediaFormat::Webp => Ok(item.path.clone()),
    }
}

pub(crate) fn detect_media_transcoder() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn export_video_to_gif(item: &MediaItem) -> Result<PathBuf, MediaTranscodeError> {
    let dir = export_media_dir().ok_or(MediaTranscodeError::MissingStorageRoot)?;
    fs::create_dir_all(&dir)?;
    let output = dir.join(format!("{}.gif", item.id));
    if cached_export_is_current(&item.path, &output) {
        return Ok(output);
    }

    let palette = dir.join(format!("{}.palette.png", item.id));
    run_ffmpeg(
        Command::new("ffmpeg")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-y")
            .arg("-i")
            .arg(&item.path)
            .arg("-vf")
            .arg("fps=15,scale=iw:-1:flags=lanczos,palettegen=stats_mode=diff")
            .arg(&palette),
    )?;
    run_ffmpeg(
        Command::new("ffmpeg")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-y")
            .arg("-i")
            .arg(&item.path)
            .arg("-i")
            .arg(&palette)
            .arg("-lavfi")
            .arg(
                "fps=15,scale=iw:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5",
            )
            .arg(&output),
    )?;
    let _ = fs::remove_file(palette);
    Ok(output)
}

pub(crate) fn scan_media_library(paths: &[PathBuf]) -> Vec<MediaItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for path in paths {
        scan_path(path, &mut seen, &mut items);
    }

    items.sort_by(|a, b| {
        b.modified_at
            .cmp(&a.modified_at)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    items
}

pub(crate) fn is_supported_media_path(path: &Path) -> bool {
    path.is_dir() || media_format(path).is_some()
}

pub(crate) fn normalize_import_path(path: &Path) -> Option<PathBuf> {
    if !is_supported_media_path(path) {
        return None;
    }

    Some(fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
}

fn scan_path(path: &Path, seen: &mut HashSet<PathBuf>, items: &mut Vec<MediaItem>) {
    if path.is_file() {
        if let Ok(Some(item)) = MediaItem::from_path(path)
            && seen.insert(item.path.clone())
        {
            items.push(item);
        }
        return;
    }

    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_path(&path, seen, items);
        } else if let Ok(Some(item)) = MediaItem::from_path(&path)
            && seen.insert(item.path.clone())
        {
            items.push(item);
        }
    }
}

fn media_format(path: &Path) -> Option<MediaFormat> {
    match path.extension()?.to_str()?.to_lowercase().as_str() {
        "gif" => Some(MediaFormat::Gif),
        "mp4" | "m4v" => Some(MediaFormat::Mp4),
        "png" => Some(MediaFormat::Png),
        "webp" => Some(MediaFormat::Webp),
        "webm" => Some(MediaFormat::Webm),
        _ => None,
    }
}

fn copy_file_to_optimized_storage(
    path: &Path,
    format: MediaFormat,
) -> Result<PathBuf, MediaTranscodeError> {
    let dir = optimized_media_dir().ok_or(MediaTranscodeError::MissingStorageRoot)?;
    copy_file_to_storage(path, format, &dir)
}

fn copy_file_to_saved_storage(
    path: &Path,
    format: MediaFormat,
) -> Result<PathBuf, MediaTranscodeError> {
    let dir = media_root()
        .map(|root| root.join("saved"))
        .ok_or(MediaTranscodeError::MissingStorageRoot)?;
    copy_file_to_storage(path, format, &dir)
}

fn copy_file_to_storage(
    path: &Path,
    format: MediaFormat,
    dir: &Path,
) -> Result<PathBuf, MediaTranscodeError> {
    fs::create_dir_all(dir)?;
    let output = content_addressed_storage_path(path, dir, format.label())?;
    if same_file(path, &output) || output.exists() {
        return Ok(output);
    }

    fs::copy(path, &output)?;
    Ok(output)
}

#[cfg(test)]
fn content_addressed_file_name(path: &Path, extension: &str) -> io::Result<String> {
    let hash = file_content_hash(path)?;
    Ok(content_addressed_file_name_for_hash(path, extension, &hash))
}

fn content_addressed_storage_path(path: &Path, dir: &Path, extension: &str) -> io::Result<PathBuf> {
    let hash = file_content_hash(path)?;
    if let Some(existing) = existing_content_addressed_file(dir, extension, &hash)? {
        return Ok(existing);
    }

    Ok(dir.join(content_addressed_file_name_for_hash(path, extension, &hash)))
}

fn content_addressed_file_name_for_hash(path: &Path, extension: &str, hash: &str) -> String {
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .map(sanitize_file_stem)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "media".to_owned());
    format!("{stem}-{hash}.{extension}")
}

fn existing_content_addressed_file(
    dir: &Path,
    extension: &str,
    hash: &str,
) -> io::Result<Option<PathBuf>> {
    if !dir.is_dir() {
        return Ok(None);
    }

    let suffix = format!("-{hash}.{extension}");
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(&suffix))
        {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn sanitize_file_stem(stem: &str) -> String {
    stem.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn strip_content_hash_suffix(stem: &str) -> &str {
    let Some((name, hash)) = stem.rsplit_once('-') else {
        return stem;
    };

    if hash.len() == 16 && hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        name
    } else {
        stem
    }
}

fn same_file(left: &Path, right: &Path) -> bool {
    let (Ok(left), Ok(right)) = (fs::canonicalize(left), fs::canonicalize(right)) else {
        return false;
    };
    left == right
}

fn stable_media_id(path: &Path) -> String {
    let mut hash = 1469598103934665603_u64;
    for byte in path.to_string_lossy().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

fn write_json<T: Serialize + ?Sized>(path: Option<&Path>, value: &T) -> io::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(value)?;
    fs::write(path, json)
}

#[derive(Debug)]
pub(crate) enum MediaTranscodeError {
    UnsupportedFormat(MediaFormat),
    UnsupportedPath(PathBuf),
    MissingStorageRoot,
    Io(io::Error),
    FfmpegMissing,
    FfmpegFailed(String),
}

impl std::fmt::Display for MediaTranscodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaTranscodeError::UnsupportedFormat(format) => {
                write!(f, "cannot transcode {} media", format.label())
            }
            MediaTranscodeError::UnsupportedPath(path) => {
                write!(f, "unsupported media path: {}", path.display())
            }
            MediaTranscodeError::MissingStorageRoot => {
                write!(f, "media storage directory is unavailable")
            }
            MediaTranscodeError::Io(err) => write!(f, "{err}"),
            MediaTranscodeError::FfmpegMissing => {
                write!(f, "ffmpeg is required for GIF/MP4/WebM conversion")
            }
            MediaTranscodeError::FfmpegFailed(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for MediaTranscodeError {}

impl From<io::Error> for MediaTranscodeError {
    fn from(err: io::Error) -> Self {
        if err.kind() == io::ErrorKind::NotFound {
            MediaTranscodeError::FfmpegMissing
        } else {
            MediaTranscodeError::Io(err)
        }
    }
}

fn file_content_hash(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = 1469598103934665603_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1099511628211);
        }
    }

    Ok(format!("{hash:016x}"))
}

fn cached_export_is_current(source: &Path, output: &Path) -> bool {
    let (Ok(source), Ok(output)) = (fs::metadata(source), fs::metadata(output)) else {
        return false;
    };
    let (Ok(source_modified), Ok(output_modified)) = (source.modified(), output.modified()) else {
        return false;
    };
    output_modified >= source_modified
}

fn run_ffmpeg(command: &mut Command) -> Result<(), MediaTranscodeError> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    Err(MediaTranscodeError::FfmpegFailed(if message.is_empty() {
        format!("ffmpeg failed with status {}", output.status)
    } else {
        message.to_owned()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detects_supported_media_extensions() {
        assert!(matches!(
            media_format(Path::new("reaction.gif")),
            Some(MediaFormat::Gif)
        ));
        assert!(matches!(
            media_format(Path::new("sticker.WEBP")),
            Some(MediaFormat::Webp)
        ));
        assert!(matches!(
            media_format(Path::new("saved.webm")),
            Some(MediaFormat::Webm)
        ));
        assert!(matches!(
            media_format(Path::new("clip.MP4")),
            Some(MediaFormat::Mp4)
        ));
        assert!(matches!(
            media_format(Path::new("loop.m4v")),
            Some(MediaFormat::Mp4)
        ));
        assert!(media_format(Path::new("notes.txt")).is_none());
    }

    #[test]
    fn sanitizes_content_addressed_file_names() {
        let root = unique_test_dir();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("UltraKill Death!!.gif");
        fs::write(&path, b"gif").unwrap();

        let file_name = content_addressed_file_name(&path, "webm").unwrap();

        fs::remove_dir_all(&root).unwrap();

        assert!(file_name.starts_with("ultrakill-death-"));
        assert!(file_name.ends_with(".webm"));
    }

    #[test]
    fn reuses_existing_file_with_same_content_hash() {
        let root = unique_test_dir();
        fs::create_dir_all(&root).unwrap();
        let source = root.join("reaction.gif");
        fs::write(&source, b"same-media").unwrap();
        let existing = root.join(content_addressed_file_name(&source, "webm").unwrap());
        fs::write(&existing, b"stored-webm").unwrap();
        let renamed = root.join("renamed.gif");
        fs::write(&renamed, b"same-media").unwrap();

        let output = content_addressed_storage_path(&renamed, &root, "webm").unwrap();

        fs::remove_dir_all(&root).unwrap();

        assert_eq!(output, existing);
    }

    #[test]
    fn strips_generated_hash_suffix_from_titles() {
        assert_eq!(
            strip_content_hash_suffix("ultrakill-death-0123456789abcdef"),
            "ultrakill-death"
        );
        assert_eq!(strip_content_hash_suffix("manual-title"), "manual-title");
    }

    #[test]
    fn scans_supported_media_recursively() {
        let root = unique_test_dir();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("reaction.gif"), b"gif").unwrap();
        fs::write(root.join("clip.mp4"), b"mp4").unwrap();
        fs::write(root.join("notes.txt"), b"text").unwrap();
        fs::write(nested.join("sticker.png"), b"png").unwrap();

        let items = scan_media_library(std::slice::from_ref(&root));

        fs::remove_dir_all(&root).unwrap();

        assert_eq!(items.len(), 3);
        assert!(items.iter().any(|item| item.title == "reaction"));
        assert!(items.iter().any(|item| item.title == "clip"));
        assert!(items.iter().any(|item| item.title == "sticker"));
    }

    #[test]
    fn favorite_ids_round_trip_as_small_json() {
        let root = unique_test_dir();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("favorite_media.json");
        let ids = vec!["a".to_owned(), "b".to_owned()];

        save_favorite_media_ids(Some(&path), &ids).unwrap();
        let loaded = load_favorite_media_ids(Some(&path));

        fs::remove_dir_all(&root).unwrap();

        assert_eq!(loaded, ids);
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "symbolis-media-library-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
