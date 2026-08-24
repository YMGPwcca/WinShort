//! Lightweight append-only file logging to `%LOCALAPPDATA%\WinShort\logs\`.
//!
//! No third-party dependencies. One file per local date (`winshort-YYYYMMDD.log`),
//! best-effort: logging never panics and never blocks meaningfully (single short
//! write behind a mutex). Privacy rule: the keyboard engine must not log per
//! keystroke; only recognized bindings at diagnostic level.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO ",
            Level::Warn => "WARN ",
            Level::Error => "ERROR",
        }
    }
}

struct Logger {
    state: Mutex<Option<(String, File)>>, // (day stamp YYYYMMDD, open handle)
    level: Level,
    dir: Option<PathBuf>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Initialize logging. Creates the directory if possible; if the filesystem is
/// unavailable the process still runs (logs go nowhere).
pub fn init(logs_dir: &Path, level: Level) {
    let dir = std::fs::create_dir_all(logs_dir)
        .is_ok()
        .then(|| logs_dir.to_path_buf());
    let _ = LOGGER.set(Logger {
        level,
        dir,
        state: Mutex::new(None),
    });
}

#[cfg(test)]
pub fn enabled(level: Level) -> bool {
    LOGGER.get().is_some_and(|l| level >= l.level)
}

pub fn log(level: Level, args: std::fmt::Arguments<'_>) {
    let Some(logger) = LOGGER.get() else { return };
    if level < logger.level {
        return;
    }
    let today = today_stamp();
    let mut guard = match logger.state.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let needs_open = match guard.as_ref() {
        Some((day, _)) => *day != today,
        None => true,
    };
    if needs_open {
        if let Some(dir) = &logger.dir {
            *guard = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path_for(dir, &today))
                .ok()
                .map(|f| (today, f));
        }
    }
    if let Some((_, f)) = guard.as_mut() {
        let _ = writeln!(f, "{} [{}] {}", timestamp(), level.as_str(), args);
        let _ = f.flush();
    }
}

fn path_for(dir: &Path, stamp: &str) -> PathBuf {
    dir.join(format!("winshort-{stamp}.log"))
}

fn now_parts() -> (u64, u32) {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => (d.as_secs(), d.subsec_millis()),
        Err(_) => (0, 0),
    }
}
fn today_stamp() -> String {
    // Local date: UTC seconds adjusted by the startup-queried zone bias.
    let (secs, _) = now_parts();
    let days = (secs as i64 + local_utc_offset_secs()) / 86_400;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}")
}

/// Timezone offset in seconds east of UTC for the current instant.
/// The app sets this once at startup from `GetTimeZoneInformation`;
/// tests run on UTC (offset 0).
static LOCAL_OFFSET: Mutex<i64> = Mutex::new(0);

pub fn set_local_offset(secs_east_of_utc: i64) {
    if let Ok(mut o) = LOCAL_OFFSET.lock() {
        *o = secs_east_of_utc;
    }
}

fn local_utc_offset_secs() -> i64 {
    LOCAL_OFFSET.lock().map(|o| *o).unwrap_or(0)
}

fn timestamp() -> String {
    let (secs, ms) = now_parts();
    let rem = (secs as i64 + local_utc_offset_secs()).rem_euclid(86_400) as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60,
        ms
    )
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => { $crate::diagnostics::logging::log($crate::diagnostics::logging::Level::Debug, format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => { $crate::diagnostics::logging::log($crate::diagnostics::logging::Level::Info, format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! warn_ {
    ($($arg:tt)*) => { $crate::diagnostics::logging::log($crate::diagnostics::logging::Level::Warn, format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! error_ {
    ($($arg:tt)*) => { $crate::diagnostics::logging::log($crate::diagnostics::logging::Level::Error, format_args!($($arg)*)) };
}
/// Howard Hinnant's civil_from_days algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }

    #[test]
    fn disabled_before_init() {
        // LOGGER may already be initialized by another test in this binary;
        // either way these must not panic.
        let _ = enabled(Level::Info);
        log(Level::Info, format_args!("test message {}", 42));
    }
}
