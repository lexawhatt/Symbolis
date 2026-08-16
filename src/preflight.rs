use std::{
    env, fmt,
    path::{Path, PathBuf},
};

use arboard::Clipboard;

use crate::media_drag::{LinuxDragHelper, detect_linux_drag_helper};

#[derive(Clone, Debug)]
pub(crate) struct PreflightReport {
    pub(crate) linux_session: LinuxSession,
    pub(crate) drag_helper: LinuxDragHelper,
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

pub(crate) fn run_startup_preflight() -> Result<PreflightReport, PreflightError> {
    let mut failures = Vec::new();

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

        let drag_helper = match detect_linux_drag_helper() {
            Ok(helper) => Some(helper),
            Err(err) => {
                failures.push(FailedCheck::new(err.to_string(), err.install_hint()));
                None
            }
        };

        let Some(linux_session) = session else {
            return Err(PreflightError::new(failures));
        };

        let Some(drag_helper) = drag_helper else {
            return Err(PreflightError::new(failures));
        };

        if failures.is_empty() {
            Ok(PreflightReport {
                linux_session,
                drag_helper,
            })
        } else {
            Err(PreflightError::new(failures))
        }
    }
}

#[cfg(target_os = "linux")]
fn detect_linux_session_from_env() -> Result<LinuxSession, Vec<FailedCheck>> {
    detect_linux_session(
        env::var("XDG_SESSION_TYPE").ok().as_deref(),
        env::var("WAYLAND_DISPLAY").ok().as_deref(),
        env::var("DISPLAY").ok().as_deref(),
        env::var("XDG_RUNTIME_DIR").ok().as_deref(),
    )
}

#[cfg(target_os = "linux")]
fn detect_linux_session(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
    xdg_runtime_dir: Option<&str>,
) -> Result<LinuxSession, Vec<FailedCheck>> {
    let normalized_session = xdg_session_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    let wayland_display = wayland_display
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let display = display.map(str::trim).filter(|value| !value.is_empty());

    if let Some(display) = wayland_display {
        let Some(runtime_dir) = xdg_runtime_dir
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
        else {
            return Err(vec![FailedCheck::new(
                "WAYLAND_DISPLAY is set, but XDG_RUNTIME_DIR is missing.",
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

        return Ok(LinuxSession::Wayland {
            display: display.to_owned(),
            runtime_dir,
        });
    }

    if let Some(display) = display {
        return Ok(LinuxSession::X11 {
            display: display.to_owned(),
        });
    }

    let mut failures = vec![FailedCheck::new(
        "No usable graphical session was detected. WAYLAND_DISPLAY and DISPLAY are both missing.",
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
fn runtime_dir_is_usable(path: &Path) -> bool {
    path.is_dir()
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;

    #[test]
    fn detects_x11_from_display() {
        assert_eq!(
            detect_linux_session(Some("x11"), None, Some(":0"), None).unwrap(),
            LinuxSession::X11 {
                display: ":0".to_owned()
            }
        );
    }

    #[test]
    fn rejects_missing_display() {
        let err = detect_linux_session(Some("x11"), None, None, None).unwrap_err();
        assert!(err.iter().any(|check| check.message.contains("DISPLAY")));
    }
}
