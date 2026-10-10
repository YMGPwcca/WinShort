//! Opt-in visual/geometry checks using test-owned windows and synthetic profiles.
//! No app singleton, audio worker, display apply, live config write or clipboard action.

use super::*;
use std::os::windows::process::CommandExt;
use std::time::{Duration, Instant};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, DispatchMessageW, GetWindowRect, PeekMessageW, SetWindowPos, ShowWindow,
    TranslateMessage, HWND_TOPMOST, MSG, PM_REMOVE, SWP_NOACTIVATE, SW_SHOWNOACTIVATE,
};

fn fixture_dir() -> std::path::PathBuf {
    std::env::current_dir()
        .unwrap()
        .join("target/ui-fix-review/data")
}

fn pump() {
    let until = Instant::now() + Duration::from_millis(160);
    while Instant::now() < until {
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn capture(hwnd: HWND, name: &str) {
    let mut rect = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut rect).unwrap();
    }
    let root = std::env::current_dir()
        .unwrap()
        .join("target/ui-fix-review");
    std::fs::create_dir_all(&root).unwrap();
    let path = root
        .join(format!("{name}.png"))
        .to_string_lossy()
        .replace('\'', "''");
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let handle = hwnd.0 as isize;
    let expected = if std::env::var("WINSHORT_UI_ACCEPTANCE_THEME").as_deref() == Ok("light") {
        238
    } else {
        36
    };
    let script = format!(
        r#"Add-Type -AssemblyName System.Drawing; Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class OwnWindowCapture {{ [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr dc, uint flags); }}'; $b = New-Object System.Drawing.Bitmap({width},{height}); $g = [System.Drawing.Graphics]::FromImage($b); $dc=$g.GetHdc(); $ok=[OwnWindowCapture]::PrintWindow([IntPtr]{handle},$dc,2); $g.ReleaseHdc($dc); $pixel=$b.GetPixel(8,100); if(!$ok -or $pixel.R -ne {expected} -or $pixel.G -ne {expected} -or $pixel.B -ne {expected}) {{ throw 'Test window pixels unavailable' }}; $b.Save('{path}',[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $b.Dispose()"#
    );
    let mut child = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Own-window capture timed out");
        }
        pump();
    }
}

#[test]
fn native_control_center_exposes_winshort_icons_to_the_shell() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, ICON_BIG, ICON_SMALL, WM_GETICON};
    let _com = crate::platform::com::ComApartment::init_sta();
    let ui = empty_settings_ui();
    let window = ControlCenterWindow::create(
        ui.devices,
        super::super::config_access::ConfigAccess::unavailable(),
        super::super::config_access::ControlCenterAccess::for_test_data_dir(fixture_dir),
    )
    .unwrap();
    struct OwnedWindow(HWND);
    impl Drop for OwnedWindow {
        fn drop(&mut self) {
            // SAFETY: this test owns the hidden HWND on its creating thread.
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }
    let _owned = OwnedWindow(window.hwnd);
    for kind in [ICON_SMALL, ICON_BIG] {
        // SAFETY: the hidden window remains alive, and WM_GETICON takes only
        // integer parameters and returns a borrowed process-lifetime HICON.
        let icon = unsafe {
            SendMessageW(
                window.hwnd,
                WM_GETICON,
                Some(WPARAM(kind as usize)),
                Some(LPARAM(0)),
            )
        };
        assert_ne!(
            icon.0, 0,
            "Shell preview needs the app's small and large icons"
        );
    }
}

#[test]
#[ignore = "shows test-owned native Control Center windows for visual inspection"]
fn native_ui_review_capture() {
    crate::platform::dpi::set_process_awareness();
    let _com = crate::platform::com::ComApartment::init_sta();
    let ui = empty_settings_ui();
    let window = ControlCenterWindow::create(
        ui.devices.clone(),
        super::super::config_access::ConfigAccess::unavailable(),
        super::super::config_access::ControlCenterAccess::for_test_data_dir(fixture_dir),
    )
    .unwrap();
    struct OwnedWindow(HWND);
    impl Drop for OwnedWindow {
        fn drop(&mut self) {
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }
    let _owned = OwnedWindow(window.hwnd);
    let cell = unsafe { crate::platform::window::state_cell::<SettingsUi>(window.hwnd) }.unwrap();
    {
        let mut state = cell.borrow_mut();
        state.onboarding_step = None;
        state.draft.display_profiles.enabled = true;
        state.draft.display_profiles.profiles = vec![
            sample_profile("desk", "Desk setup", true),
            sample_profile("presentation", "Presentation", true),
            sample_profile("portable", "Laptop only", false),
        ];
        state.draft.display_profiles.active_profile = Some("desk".into());
        state.inventory = DisplayInventory::Available(
            state.draft.display_profiles.profiles[..2]
                .iter()
                .map(|profile| crate::display::DisplayOutput {
                    route: profile.routes[0].clone(),
                    monitor_name: profile.name.clone(),
                    adapter_name: "Test adapter".into(),
                    connector_name: "HDMI".into(),
                    active: true,
                })
                .collect(),
        );
    }
    unsafe {
        SetWindowPos(
            window.hwnd,
            Some(HWND_TOPMOST),
            180,
            100,
            960,
            660,
            SWP_NOACTIVATE,
        )
        .unwrap();
        let _ = ShowWindow(window.hwnd, SW_SHOWNOACTIVATE);
    }
    let overlay_only = std::env::var_os("WINSHORT_UI_REVIEW_OVERLAY_ONLY").is_some();
    for theme in ["dark", "light"] {
        std::env::set_var("WINSHORT_UI_ACCEPTANCE_THEME", theme);
        for page in [Page::Home, Page::Displays, Page::Overlay, Page::System] {
            if overlay_only && page != Page::Overlay {
                continue;
            }
            {
                let mut state = cell.borrow_mut();
                state.renderer = None;
                state.set_page(page);
                state.rebuild_layout(window.hwnd);
            }
            super::super::native::invalidate(window.hwnd);
            pump();
            capture(
                window.hwnd,
                &format!("{theme}-{}", page.label().to_lowercase()),
            );
        }
        for (page, target, label) in [
            (
                Page::Overlay,
                ElementId::OverlayHoverOpacity,
                "hover-setting",
            ),
            (Page::System, ElementId::CopyVersionInfo, "version-info"),
        ] {
            if overlay_only {
                continue;
            }
            cell.borrow_mut()
                .open_search_destination(window.hwnd, page, target);
            pump();
            let state = cell.borrow();
            let target_rect = state.layout.element(target).unwrap().rect;
            assert!(target_rect.y >= state.layout.content_clip.y);
            assert!(target_rect.bottom() <= state.layout.content_clip.bottom());
            drop(state);
            capture(window.hwnd, &format!("{theme}-{label}"));
        }
    }
    std::env::remove_var("WINSHORT_UI_ACCEPTANCE_THEME");
}
