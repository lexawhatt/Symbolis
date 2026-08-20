use std::sync::OnceLock;

pub(crate) const LOG_LEVEL_ENV: &str = "SYMBOLIS_LOG_LEVEL";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum LogLevel {
    Important = 0,
    Info = 1,
    Trace = 2,
}

impl LogLevel {
    fn from_value(value: &str) -> Option<Self> {
        match value.trim() {
            "0" | "important" | "warn" | "error" => Some(Self::Important),
            "1" | "info" => Some(Self::Info),
            "2" | "trace" | "debug" | "all" => Some(Self::Trace),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Important => "important",
            Self::Info => "info",
            Self::Trace => "trace",
        }
    }
}

pub(crate) fn configured_log_level() -> LogLevel {
    static LOG_LEVEL: OnceLock<LogLevel> = OnceLock::new();
    *LOG_LEVEL.get_or_init(|| {
        log_level_from_args()
            .or_else(log_level_from_env)
            .unwrap_or(LogLevel::Important)
    })
}

pub(crate) fn log(level: LogLevel, message: impl AsRef<str>) {
    if level <= configured_log_level() {
        eprintln!("[symbolis][{}] {}", level.label(), message.as_ref());
    }
}

fn log_level_from_args() -> Option<LogLevel> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--log-level=") {
            return LogLevel::from_value(value);
        }
        if arg == "--log-level" {
            return args.next().and_then(|value| LogLevel::from_value(&value));
        }
    }
    None
}

fn log_level_from_env() -> Option<LogLevel> {
    std::env::var(LOG_LEVEL_ENV)
        .ok()
        .and_then(|value| LogLevel::from_value(&value))
}
