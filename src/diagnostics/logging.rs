//! Lightweight buffered daily logging to `%LOCALAPPDATA%\WinShort\logs\`.
//!
//! Normal records are buffered and flushed on warning/error, a bounded main
//! window timer, support-bundle/open-log actions, and orderly shutdown. Panic
//! records use an independent synchronous append path because release builds
//! abort and normal destructors are not guaranteed to run.

use std::fmt::Arguments;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use windows::Win32::Foundation::{HWND, SYSTEMTIME};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

pub const LOG_RETENTION_DAYS: i64 = 14;
pub const FLUSH_TIMER_ID: usize = 0x574C;
pub const FLUSH_TIMER_MS: u32 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    const fn as_u8(self) -> u8 {
        match self {
            Self::Debug => 0,
            Self::Info => 1,
            Self::Warn => 2,
            Self::Error => 3,
        }
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Debug,
            2 => Self::Warn,
            3 => Self::Error,
            _ => Self::Info,
        }
    }
}

pub const fn default_level_for_build(debug_assertions: bool) -> Level {
    if debug_assertions {
        Level::Debug
    } else {
        Level::Info
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CivilDate {
    year: i32,
    month: u32,
    day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LocalDateTime {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millisecond: u32,
}

impl LocalDateTime {
    const fn date(self) -> CivilDate {
        CivilDate {
            year: self.year,
            month: self.month,
            day: self.day,
        }
    }
}

#[derive(Default)]
struct LogFileState {
    day: Option<CivilDate>,
    file: Option<BufWriter<File>>,
    dirty: bool,
}

struct Logger {
    state: Mutex<LogFileState>,
    level: AtomicU8,
    default_level: Level,
    temporary_debug: AtomicBool,
    dir: Option<PathBuf>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Read-only logger metadata used by the Diagnostics surface.
#[derive(Debug, Clone)]
pub struct LoggerInfo {
    pub directory: Option<PathBuf>,
    pub current_file: Option<PathBuf>,
    pub level: Level,
    pub default_level: Level,
    pub temporary_debug: bool,
    pub retention_days: i64,
    pub buffered: bool,
}

/// RAII lifecycle guard. Dropping it flushes the active buffer.
pub struct LoggingGuard;

impl Drop for LoggingGuard {
    fn drop(&mut self) {
        flush();
    }
}

pub fn guard() -> LoggingGuard {
    LoggingGuard
}

pub fn info() -> Option<LoggerInfo> {
    LOGGER.get().map(|logger| LoggerInfo {
        directory: logger.dir.clone(),
        current_file: current_file_for(logger),
        level: current_level(logger),
        default_level: logger.default_level,
        temporary_debug: logger.temporary_debug.load(Ordering::Acquire),
        retention_days: LOG_RETENTION_DAYS,
        buffered: true,
    })
}

pub fn current_log_path() -> Option<PathBuf> {
    LOGGER.get().and_then(current_file_for)
}

/// Initialize logging. Creates the directory and performs one bounded cleanup;
/// if the filesystem is unavailable the process still runs with no file logs.
pub fn init(logs_dir: &Path, level: Level) {
    let dir = std::fs::create_dir_all(logs_dir)
        .is_ok()
        .then(|| logs_dir.to_path_buf());
    if let Some(dir) = &dir {
        cleanup_retention(dir, local_now().date());
    }
    let _ = LOGGER.set(Logger {
        state: Mutex::new(LogFileState::default()),
        level: AtomicU8::new(level.as_u8()),
        default_level: level,
        temporary_debug: AtomicBool::new(false),
        dir,
    });
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        write_panic_record(panic_info);
        previous(panic_info);
    }));
}

pub fn debug_logging_enabled() -> bool {
    LOGGER
        .get()
        .is_some_and(|logger| current_level(logger) == Level::Debug)
}

pub fn set_debug_logging(enabled: bool) {
    let Some(logger) = LOGGER.get() else { return };
    let target = if enabled { Level::Debug } else { Level::Info };
    let old = current_level(logger);
    if enabled {
        logger.level.store(target.as_u8(), Ordering::Release);
        logger.temporary_debug.store(
            target == Level::Debug && logger.default_level != Level::Debug,
            Ordering::Release,
        );
    } else if old != target {
        // Write before lowering the threshold so the transition is retained.
        log(Level::Info, format_args!("log level changed to INFO"));
        logger.level.store(target.as_u8(), Ordering::Release);
        logger.temporary_debug.store(false, Ordering::Release);
    } else {
        logger.temporary_debug.store(false, Ordering::Release);
    }
    if old != target && enabled {
        log(
            Level::Info,
            format_args!("log level changed to DEBUG (temporary)"),
        );
    }
}

pub fn flush_if_dirty() {
    let Some(logger) = LOGGER.get() else { return };
    let mut state = lock_state(logger);
    if state.dirty {
        let _ = flush_state(&mut state);
    }
}

pub fn flush() {
    let Some(logger) = LOGGER.get() else { return };
    let mut state = lock_state(logger);
    let _ = flush_state(&mut state);
}

pub fn start_flush_timer(hwnd: HWND) {
    unsafe {
        let _ = SetTimer(Some(hwnd), FLUSH_TIMER_ID, FLUSH_TIMER_MS, None);
    }
}

pub fn stop_flush_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), FLUSH_TIMER_ID);
    }
}

pub fn log(level: Level, args: Arguments<'_>) {
    let Some(logger) = LOGGER.get() else { return };
    if level < current_level(logger) {
        return;
    }
    let now = local_now();
    let mut state = lock_state(logger);
    write_record(&mut state, logger.dir.as_deref(), now, level, args);
}

fn current_level(logger: &Logger) -> Level {
    Level::from_u8(logger.level.load(Ordering::Acquire))
}

fn lock_state(logger: &Logger) -> std::sync::MutexGuard<'_, LogFileState> {
    logger
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn current_file_for(logger: &Logger) -> Option<PathBuf> {
    let dir = logger.dir.as_ref()?;
    let state = lock_state(logger);
    let day = state.day.unwrap_or_else(|| local_now().date());
    Some(path_for(dir, day))
}

fn write_record(
    state: &mut LogFileState,
    dir: Option<&Path>,
    now: LocalDateTime,
    level: Level,
    args: Arguments<'_>,
) {
    let day = now.date();
    if state.day != Some(day) {
        if state.day.is_some() {
            let _ = flush_state(state);
            state.file = None;
            if let Some(dir) = dir {
                cleanup_retention(dir, day);
            }
        }
        state.day = Some(day);
        state.file = dir.and_then(|dir| open_day_file(dir, day));
    }
    if let Some(file) = state.file.as_mut() {
        if writeln!(
            file,
            "{} [{}] {}",
            format_timestamp(now),
            level.as_str(),
            args
        )
        .is_ok()
        {
            state.dirty = true;
        }
    }
    if level >= Level::Warn {
        let _ = flush_state(state);
    }
}

fn open_day_file(dir: &Path, day: CivilDate) -> Option<BufWriter<File>> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path_for(dir, day))
        .ok()
        .map(BufWriter::new)
}

fn flush_state(state: &mut LogFileState) -> bool {
    let Some(file) = state.file.as_mut() else {
        state.dirty = false;
        return true;
    };
    match file.flush() {
        Ok(()) => {
            state.dirty = false;
            true
        }
        Err(_) => false,
    }
}

fn write_panic_record(panic_info: &std::panic::PanicHookInfo<'_>) {
    let Some(logger) = LOGGER.get() else { return };
    let now = local_now();
    if let Ok(mut state) = logger.state.try_lock() {
        let _ = flush_state(&mut state);
    }
    let Some(dir) = logger.dir.as_deref() else {
        return;
    };
    let path = path_for(dir, now.date());
    let payload = panic_info
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| {
            panic_info
                .payload()
                .downcast_ref::<String>()
                .map(String::as_str)
        })
        .unwrap_or("<non-string panic payload>");
    let current_thread = thread::current();
    let thread_name = current_thread.name().unwrap_or("<unnamed>");
    let location = panic_info.location().map_or_else(
        || "<unknown>".to_string(),
        |value| format!("{}:{}:{}", value.file(), value.line(), value.column()),
    );
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(
        file,
        "{} [PANIC] thread={} payload={} location={}",
        format_timestamp(now),
        thread_name,
        payload,
        location
    );
    let _ = file.flush();
}

fn local_now() -> LocalDateTime {
    let value: SYSTEMTIME = unsafe { GetLocalTime() };
    LocalDateTime {
        year: value.wYear as i32,
        month: value.wMonth as u32,
        day: value.wDay as u32,
        hour: value.wHour as u32,
        minute: value.wMinute as u32,
        second: value.wSecond as u32,
        millisecond: value.wMilliseconds as u32,
    }
}

fn format_day_stamp(day: CivilDate) -> String {
    format!("{:04}{:02}{:02}", day.year, day.month, day.day)
}

fn format_timestamp(value: LocalDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        value.year,
        value.month,
        value.day,
        value.hour,
        value.minute,
        value.second,
        value.millisecond
    )
}

fn path_for(dir: &Path, day: CivilDate) -> PathBuf {
    dir.join(format!("winshort-{}.log", format_day_stamp(day)))
}

fn parse_daily_file(name: &str) -> Option<CivilDate> {
    let digits = name.strip_prefix("winshort-")?.strip_suffix(".log")?;
    if digits.len() != 8 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year = digits[0..4].parse::<i32>().ok()?;
    let month = digits[4..6].parse::<u32>().ok()?;
    let day = digits[6..8].parse::<u32>().ok()?;
    let date = CivilDate { year, month, day };
    if !(1..=12).contains(&month) || day == 0 || day > 31 {
        return None;
    }
    let round_trip = civil_from_days(days_from_civil(date));
    (round_trip == date).then_some(date)
}

fn cleanup_retention(dir: &Path, today: CivilDate) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let today_days = days_from_civil(today);
    let cutoff = today_days - LOG_RETENTION_DAYS;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(day) = parse_daily_file(&name) else {
            continue;
        };
        let file_days = days_from_civil(day);
        if file_days <= cutoff && file_days <= today_days {
            if let Err(error) = fs::remove_file(&path) {
                eprintln!(
                    "winshort logging: retention remove {} failed: {error}",
                    path.display()
                );
            }
        }
    }
}

fn days_from_civil(date: CivilDate) -> i64 {
    let y = date.year - i32::from(date.month <= 2);
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let month = date.month as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + date.day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146_097 + doe as i64 - 719_468
}

/// Howard Hinnant's civil_from_days algorithm.
fn civil_from_days(z: i64) -> CivilDate {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    CivilDate {
        year: (if month <= 2 { y + 1 } else { y }) as i32,
        month,
        day,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn local(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        ms: u32,
    ) -> LocalDateTime {
        LocalDateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond: ms,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("winshort-logging-{name}-{}", std::process::id()))
    }

    #[test]
    fn level_order_and_build_defaults_are_explicit() {
        assert!(Level::Debug < Level::Info);
        assert!(Level::Info < Level::Warn);
        assert!(Level::Warn < Level::Error);
        assert_eq!(default_level_for_build(false), Level::Info);
        assert_eq!(default_level_for_build(true), Level::Debug);
        assert_eq!(Level::from_u8(Level::Error.as_u8()), Level::Error);
    }

    #[test]
    fn local_formatting_uses_one_clock_snapshot() {
        let first = local(2026, 1, 1, 0, 0, 0, 1);
        assert_eq!(format_day_stamp(first.date()), "20260101");
        assert_eq!(format_timestamp(first), "2026-01-01 00:00:00.001");
        let end = local(2026, 12, 31, 23, 59, 59, 999);
        assert_eq!(format_day_stamp(end.date()), "20261231");
        assert_eq!(format_timestamp(end), "2026-12-31 23:59:59.999");
        assert_eq!(
            days_from_civil(local(2024, 2, 29, 0, 0, 0, 0).date()),
            19_782
        );
        assert_eq!(civil_from_days(days_from_civil(end.date())), end.date());
    }

    #[test]
    fn retention_deletes_only_old_exact_daily_files() {
        let dir = temp_dir("retention");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let today = CivilDate {
            year: 2026,
            month: 1,
            day: 15,
        };
        for name in [
            "winshort-20260101.log",
            "winshort-20260102.log",
            "winshort-20260129.log",
            "winshort-20260101.log.bak",
            "other-20251201.log",
            "winshort-20251301.log",
        ] {
            fs::write(dir.join(name), "x").unwrap();
        }
        cleanup_retention(&dir, today);
        assert!(!dir.join("winshort-20260101.log").exists());
        assert!(dir.join("winshort-20260102.log").exists());
        assert!(dir.join("winshort-20260129.log").exists());
        assert!(dir.join("winshort-20260101.log.bak").exists());
        assert!(dir.join("other-20251201.log").exists());
        assert!(dir.join("winshort-20251301.log").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn buffered_records_flush_at_explicit_policy_points() {
        let dir = temp_dir("buffer");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let day = local(2026, 1, 1, 12, 0, 0, 1);
        let path = path_for(&dir, day.date());
        let mut state = LogFileState::default();
        write_record(
            &mut state,
            Some(&dir),
            day,
            Level::Info,
            format_args!("buffered"),
        );
        assert!(state.dirty);
        assert!(fs::read_to_string(&path).unwrap().is_empty());
        assert!(flush_state(&mut state));
        assert!(fs::read_to_string(&path).unwrap().contains("buffered"));
        write_record(
            &mut state,
            Some(&dir),
            day,
            Level::Warn,
            format_args!("warning"),
        );
        assert!(!state.dirty);
        assert!(fs::read_to_string(&path).unwrap().contains("warning"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn panic_child_writes_emergency_record() {
        let dir = temp_dir("panic");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "diagnostics::logging::tests::panic_child",
                "--nocapture",
            ])
            .env("WINSHORT_PANIC_LOG_DIR", &dir)
            .status()
            .unwrap();
        assert!(!status.success());
        let path = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| parse_daily_file(name).is_some())
            })
            .expect("panic daily log");
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("[PANIC]"));
        assert!(content.contains("thread="));
        assert!(content.contains("deliberate diagnostics panic"));
        assert!(content.contains("panic_child"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn panic_child() {
        let Ok(dir) = std::env::var("WINSHORT_PANIC_LOG_DIR") else {
            return;
        };
        init(Path::new(&dir), Level::Info);
        install_panic_hook();
        panic!("deliberate diagnostics panic");
    }

    #[test]
    fn disabled_before_init_is_non_panicking() {
        let _ = debug_logging_enabled();
        log(Level::Info, format_args!("test message {}", 42));
    }
}
