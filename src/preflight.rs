use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

use arboard::Clipboard;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::{
    emoji_cache::detect_color_emoji_renderer,
    media_drag::{LinuxDragHelper, detect_linux_drag_helper},
    media_library::detect_media_transcoder,
};

const WINDOW_BACKEND_ENV: &str = "SYMBOLIS_WINDOW_BACKEND";

#[derive(Clone, Debug)]
pub(crate) struct PreflightReport {
    pub(crate) linux_session: LinuxSession,
    pub(crate) drag_helper: Option<LinuxDragHelper>,
    pub(crate) color_emoji_renderer: bool,
    pub(crate) warnings: Vec<StartupWarning>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LinuxSession {
    Wayland {
        display: String,
        runtime_dir: PathBuf,
    },
    X11 {
        display: String,
    },
}

impl LinuxSession {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            LinuxSession::Wayland { .. } => "Wayland",
            LinuxSession::X11 { .. } => "X11",
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LinuxWindowBackendPreference {
    Auto,
    X11,
    Wayland,
}

#[derive(Clone, Debug)]
pub(crate) struct PreflightError {
    checks: Vec<FailedCheck>,
}

impl PreflightError {
    fn new(checks: Vec<FailedCheck>) -> Self {
        Self { checks }
    }
}

impl fmt::Display for PreflightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Symbolis cannot start because required Linux desktop capabilities are missing."
        )?;
        writeln!(f)?;
        for check in &self.checks {
            writeln!(f, "- {}", check.message)?;
            if let Some(hint) = &check.hint {
                writeln!(f, "  Fix: {hint}")?;
            }
        }
        writeln!(f)?;
        writeln!(
            f,
            "Symbolis currently targets Linux desktop sessions with working Wayland or X11 clipboard support."
        )
    }
}

#[derive(Clone, Debug)]
struct FailedCheck {
    message: String,
    hint: Option<String>,
}

impl FailedCheck {
    fn new(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    fn without_hint(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StartupWarning {
    pub(crate) feature: &'static str,
    pub(crate) message: String,
    pub(crate) hint: Option<String>,
}

impl StartupWarning {
    fn new(feature: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            feature,
            message: message.into(),
            hint: Some(hint.into()),
        }
    }
}

pub(crate) fn run_startup_preflight() -> Result<PreflightReport, PreflightError> {
    let mut failures = Vec::new();
    let mut warnings = Vec::new();

    #[cfg(not(target_os = "linux"))]
    {
        failures.push(FailedCheck::new(
            "Unsupported operating system.",
            "Run Symbolis on Linux. Native media delivery is being prepared for Wayland/X11 first.",
        ));
        return Err(PreflightError::new(failures));
    }

    #[cfg(target_os = "linux")]
    {
        let session = match detect_linux_session_from_env() {
            Ok(session) => Some(session),
            Err(checks) => {
                failures.extend(checks);
                None
            }
        };

        if let Err(err) = Clipboard::new() {
            failures.push(FailedCheck::new(
                format!("Clipboard backend is unavailable: {err}"),
                "Start Symbolis inside a graphical Wayland/X11 session with clipboard access. On minimal Wayland sessions, make sure the compositor supports data-control clipboard protocols.",
            ));
        }

        let drag_helper = detect_linux_drag_helper().map(Some).unwrap_or_else(|err| {
            warnings.push(StartupWarning::new(
                "Drag and drop",
                err.to_string(),
                err.install_hint(),
            ));
            None
        });

        let color_emoji_renderer = detect_color_emoji_renderer();
        if !color_emoji_renderer {
            warnings.push(StartupWarning::new(
                "Color emoji",
                "pango-view is missing; emoji will use the fallback text renderer.",
                "Install pango tools, usually packaged as pango, pango-utils, or libpango1.0-bin depending on the distribution.",
            ));
        }

        if !detect_media_transcoder() {
            warnings.push(StartupWarning::new(
                "Media conversion",
                "ffmpeg is missing; GIF/MP4/WebM conversion actions will be unavailable.",
                "Install ffmpeg. On Arch-based systems use `sudo pacman -S ffmpeg`; on Debian/Ubuntu use `sudo apt install ffmpeg`.",
            ));
        }

        if !command_in_path("curl") {
            warnings.push(StartupWarning::new(
                "Telegram sticker import",
                "curl is missing; Telegram sticker set imports will be unavailable.",
                "Install curl. On Arch-based systems use `sudo pacman -S curl`; on Debian/Ubuntu use `sudo apt install curl`.",
            ));
        }

        let Some(linux_session) = session else {
            return Err(PreflightError::new(failures));
        };

        match &linux_session {
            LinuxSession::Wayland { .. } => {
                warnings.push(StartupWarning::new(
                    "Incoming file drop",
                    "the current winit Wayland backend may not deliver files dropped from file managers into the app.",
                    "For Dolphin/Nautilus drag-in support, run with XWayland/X11 available or set SYMBOLIS_WINDOW_BACKEND=x11. Set SYMBOLIS_WINDOW_BACKEND=wayland only when native Wayland is preferred over file drop.",
                ));
            }
            LinuxSession::X11 { display } if desktop_session_is_wayland() => {
                let mut hint = "This is the default on Wayland when DISPLAY is available because XWayland currently gives more reliable file drops. Set SYMBOLIS_WINDOW_BACKEND=wayland to force native Wayland.".to_owned();
                if !x11_display_has_local_socket(display) {
                    hint.push_str(
                        " DISPLAY does not look like a local XWayland socket; if startup fails, install/enable XWayland or force native Wayland.",
                    );
                }
                warnings.push(StartupWarning::new(
                    "Window backend",
                    format!(
                        "using X11/XWayland backend on a Wayland desktop via DISPLAY={display}."
                    ),
                    hint,
                ));
            }
            LinuxSession::X11 { .. } => {}
        }

        if failures.is_empty() {
            Ok(PreflightReport {
                linux_session,
                drag_helper,
                color_emoji_renderer,
                warnings,
            })
        } else {
            Err(PreflightError::new(failures))
        }
    }
}

#[cfg(target_os = "linux")]
fn detect_linux_session_from_env() -> Result<LinuxSession, Vec<FailedCheck>> {
    let preference = linux_window_backend_preference_from_env().map_err(|check| vec![check])?;
    detect_linux_session(
        env::var("XDG_SESSION_TYPE").ok().as_deref(),
        env::var("WAYLAND_DISPLAY").ok().as_deref(),
        env::var("WAYLAND_SOCKET").ok().as_deref(),
        env::var("DISPLAY").ok().as_deref(),
        env::var("XDG_RUNTIME_DIR").ok().as_deref(),
        preference,
    )
}

#[cfg(target_os = "linux")]
fn detect_linux_session(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
    wayland_socket: Option<&str>,
    display: Option<&str>,
    xdg_runtime_dir: Option<&str>,
    preference: LinuxWindowBackendPreference,
) -> Result<LinuxSession, Vec<FailedCheck>> {
    let normalized_session = xdg_session_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    let wayland_display = wayland_display
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let wayland_socket = wayland_socket
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let display = display.map(str::trim).filter(|value| !value.is_empty());

    match preference {
        LinuxWindowBackendPreference::X11 => {
            if let Some(display) = display {
                return Ok(LinuxSession::X11 {
                    display: display.to_owned(),
                });
            }
            return Err(vec![FailedCheck::new(
                format!("{WINDOW_BACKEND_ENV}=x11, but DISPLAY is missing."),
                "Launch Symbolis from an X11/XWayland-capable session, or set SYMBOLIS_WINDOW_BACKEND=wayland.",
            )]);
        }
        LinuxWindowBackendPreference::Wayland => {
            return detect_wayland_session(wayland_display, wayland_socket, xdg_runtime_dir);
        }
        LinuxWindowBackendPreference::Auto => {
            if let Some(display) = display {
                return Ok(LinuxSession::X11 {
                    display: display.to_owned(),
                });
            }
            if wayland_display.is_some() || wayland_socket.is_some() {
                return detect_wayland_session(wayland_display, wayland_socket, xdg_runtime_dir);
            }
        }
    }

    let mut failures = vec![FailedCheck::new(
        "No usable graphical session was detected. WAYLAND_DISPLAY, WAYLAND_SOCKET, and DISPLAY are missing.",
        "Launch Symbolis from a running Wayland or X11 desktop session.",
    )];

    match normalized_session.as_deref() {
        Some("wayland") => failures.push(FailedCheck::new(
            "XDG_SESSION_TYPE says Wayland, but WAYLAND_DISPLAY is missing.",
            "Check that the app is not launched through a sanitized environment that strips Wayland variables.",
        )),
        Some("x11") => failures.push(FailedCheck::new(
            "XDG_SESSION_TYPE says X11, but DISPLAY is missing.",
            "Check that DISPLAY is exported, for example DISPLAY=:0 inside a normal X11 session.",
        )),
        Some(other) => failures.push(FailedCheck::without_hint(format!(
            "XDG_SESSION_TYPE is '{other}', which Symbolis does not recognize as Wayland or X11."
        ))),
        None => {}
    }

    Err(failures)
}

#[cfg(target_os = "linux")]
fn linux_window_backend_preference_from_env() -> Result<LinuxWindowBackendPreference, FailedCheck> {
    let Some(value) = env::var(WINDOW_BACKEND_ENV)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(LinuxWindowBackendPreference::Auto);
    };

    match value.as_str() {
        "auto" => Ok(LinuxWindowBackendPreference::Auto),
        "x11" | "x" => Ok(LinuxWindowBackendPreference::X11),
        "wayland" | "wl" => Ok(LinuxWindowBackendPreference::Wayland),
        other => Err(FailedCheck::new(
            format!("{WINDOW_BACKEND_ENV} has unsupported value '{other}'."),
            "Use SYMBOLIS_WINDOW_BACKEND=auto, x11, or wayland.",
        )),
    }
}

#[cfg(target_os = "linux")]
fn detect_wayland_session(
    wayland_display: Option<&str>,
    wayland_socket: Option<&str>,
    xdg_runtime_dir: Option<&str>,
) -> Result<LinuxSession, Vec<FailedCheck>> {
    let Some(display) = wayland_display.or(wayland_socket) else {
        return Err(vec![FailedCheck::new(
            "Wayland backend was requested, but WAYLAND_DISPLAY and WAYLAND_SOCKET are missing.",
            "Launch Symbolis from a Wayland session, or set SYMBOLIS_WINDOW_BACKEND=x11 when DISPLAY is available.",
        )]);
    };

    let Some(runtime_dir) = xdg_runtime_dir
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    else {
        return Err(vec![FailedCheck::new(
            "Wayland backend was selected, but XDG_RUNTIME_DIR is missing.",
            "Start Symbolis from your desktop session, not from a stripped environment. XDG_RUNTIME_DIR should point to the active user runtime directory.",
        )]);
    };

    if !runtime_dir_is_usable(&runtime_dir) {
        return Err(vec![FailedCheck::new(
            format!(
                "XDG_RUNTIME_DIR does not exist or is not a directory: {}",
                runtime_dir.display()
            ),
            "Fix the session environment. On systemd-based desktops this is normally created automatically under /run/user/<uid>.",
        )]);
    }

    Ok(LinuxSession::Wayland {
        display: display.to_owned(),
        runtime_dir,
    })
}

#[cfg(target_os = "linux")]
fn runtime_dir_is_usable(path: &Path) -> bool {
    path.is_dir()
}

#[cfg(target_os = "linux")]
fn desktop_session_is_wayland() -> bool {
    env::var("XDG_SESSION_TYPE")
        .ok()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("wayland"))
}

#[cfg(target_os = "linux")]
fn x11_display_has_local_socket(display: &str) -> bool {
    x11_display_number(display)
        .map(|number| {
            Path::new("/tmp/.X11-unix")
                .join(format!("X{number}"))
                .exists()
        })
        .unwrap_or(true)
}

#[cfg(target_os = "linux")]
fn x11_display_number(display: &str) -> Option<&str> {
    let rest = display.strip_prefix(':')?;
    let number = rest
        .split(['.', ' '])
        .next()
        .filter(|value| !value.is_empty())?;
    number
        .chars()
        .all(|ch| ch.is_ascii_digit())
        .then_some(number)
}

#[cfg(target_os = "linux")]
fn command_in_path(name: &str) -> bool {
    env::var_os("PATH")
        .map(|path| {
            path.to_string_lossy()
                .split(':')
                .filter(|dir| !dir.is_empty())
                .map(|dir| Path::new(dir).join(name))
                .any(|path| executable_file(&path))
        })
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn executable_file(path: &Path) -> bool {
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

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;

    #[test]
    fn detects_x11_from_display() {
        assert_eq!(
            detect_linux_session(
                Some("x11"),
                None,
                None,
                Some(":0"),
                None,
                LinuxWindowBackendPreference::Auto
            )
            .unwrap(),
            LinuxSession::X11 {
                display: ":0".to_owned()
            }
        );
    }

    #[test]
    fn auto_prefers_x11_when_both_linux_backends_exist() {
        assert!(matches!(
            detect_linux_session(
                Some("wayland"),
                Some("wayland-0"),
                None,
                Some(":0"),
                Some("/tmp"),
                LinuxWindowBackendPreference::Auto
            )
            .unwrap(),
            LinuxSession::X11 { .. }
        ));
    }

    #[test]
    fn explicit_wayland_keeps_wayland_when_display_also_exists() {
        assert!(matches!(
            detect_linux_session(
                Some("wayland"),
                Some("wayland-0"),
                None,
                Some(":0"),
                Some("/tmp"),
                LinuxWindowBackendPreference::Wayland
            )
            .unwrap(),
            LinuxSession::Wayland { .. }
        ));
    }

    #[test]
    fn rejects_missing_display() {
        let err = detect_linux_session(
            Some("x11"),
            None,
            None,
            None,
            None,
            LinuxWindowBackendPreference::X11,
        )
        .unwrap_err();
        assert!(err.iter().any(|check| check.message.contains("DISPLAY")));
    }

    #[test]
    fn parses_x11_display_numbers_for_local_socket_checks() {
        assert_eq!(x11_display_number(":0"), Some("0"));
        assert_eq!(x11_display_number(":1.0"), Some("1"));
        assert_eq!(x11_display_number("hostname:0"), None);
    }
}
