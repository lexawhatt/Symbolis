use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(unix)]
use std::os::unix::{fs::OpenOptionsExt, fs::PermissionsExt};

use serde::{Deserialize, Serialize};

use crate::media_library::media_root;

pub(crate) const TELEGRAM_BOT_TOKEN_ENV: &str = "SYMBOLIS_TELEGRAM_BOT_TOKEN";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TelegramStickerImportSummary {
    pub(crate) set_name: String,
    pub(crate) title: String,
    pub(crate) imported: usize,
    pub(crate) skipped_animated: usize,
    pub(crate) skipped_unsupported: usize,
    pub(crate) failed: usize,
}

impl TelegramStickerImportSummary {
    pub(crate) fn status_label(&self) -> String {
        let mut label = format!(
            "Imported {} Telegram sticker{} from {}",
            self.imported,
            plural_suffix(self.imported),
            self.title
        );

        if self.skipped_animated > 0 {
            label.push_str(&format!(
                "; skipped {} animated .tgs sticker{}",
                self.skipped_animated,
                plural_suffix(self.skipped_animated)
            ));
        }
        if self.skipped_unsupported > 0 {
            label.push_str(&format!(
                "; skipped {} unsupported file{}",
                self.skipped_unsupported,
                plural_suffix(self.skipped_unsupported)
            ));
        }
        if self.failed > 0 {
            label.push_str(&format!(
                "; failed {} download{}",
                self.failed,
                plural_suffix(self.failed)
            ));
        }

        label
    }
}

pub(crate) fn telegram_bot_token() -> Option<String> {
    telegram_bot_token_from_env().or_else(load_saved_telegram_bot_token)
}

pub(crate) fn telegram_bot_token_from_env() -> Option<String> {
    env::var(TELEGRAM_BOT_TOKEN_ENV)
        .ok()
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

pub(crate) fn load_saved_telegram_bot_token() -> Option<String> {
    let path = telegram_secret_path()?;
    let content = fs::read_to_string(path).ok()?;
    let secret = serde_json::from_str::<TelegramSecret>(&content).ok()?;
    normalize_bot_token(&secret.bot_token)
}

pub(crate) fn save_telegram_bot_token(token: &str) -> io::Result<()> {
    let Some(token) = normalize_bot_token(token) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Telegram bot token is empty",
        ));
    };
    let path = telegram_secret_path()
        .ok_or_else(|| io::Error::other("config directory is unavailable"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(&TelegramSecret { bot_token: token })?;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path)?;
    file.write_all(json.as_bytes())?;
    set_secret_file_permissions(&path)?;
    Ok(())
}

pub(crate) fn clear_saved_telegram_bot_token() -> io::Result<()> {
    let Some(path) = telegram_secret_path() else {
        return Ok(());
    };
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub(crate) fn telegram_secret_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("symbolis").join("telegram_secret.json"))
}

fn normalize_bot_token(token: &str) -> Option<String> {
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

fn set_secret_file_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub(crate) fn sticker_set_name_from_input(input: &str) -> Option<String> {
    let input = input.trim().trim_end_matches('/');
    let name = if let Some(rest) = input
        .strip_prefix("https://t.me/addstickers/")
        .or_else(|| input.strip_prefix("http://t.me/addstickers/"))
        .or_else(|| input.strip_prefix("https://telegram.me/addstickers/"))
        .or_else(|| input.strip_prefix("http://telegram.me/addstickers/"))
    {
        rest.split(['?', '#', '/'])
            .next()
            .unwrap_or_default()
            .to_owned()
    } else if let Some(query) = input.strip_prefix("tg://addstickers?") {
        query
            .split('&')
            .filter_map(|part| part.split_once('='))
            .find_map(|(key, value)| (key == "set").then(|| percent_decode(value)))
            .unwrap_or_default()
    } else {
        return None;
    };

    is_valid_sticker_set_name(&name).then_some(name)
}

pub(crate) fn import_telegram_sticker_set(
    set_name: &str,
    token: &str,
) -> Result<TelegramStickerImportSummary, TelegramStickerImportError> {
    import_telegram_sticker_set_with_progress(set_name, token, |_| {})
}

pub(crate) fn import_telegram_sticker_set_with_progress(
    set_name: &str,
    token: &str,
    mut progress: impl FnMut(String),
) -> Result<TelegramStickerImportSummary, TelegramStickerImportError> {
    progress(format!(
        "fetching Telegram sticker set metadata for {set_name}"
    ));
    let set: StickerSet = call_telegram_method(token, "getStickerSet", &[("name", set_name)])?;
    let dir = telegram_sticker_set_dir(&set.name)?;
    fs::create_dir_all(&dir)?;
    let total = set.stickers.len();
    progress(format!(
        "Telegram set {} has {total} sticker{}",
        set.title,
        plural_suffix(total)
    ));

    let mut summary = TelegramStickerImportSummary {
        set_name: set.name.clone(),
        title: set.title.clone(),
        imported: 0,
        skipped_animated: 0,
        skipped_unsupported: 0,
        failed: 0,
    };

    for (index, sticker) in set.stickers.iter().enumerate() {
        if sticker.is_animated && !sticker.is_video {
            summary.skipped_animated += 1;
            report_progress(index, total, &summary, &mut progress);
            continue;
        }

        let file = match call_telegram_method::<TelegramFile>(
            token,
            "getFile",
            &[("file_id", &sticker.file_id)],
        ) {
            Ok(file) => file,
            Err(_) => {
                summary.failed += 1;
                report_progress(index, total, &summary, &mut progress);
                continue;
            }
        };
        let Some(file_path) = file.file_path else {
            summary.failed += 1;
            report_progress(index, total, &summary, &mut progress);
            continue;
        };
        let Some(extension) = supported_sticker_extension(&file_path, sticker) else {
            summary.skipped_unsupported += 1;
            report_progress(index, total, &summary, &mut progress);
            continue;
        };

        let file_name = telegram_sticker_file_name(index, sticker, extension);
        let output = dir.join(file_name);
        if output.exists() {
            summary.imported += 1;
            report_progress(index, total, &summary, &mut progress);
            continue;
        }

        match download_telegram_file(token, &file_path, &output) {
            Ok(()) => summary.imported += 1,
            Err(_) => summary.failed += 1,
        }
        report_progress(index, total, &summary, &mut progress);
    }

    Ok(summary)
}

fn report_progress(
    index: usize,
    total: usize,
    summary: &TelegramStickerImportSummary,
    progress: &mut impl FnMut(String),
) {
    let processed = index + 1;
    if processed == 1 || processed == total || processed.is_multiple_of(10) {
        progress(format!(
            "Telegram {}: processed {processed}/{total}; imported {}, skipped {}, failed {}",
            summary.title,
            summary.imported,
            summary.skipped_animated + summary.skipped_unsupported,
            summary.failed
        ));
    }
}

fn call_telegram_method<T: for<'de> Deserialize<'de>>(
    token: &str,
    method: &str,
    params: &[(&str, &str)],
) -> Result<T, TelegramStickerImportError> {
    let mut command = Command::new("curl");
    command.arg("-fsSL").arg("--get");
    for (key, value) in params {
        command
            .arg("--data-urlencode")
            .arg(format!("{key}={value}"));
    }
    command.arg("--config").arg("-");

    let output = run_curl(
        &mut command,
        &curl_url_config(&telegram_api_url(token, method)),
    )?;
    let response: TelegramResponse<T> = serde_json::from_slice(&output.stdout)?;
    if response.ok {
        response
            .result
            .ok_or(TelegramStickerImportError::MissingResult)
    } else {
        Err(TelegramStickerImportError::TelegramApi(
            response
                .description
                .unwrap_or_else(|| "Telegram API returned ok=false".to_owned()),
        ))
    }
}

fn download_telegram_file(
    token: &str,
    file_path: &str,
    output: &Path,
) -> Result<(), TelegramStickerImportError> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = output.with_file_name(format!(
        ".{}.part",
        output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("telegram-sticker")
    ));
    let mut command = Command::new("curl");
    command
        .arg("-fsSL")
        .arg("--retry")
        .arg("2")
        .arg("--config")
        .arg("-")
        .arg("--output")
        .arg(&tmp);
    run_curl(
        &mut command,
        &curl_url_config(&telegram_file_url(token, file_path)),
    )?;
    fs::rename(tmp, output)?;
    Ok(())
}

fn run_curl(
    command: &mut Command,
    config: &str,
) -> Result<std::process::Output, TelegramStickerImportError> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                TelegramStickerImportError::CurlMissing
            } else {
                TelegramStickerImportError::Io(err)
            }
        })?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        TelegramStickerImportError::Io(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "curl stdin is unavailable",
        ))
    })?;
    stdin.write_all(config.as_bytes()).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            TelegramStickerImportError::CurlMissing
        } else {
            TelegramStickerImportError::Io(err)
        }
    })?;
    drop(stdin);

    let output = child.wait_with_output()?;
    if output.status.success() {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(TelegramStickerImportError::CurlFailed(
        if stderr.trim().is_empty() {
            format!("curl failed with status {}", output.status)
        } else {
            stderr.trim().to_owned()
        },
    ))
}

fn telegram_sticker_set_dir(set_name: &str) -> Result<PathBuf, TelegramStickerImportError> {
    let root = media_root().ok_or(TelegramStickerImportError::MissingStorageRoot)?;
    Ok(root
        .join("stickers")
        .join("telegram")
        .join(sanitize_file_part(set_name)))
}

fn telegram_sticker_file_name(index: usize, sticker: &TelegramSticker, extension: &str) -> String {
    format!(
        "{:03}-{}.{}",
        index + 1,
        sanitize_file_part(&sticker.file_unique_id),
        extension
    )
}

fn supported_sticker_extension<'a>(
    file_path: &'a str,
    sticker: &TelegramSticker,
) -> Option<&'a str> {
    let extension = Path::new(file_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    match extension.as_deref() {
        Some("webp") => Some("webp"),
        Some("webm") if sticker.is_video => Some("webm"),
        Some("png") => Some("png"),
        _ => None,
    }
}

fn telegram_api_url(token: &str, method: &str) -> String {
    format!("https://api.telegram.org/bot{token}/{method}")
}

fn telegram_file_url(token: &str, file_path: &str) -> String {
    format!("https://api.telegram.org/file/bot{token}/{file_path}")
}

fn curl_url_config(url: &str) -> String {
    format!("url = \"{}\"\n", curl_config_escape(url))
}

fn curl_config_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn is_valid_sticker_set_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn sanitize_file_part(value: &str) -> String {
    let sanitized = value
        .chars()
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
        .join("-");

    if sanitized.is_empty() {
        "telegram-sticker".to_owned()
    } else {
        sanitized
    }
}

fn percent_decode(value: &str) -> String {
    let mut output = String::new();
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            output.push((high << 4 | low) as char);
            index += 3;
            continue;
        }
        output.push(bytes[index] as char);
        index += 1;
    }

    output
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

#[derive(Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct StickerSet {
    name: String,
    title: String,
    stickers: Vec<TelegramSticker>,
}

#[derive(Deserialize)]
struct TelegramSticker {
    file_id: String,
    file_unique_id: String,
    #[serde(default)]
    is_animated: bool,
    #[serde(default)]
    is_video: bool,
}

#[derive(Deserialize)]
struct TelegramFile {
    file_path: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct TelegramSecret {
    bot_token: String,
}

#[derive(Debug)]
pub(crate) enum TelegramStickerImportError {
    MissingStorageRoot,
    MissingResult,
    CurlMissing,
    CurlFailed(String),
    TelegramApi(String),
    Io(io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for TelegramStickerImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TelegramStickerImportError::MissingStorageRoot => {
                write!(f, "media storage directory is unavailable")
            }
            TelegramStickerImportError::MissingResult => {
                write!(f, "Telegram API response did not include a result")
            }
            TelegramStickerImportError::CurlMissing => {
                write!(f, "curl is required for Telegram sticker import")
            }
            TelegramStickerImportError::CurlFailed(message) => write!(f, "{message}"),
            TelegramStickerImportError::TelegramApi(message) => write!(f, "{message}"),
            TelegramStickerImportError::Io(err) => write!(f, "{err}"),
            TelegramStickerImportError::Json(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for TelegramStickerImportError {}

impl From<io::Error> for TelegramStickerImportError {
    fn from(err: io::Error) -> Self {
        TelegramStickerImportError::Io(err)
    }
}

impl From<serde_json::Error> for TelegramStickerImportError {
    fn from(err: serde_json::Error) -> Self {
        TelegramStickerImportError::Json(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_addstickers_links() {
        assert_eq!(
            sticker_set_name_from_input("https://t.me/addstickers/EdgyCatboy"),
            Some("EdgyCatboy".to_owned())
        );
        assert_eq!(
            sticker_set_name_from_input("tg://addstickers?set=EdgyCatboy"),
            Some("EdgyCatboy".to_owned())
        );
    }

    #[test]
    fn rejects_non_sticker_inputs() {
        assert_eq!(sticker_set_name_from_input("EdgyCatboy"), None);
        assert_eq!(sticker_set_name_from_input("/home/lexa/Pictures"), None);
        assert_eq!(sticker_set_name_from_input("https://example.com/x"), None);
        assert_eq!(sticker_set_name_from_input("bad/name"), None);
    }

    #[test]
    fn normalizes_saved_token_input() {
        assert_eq!(
            normalize_bot_token("  123:abc  "),
            Some("123:abc".to_owned())
        );
        assert_eq!(normalize_bot_token("   "), None);
    }

    #[test]
    fn formats_partial_import_summary() {
        let summary = TelegramStickerImportSummary {
            set_name: "Set".to_owned(),
            title: "Set Title".to_owned(),
            imported: 3,
            skipped_animated: 2,
            skipped_unsupported: 1,
            failed: 1,
        };

        assert_eq!(
            summary.status_label(),
            "Imported 3 Telegram stickers from Set Title; skipped 2 animated .tgs stickers; skipped 1 unsupported file; failed 1 download"
        );
    }
}
