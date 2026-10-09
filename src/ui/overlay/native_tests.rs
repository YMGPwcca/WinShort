//! Opt-in native regression checks. They create only test-owned overlay HWNDs;
//! they do not start the app, acquire its mutex, or touch audio/configuration.

use super::manager::OverlayLifetime;
use super::*;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetWindowRect, IsWindowVisible, MsgWaitForMultipleObjectsEx, PeekMessageW,
    TranslateMessage, WindowFromPoint, MSG, MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT,
};

fn pump_for(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        // Wait on messages so this harness does not cap a high-refresh frame
        // source with its own fixed sleep or spin when overlays are idle.
        let wait = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .max(1)
            .min(u32::MAX as u128) as u32;
        unsafe {
            MsgWaitForMultipleObjectsEx(None, wait, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        }
    }
}

#[test]
#[ignore = "measures real overlay rendering cadence and briefly blocks its test UI thread"]
fn native_refresh_animation_coalesces_frames_and_stops_when_idle() {
    crate::platform::dpi::set_process_awareness();
    let _com = crate::platform::com::ComApartment::init_sta();
    let mut manager = OverlayManager::create().unwrap();
    let mut config = crate::config::Config::default().overlay;
    config.hover_opacity = 1.0;
    manager
        .present(
            OverlayRequest::permanent(
                OverlayKey::MicrophonePermanent,
                OverlayModel::single(microphone_row(&crate::audio::AudioState::Muted {
                    volume_pct: 100,
                })),
            ),
            config,
        )
        .unwrap();
    let hwnd = manager.test_hwnds()[0];
    // The producer must leave at most one pending frame while rendering/input
    // handling is stalled. It must not queue a burst of obsolete frames.
    std::thread::sleep(Duration::from_millis(70));
    let mut queued = Vec::new();
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(
            &mut msg,
            Some(hwnd),
            super::frame_clock::FRAME_MESSAGE,
            super::frame_clock::FRAME_MESSAGE,
            PM_REMOVE,
        )
        .as_bool()
        {
            queued.push(msg);
        }
    }
    assert_eq!(queued.len(), 1, "animation frame wakes must be coalesced");
    for msg in queued {
        unsafe {
            DispatchMessageW(&msg);
        }
    }
    pump_for(Duration::from_millis(1650));
    let (period, frames, clock) = {
        // SAFETY: this test owns the live HWND and reads its state only on the
        // creating UI thread, outside native message dispatch.
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(hwnd) }
                .unwrap();
        let state = cell.borrow();
        (
            state.frame_period,
            state.test_frame_times.clone(),
            state.graphics.clock.clone(),
        )
    };
    let mut intervals = frames
        .windows(2)
        .map(|times| times[1].duration_since(times[0]))
        .filter(|gap| *gap < Duration::from_millis(40))
        .collect::<Vec<_>>();
    intervals.sort();
    assert!(
        intervals.len() >= 8,
        "animation produced too few real rendered frames"
    );
    let median = intervals[intervals.len() / 2];
    let monitor_hz = 1.0 / period.as_secs_f64();
    let rendered_hz = 1.0 / median.as_secs_f64();
    println!("monitor={monitor_hz:.2} Hz, rendered median={rendered_hz:.2} fps, frames={}, idle targets={}", frames.len(), clock.active_targets());
    if monitor_hz >= 120.0 {
        assert!(
            rendered_hz > 90.0,
            "high-refresh animation is still capped near 60 fps"
        );
    }
    assert_eq!(
        clock.active_targets(),
        0,
        "settled permanent badges must stop clock work"
    );
    manager.shutdown();
}

#[test]
#[ignore = "shows isolated test-owned native overlay windows"]
fn native_three_audio_cards_are_visible_and_separate() {
    crate::platform::dpi::set_process_awareness();
    let _com = crate::platform::com::ComApartment::init_sta();
    let mut manager = OverlayManager::create().unwrap();
    let mut config = crate::config::Config::default().overlay;
    config.position = match std::env::var("WINSHORT_OVERLAY_TEST_POSITION").as_deref() {
        Ok("bottom-right") => OverlayPosition::BottomRight,
        Ok("center") => OverlayPosition::Center,
        _ => OverlayPosition::TopLeft,
    };
    config.duration_ms = 10000;
    config.hover_opacity = 1.0;
    for (key, icon, lifetime) in [
        (
            OverlayKey::MicrophonePermanent,
            OverlayIcon::Microphone,
            OverlayLifetime::Permanent,
        ),
        (
            OverlayKey::OutputDevice,
            OverlayIcon::Output,
            OverlayLifetime::Toast,
        ),
        (
            OverlayKey::CurrentAppAudio,
            OverlayIcon::Application,
            OverlayLifetime::Toast,
        ),
    ] {
        let tone = if key == OverlayKey::MicrophonePermanent {
            OverlayTone::Muted
        } else {
            OverlayTone::Changed
        };
        manager
            .present(
                OverlayRequest {
                    key,
                    lifetime,
                    replace_key: None,
                    model: OverlayModel::single(OverlayRow {
                        category: None,
                        icon,
                        tone,
                        title: match key {
                            OverlayKey::MicrophonePermanent => "Microphone muted",
                            OverlayKey::OutputDevice => "Next speaker",
                            _ => "Current app audio",
                        }
                        .into(),
                        detail: if key == OverlayKey::MicrophonePermanent {
                            "100% input volume"
                        } else {
                            "Isolated native regression"
                        }
                        .into(),
                    }),
                },
                config.clone(),
            )
            .unwrap();
        pump_for(Duration::from_millis(250));
    }
    let rectangles = manager.test_window_rectangles();
    assert_eq!(rectangles.len(), 3);
    for (i, left) in rectangles.iter().enumerate() {
        for right in &rectangles[i + 1..] {
            assert!(
                left.bottom <= right.top || right.bottom <= left.top,
                "cards overlap: {left:?}, {right:?}"
            );
        }
    }
    println!(
        "native overlay PID: {}; rectangles: {rectangles:?}",
        std::process::id()
    );
    unsafe {
        use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
        let dc = GetDC(None);
        let palette =
            super::palette::palette_for(config.appearance, SystemVisualPreferences::query());
        for (i, rect) in rectangles.iter().enumerate() {
            let pixel = GetPixel(dc, rect.left + 38, rect.top + 34).0;
            println!("badge at {rect:?}: #{pixel:06x}");
            let tone = if i == 0 {
                palette.tone_muted
            } else {
                palette.tone_changed
            };
            let expected = tone.r as u32 | ((tone.g as u32) << 8) | ((tone.b as u32) << 16);
            assert_eq!(
                pixel, expected,
                "visible HWND must contain its own rendered badge"
            );
            let hit = WindowFromPoint(windows::Win32::Foundation::POINT {
                x: rect.left + 38,
                y: rect.top + 34,
            });
            assert!(
                !manager.test_hwnds().contains(&hit),
                "overlay intercepted hit testing"
            );
        }
        ReleaseDC(None, dc);
    }
    capture("three-audio-cards", &rectangles);
    let microphone_hwnd = manager.test_hwnds()[0];
    pump_for(Duration::from_millis(750));
    manager.refresh_visuals().unwrap();
    pump_for(Duration::from_millis(180));
    let collapsed = manager.test_window_rectangles();
    assert_eq!(
        collapsed[0].right - collapsed[0].left,
        collapsed[0].bottom - collapsed[0].top
    );
    assert!(collapsed[0].right - collapsed[0].left < rectangles[0].right - rectangles[0].left);
    assert_anchor(config.position, rectangles[0], collapsed[0]);
    capture("microphone-badge", &collapsed[..1]);
    config.duration_ms = 1800;
    manager
        .present(
            OverlayRequest::toast(
                OverlayKey::MicrophoneToast,
                OverlayModel::single(super::presentation::microphone_row(
                    &crate::audio::AudioState::Active { volume_pct: 100 },
                )),
            )
            .replacing(OverlayKey::MicrophonePermanent),
            config.clone(),
        )
        .unwrap();
    assert_eq!(
        manager.test_hwnds()[0],
        microphone_hwnd,
        "unmute must keep the HWND"
    );
    let expanding = manager.test_window_rectangles();
    assert_anchor(config.position, collapsed[0], expanding[0]);
    pump_for(Duration::from_millis(300));
    let expanded = manager.test_window_rectangles();
    assert!(expanded[0].right - expanded[0].left > collapsed[0].right - collapsed[0].left);
    assert_eq!(
        expanded[0].bottom - expanded[0].top,
        rectangles[0].bottom - rectangles[0].top
    );
    assert_anchor(config.position, rectangles[0], expanded[0]);
    capture("microphone-unmuted", &expanded[..1]);
    pump_for(Duration::from_millis(2400));
    assert!(
        !unsafe { IsWindowVisible(microphone_hwnd).as_bool() },
        "unmute toast must disappear after its hold and exit animation"
    );
    manager.shutdown();
}

fn assert_anchor(position: OverlayPosition, before: RECT, after: RECT) {
    match position {
        OverlayPosition::BottomRight => {
            assert_eq!(after.right, before.right);
            assert_eq!(after.bottom, before.bottom);
        }
        OverlayPosition::Center => {
            assert!(((after.left + after.right) - (before.left + before.right)).abs() <= 2);
            assert!(((after.top + after.bottom) - (before.top + before.bottom)).abs() <= 2);
        }
        _ => {
            assert_eq!(after.left, before.left);
            assert_eq!(after.top, before.top);
        }
    }
}

#[test]
#[ignore = "shows isolated test-owned app mute and audio overlay windows"]
fn native_app_mute_badge_expands_in_place_and_stacks_with_mic_and_output() {
    crate::platform::dpi::set_process_awareness();
    let _com = crate::platform::com::ComApartment::init_sta();
    let mut manager = OverlayManager::create().unwrap();
    let mut config = crate::config::Config::default().overlay;
    config.position = match std::env::var("WINSHORT_OVERLAY_TEST_POSITION").as_deref() {
        Ok("bottom-right") => OverlayPosition::BottomRight,
        Ok("center") => OverlayPosition::Center,
        _ => OverlayPosition::TopLeft,
    };
    config.duration_ms = 10000;
    config.hover_opacity = 1.0;
    let app_state = crate::audio::AppAudioState {
        app_name: Some("Example player".into()),
        aggregate: crate::audio::Aggregate::AllMuted,
        sessions: 1,
        error: None,
    };
    manager
        .present(
            OverlayRequest::permanent(
                OverlayKey::CurrentAppAudioPermanent,
                OverlayModel::single(application_row(&app_state)),
            ),
            config.clone(),
        )
        .unwrap();
    pump_for(Duration::from_millis(300));
    let hwnd = manager.test_hwnds()[0];
    let expanded = manager.test_window_rectangles()[0];
    capture("app-muted-full", &[expanded]);
    pump_for(Duration::from_millis(1300));
    manager.refresh_visuals().unwrap();
    pump_for(Duration::from_millis(180));
    let compact = manager.test_window_rectangles()[0];
    assert_eq!(compact.right - compact.left, compact.bottom - compact.top);
    assert!(compact.right - compact.left < expanded.right - expanded.left);
    assert_anchor(config.position, expanded, compact);
    let hit = unsafe {
        WindowFromPoint(windows::Win32::Foundation::POINT {
            x: compact.left + 10,
            y: compact.top + 10,
        })
    };
    assert_ne!(hit, hwnd, "app mute badge must allow click-through");
    capture("app-muted-badge", &[compact]);

    for request in [
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(microphone_row(&crate::audio::AudioState::Muted {
                volume_pct: 100,
            })),
        ),
        OverlayRequest::toast(
            OverlayKey::OutputDevice,
            OverlayModel::single(OverlayRow {
                category: None,
                icon: OverlayIcon::Output,
                tone: OverlayTone::Changed,
                title: "Next speaker".into(),
                detail: "Example output".into(),
            }),
        ),
        OverlayRequest::toast(
            OverlayKey::CurrentAppVolume,
            OverlayModel::single(OverlayRow {
                category: None,
                icon: OverlayIcon::Application,
                tone: OverlayTone::Changed,
                title: "App volume".into(),
                detail: "Example player · 50%".into(),
            }),
        ),
    ] {
        manager.present(request, config.clone()).unwrap();
    }
    pump_for(Duration::from_millis(1650));
    manager.refresh_visuals().unwrap();
    pump_for(Duration::from_millis(180));
    let cards = manager.test_window_rectangles();
    assert_eq!(cards.len(), 4);
    for (index, left) in cards.iter().enumerate() {
        for right in &cards[index + 1..] {
            assert!(
                left.bottom <= right.top || right.bottom <= left.top,
                "cards overlap: {left:?}, {right:?}"
            );
        }
    }
    assert_eq!(manager.status().permanent_card_count, 2);
    capture("app-mic-output-volume", &cards);
    manager
        .remove_key(OverlayKey::MicrophonePermanent, &config)
        .unwrap();
    manager
        .remove_key(OverlayKey::OutputDevice, &config)
        .unwrap();
    manager
        .remove_key(OverlayKey::CurrentAppVolume, &config)
        .unwrap();
    pump_for(Duration::from_millis(300));
    let compact = manager.test_window_rectangles()[0];
    config.duration_ms = 1800;
    let position = config.position;
    manager
        .present(
            OverlayRequest::toast(
                OverlayKey::CurrentAppAudio,
                OverlayModel::single(application_row(&crate::audio::AppAudioState {
                    aggregate: crate::audio::Aggregate::AllActive,
                    ..app_state
                })),
            )
            .replacing(OverlayKey::CurrentAppAudioPermanent),
            config,
        )
        .unwrap();
    assert_eq!(
        manager.test_hwnds()[0],
        hwnd,
        "unmute must reuse the app badge HWND"
    );
    pump_for(Duration::from_millis(280));
    let unmuted = manager.test_window_rectangles()[0];
    assert!(unmuted.right - unmuted.left > compact.right - compact.left);
    assert_eq!(unmuted.bottom - unmuted.top, expanded.bottom - expanded.top);
    assert_anchor(position, compact, unmuted);
    capture("app-unmuted", &[unmuted]);
    pump_for(Duration::from_millis(2400));
    assert!(
        !unsafe { IsWindowVisible(hwnd).as_bool() },
        "app unmute feedback must expire"
    );
    manager.shutdown();
}

fn capture(name: &str, rectangles: &[RECT]) {
    let Some(root) = std::env::var_os("WINSHORT_OVERLAY_CAPTURE_DIR") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root
        .join(format!("{name}.png"))
        .to_string_lossy()
        .replace('\'', "''");
    let left = rectangles.iter().map(|r| r.left).min().unwrap();
    let top = rectangles.iter().map(|r| r.top).min().unwrap();
    let width = rectangles.iter().map(|r| r.right).max().unwrap() - left;
    let height = rectangles.iter().map(|r| r.bottom).max().unwrap() - top;
    let script = format!(
        r#"Add-Type -AssemblyName System.Drawing; $b = New-Object System.Drawing.Bitmap({width},{height}); $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen({left},{top},0,0,$b.Size); $b.Save('{path}',[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $b.Dispose()"#
    );
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .unwrap();
    assert!(status.success());
}

impl OverlayManager {
    // Defined in manager.rs because the HWND registry is private to that module.
    fn test_window_rectangles(&self) -> Vec<RECT> {
        self.test_hwnds()
            .into_iter()
            .map(|hwnd| unsafe {
                assert!(IsWindowVisible(hwnd).as_bool());
                let mut rect = RECT::default();
                GetWindowRect(hwnd, &mut rect).unwrap();
                rect
            })
            .collect()
    }
}

#[test]
#[ignore = "moves and restores the pointer over a test-owned overlay"]
fn native_hover_fades_restores_and_can_be_disabled_without_blocking_clicks() {
    use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};
    crate::platform::dpi::set_process_awareness();
    let _com = crate::platform::com::ComApartment::init_sta();
    let mut original = windows::Win32::Foundation::POINT::default();
    unsafe {
        GetCursorPos(&mut original).unwrap();
    }
    struct RestorePointer(windows::Win32::Foundation::POINT);
    impl Drop for RestorePointer {
        fn drop(&mut self) {
            let _ = unsafe { SetCursorPos(self.0.x, self.0.y) };
        }
    }
    let _restore = RestorePointer(original);
    let mut manager = OverlayManager::create().unwrap();
    let mut config = crate::config::Config::default().overlay;
    config.position = OverlayPosition::TopRight;
    config.monitor = crate::config::model::MonitorChoice::Primary;
    config.blur = OverlayBlur::Solid;
    config.hover_opacity = 0.3;
    manager
        .present(
            OverlayRequest::permanent(
                OverlayKey::Status,
                OverlayModel::single(OverlayRow::preview(
                    "Hover opacity test",
                    "Test-owned overlay",
                )),
            ),
            config.clone(),
        )
        .unwrap();
    pump_for(Duration::from_millis(220));
    let hwnd = manager.test_hwnds()[0];
    let rect = manager.test_window_rectangles()[0];
    let outside = windows::Win32::Foundation::POINT {
        x: rect.left - 50,
        y: rect.bottom + 50,
    };
    move_pointer(outside);
    pump_for(Duration::from_millis(220));
    let sample = || unsafe {
        let dc = GetDC(None);
        let pixel = GetPixel(dc, rect.left + 46, rect.top + 47).0;
        ReleaseDC(None, dc);
        pixel
    };
    let full = sample();
    capture("hover-normal", &[rect]);
    move_pointer(windows::Win32::Foundation::POINT {
        x: rect.left + 34,
        y: rect.top + 47,
    });
    pump_for(Duration::from_millis(220));
    let dim = sample();
    capture("hover-dimmed", &[rect]);
    let cell =
        unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(hwnd) }.unwrap();
    assert_eq!(
        cell.borrow().hover.value(Instant::now()),
        0.3,
        "mouse hook must observe hover over a click-through HWND"
    );
    assert_ne!(full, dim, "whole rendered card must fade on the desktop");
    assert_eq!(
        cell.borrow().hover.timer_interval(),
        None,
        "stationary hover must stop its frame timer"
    );
    assert_ne!(
        unsafe {
            WindowFromPoint(windows::Win32::Foundation::POINT {
                x: rect.left + 34,
                y: rect.top + 47,
            })
        },
        hwnd
    );
    move_pointer(outside);
    pump_for(Duration::from_millis(220));
    assert_eq!(sample(), full, "pointer leave must restore normal opacity");
    move_pointer(windows::Win32::Foundation::POINT {
        x: rect.left + 34,
        y: rect.top + 47,
    });
    pump_for(Duration::from_millis(220));
    config.hover_opacity = 1.0;
    manager.apply_config(&config).unwrap();
    pump_for(Duration::from_millis(220));
    assert_eq!(sample(), full, "100% must disable hover fading immediately");
    assert!(
        !super::hover::observer_active(),
        "disabled hover must release the mouse hook"
    );
    manager.shutdown();
    assert!(!super::hover::observer_active());
}

fn move_pointer(point: windows::Win32::Foundation::POINT) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: ((point.x - left) as i64 * 65535 / (width - 1) as i64) as i32,
                dy: ((point.y - top) as i64 * 65535 / (height - 1) as i64) as i32,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                ..Default::default()
            },
        },
    };
    assert_eq!(
        unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) },
        1
    );
}
