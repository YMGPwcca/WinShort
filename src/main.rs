//! WinShort — native Windows tray utility.
//!
//! Audio hotkeys, status overlay, virtual desktop switching. Pure Rust
//! against Win32/COM. See docs/ARCHITECTURE.md.

// GUI subsystem in release: no console window (spec §67.2). Debug keeps the
// console for `cargo run` diagnostics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Unsafe hygiene (#25): inside `unsafe fn` bodies every operation must be
// explicitly re-wrapped and justified — no implicit blanket unsafety.
#![deny(unsafe_op_in_unsafe_fn)]
#[macro_use]
mod diagnostics;

mod app;
mod audio;
mod config;
mod desktop;
mod error;
mod event;
mod keyboard;
mod platform;
mod tray;
mod ui;

use std::path::PathBuf;

use crate::error::Result;
use crate::platform::single_instance::{InstanceRole, PrimaryRole};

/// Process data root: %LOCALAPPDATA%\WinShort.
fn app_data_dir() -> Result<PathBuf> {
    crate::config::try_data_dir().map_err(|e| e)
}

fn init_logging() -> Result<()> {
    let dir = app_data_dir()?.join("logs");
    diagnostics::logging::init(&dir, diagnostics::logging::Level::Debug);
    // Bias is minutes WEST of UTC (documented sign); we need seconds east.
    let bias = unsafe { tz_bias_minutes() };
    diagnostics::logging::set_local_offset(-(bias as i64) * 60);
    Ok(())
}

unsafe fn tz_bias_minutes() -> i32 {
    use windows::Win32::System::Time::{
        GetDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION,
    };
    // SAFETY: plain out-parameter query with no side effects.
    unsafe {
        let mut tz = DYNAMIC_TIME_ZONE_INFORMATION::default();
        GetDynamicTimeZoneInformation(&mut tz);
        tz.Bias
    }
}

fn main() -> std::process::ExitCode {
    // ---- Single instance (spec §7) -------------------------------------
    let role = match platform::single_instance::acquire() {
        Ok(InstanceRole::Secondary) => {
            info!("another instance running; activation requested");
            return std::process::ExitCode::SUCCESS;
        }
        Ok(InstanceRole::Primary(primary)) => primary,
        Err(e) => {
            eprintln!("winshort: single-instance check failed: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    // ---- DPI awareness before any window -------------------------------
    platform::dpi::set_process_awareness();
    if let Err(e) = init_logging() {
        // Logging is best effort, but a missing data dir means config cannot
        // be persisted either — treat as fatal with a visible reason (#15e).
        error_!("fatal: {e}");
        fatal_message_box(&e.to_string());
        return std::process::ExitCode::FAILURE;
    }
    info!("WinShort starting");

    // ---- Configuration (spec §46: load/validate before UI) ---------------
    let data_dir = match app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            error_!("fatal: {e}");
            fatal_message_box(&e.to_string());
            return std::process::ExitCode::FAILURE;
        }
    };
    let (cfg, warnings) = config::load::load(&data_dir);
    for w in &warnings {
        warn_!("config warning: {w}");
    }
    let handle = std::sync::Arc::new(config::ConfigHandle::new(cfg));
    let _ = app::CONFIG.set(handle);
    info!("config loaded");

    // ---- COM for the UI thread (WIC, shell): required, not optional (#24).
    let com = platform::com::ComApartment::init_sta();
    if !com.ok() {
        error_!("CoInitializeEx failed on the UI thread; COM-backed subsystems cannot start");
    }

    let run_result = run(role, com);
    if let Err(e) = run_result {
        error_!("fatal: {e}");
        fatal_message_box(&e.to_string());
        return std::process::ExitCode::FAILURE;
    }

    info!("exit");
    std::process::ExitCode::SUCCESS
}

fn run(role: PrimaryRole, _com: crate::platform::com::ComApartment) -> Result<()> {
    // Hidden message-only main window + App singleton.
    app::App::create_main_window()?;
    let hwnd_raw = app::main_hwnd()
        .ok_or_else(|| error::Error::internal("main window missing"))?
        .0 as isize;
    let config = app::CONFIG
        .get()
        .cloned()
        .ok_or_else(|| error::Error::internal("config handle missing"))?;

    // Degraded startup (#27): each subsystem failure is recorded and logged;
    // only the main window itself is fatal. Settings/tray still come up.
    let install_errors = app::with_app(|app| -> Vec<String> {
        let mut failures = Vec::new();
        macro_rules! step {
            ($name:literal, $expr:expr) => {
                if let Err(e) = $expr {
                    app.degrade($name, &e);
                    failures.push(format!("{}: {}", $name, e));
                }
            };
        }
        step!("tray", app.install_tray());
        step!("overlay", app.install_overlay());
        step!("foreground", app.install_foreground_tracker());
        step!("audio", app.install_audio(config.clone()));
        step!("keyboard", app.install_keyboard(config));
        step!("desktop", app.install_desktop());
        failures
    })
    .unwrap_or_default();
    for f in &install_errors {
        error_!("degraded startup: {f}");
    }

    // Owned watcher runtime (#24): joined after the loop exits; begin_shutdown
    // signals its shutdown event BEFORE destroying any window.
    let mut watcher =
        platform::single_instance::spawn_watcher(role.activate_event, move || unsafe {
            let hwnd = windows::Win32::Foundation::HWND(hwnd_raw as *mut _);
            unsafe {
                event::post_event(hwnd, event::AppEvent::ShowSettings);
            }
        });

    info!("startup complete; entering message loop");
    let code = platform::message_loop::run();
    info!("message loop exited (code {code})");
    watcher.join();
    Ok(())
}

/// Best-effort user-visible failure before/around startup.
fn fatal_message_box(text: &str) {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(HSTRING::from(format!("WinShort failed to start:\n{text}")).as_ptr()),
            PCWSTR(HSTRING::from("WinShort").as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}
