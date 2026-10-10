//! Opt-in native regression checks. They create only test-owned overlay HWNDs;
//! they do not start the app, acquire its mutex, or touch audio/configuration.

use super::manager::OverlayLifetime;
use super::*;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetWindow, GetWindowRect, IsWindowVisible, MsgWaitForMultipleObjectsEx,
    PeekMessageW, TranslateMessage, WindowFromPoint, GW_HWNDPREV, MSG, MWMO_INPUTAVAILABLE,
    PM_REMOVE, QS_ALLINPUT,
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
#[ignore = "shows test-owned multi-application bar with actual executable icons; does not touch audio"]
fn native_multiple_executable_badges_extend_and_shrink_one_bar() {
    crate::platform::dpi::set_process_awareness();
    let _com = crate::platform::com::ComApartment::init_sta();
    let mut manager = OverlayManager::create().unwrap();
    let mut config = crate::config::Config::default().overlay;
    config.position = OverlayPosition::Center;
    config.hover_opacity = 1.0;
    config.duration_ms = 1000;
    let mic = || {
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(microphone_row(&crate::audio::AudioState::Muted {
                volume_pct: 100,
            })),
        )
    };
    let mic_last = std::env::var_os("WINSHORT_GROUP_TEST_MIC_LAST").is_some();
    if mic_last {
        config.position = if std::env::var("WINSHORT_GROUP_TEST_MIC_LAST").as_deref() == Ok("right")
        {
            OverlayPosition::TopRight
        } else {
            OverlayPosition::TopLeft
        };
    }
    let system = std::env::var("WINDIR").unwrap();
    let paths = [
        format!("{system}\\System32\\notepad.exe"),
        format!("{system}\\System32\\cmd.exe"),
        format!("{system}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"),
    ];
    let models = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let mut row = application_row(&crate::audio::AppAudioState {
                app_name: Some(format!("Test app {}", index + 1)),
                aggregate: crate::audio::Aggregate::AllMuted,
                sessions: 1,
                error: None,
            });
            row.icon = executable_icon(path).expect("real executable icon");
            OverlayModel::single(row)
        })
        .collect::<Vec<_>>();
    if mic_last {
        manager
            .present(
                OverlayRequest::permanent(OverlayKey::ApplicationPermanent(1), models[0].clone()),
                config.clone(),
            )
            .unwrap();
    } else {
        manager.present(mic(), config.clone()).unwrap();
    }
    pump_for(Duration::from_millis(1500));
    manager.refresh_visuals().unwrap();
    let host = manager.test_hwnds()[0];
    let anchor = manager.test_window_rectangles()[0];
    for (index, model) in models.iter().enumerate() {
        if mic_last && index == 0 {
            continue;
        }
        manager
            .present(
                OverlayRequest::permanent(
                    OverlayKey::ApplicationPermanent(index as u64 + 1),
                    model.clone(),
                )
                .replacing(OverlayKey::ApplicationToast(index as u64 + 1)),
                config.clone(),
            )
            .unwrap();
    }
    pump_for(Duration::from_millis(1500));
    manager.refresh_visuals().unwrap();
    pump_for(Duration::from_millis(250));
    manager.refresh_visuals().unwrap();
    if mic_last {
        manager.present(mic(), config.clone()).unwrap();
        pump_for(Duration::from_millis(1500));
        manager.refresh_visuals().unwrap();
        pump_for(Duration::from_millis(250));
        manager.refresh_visuals().unwrap();
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(host) }
                .unwrap();
        let state = cell.borrow();
        assert_eq!(
            state.cluster.primary_offset(Instant::now()),
            if config.position == OverlayPosition::TopRight {
                -88.0
            } else {
                44.0
            }
        );
        let icons = state.cluster.icons(Instant::now());
        let microphone = icons
            .iter()
            .find(|peer| peer.row.icon == OverlayIcon::Microphone)
            .unwrap();
        assert_eq!(
            microphone.offset,
            if config.position == OverlayPosition::TopRight {
                -132.0
            } else {
                0.0
            },
            "microphone must be the first visual member even when programs muted earlier"
        );
    }
    let bars = manager.test_window_rectangles();
    assert_eq!(bars.len(), 1, "mic plus three programs must share one bar");
    let factor = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(host) } as f32 / 96.0;
    assert_eq!(
        bars[0].right - bars[0].left,
        (184.0 * factor).round() as i32
    );
    if config.position == OverlayPosition::TopRight {
        assert_eq!((bars[0].right, bars[0].top), (anchor.right, anchor.top));
    } else {
        assert_eq!((bars[0].left, bars[0].top), (anchor.left, anchor.top));
    }
    assert_eq!(manager.status().permanent_card_count, 4);
    assert_eq!(manager.test_hwnds().len(), 4);
    capture("multiple-exe-muted", &bars);
    let peer_ids = {
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(host) }
                .unwrap();
        cell.borrow()
            .cluster
            .icons(Instant::now())
            .iter()
            .map(|peer| peer.id)
            .collect::<Vec<_>>()
    };
    manager
        .present(
            OverlayRequest::permanent(OverlayKey::ApplicationPermanent(1), models[0].clone()),
            config.clone(),
        )
        .unwrap();
    {
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(host) }
                .unwrap();
        let state = cell.borrow();
        assert_eq!(
            state
                .cluster
                .icons(Instant::now())
                .iter()
                .map(|peer| peer.id)
                .collect::<Vec<_>>(),
            peer_ids,
            "passive metadata refresh must not shuffle settled executable slots"
        );
        assert_eq!(
            state.graphics.clock.active_targets(),
            0,
            "settled bars must not wake every frame"
        );
    }
    let mut active = models[1].clone();
    active.rows[0].tone = OverlayTone::Active;
    active.rows[0].title = "App audio unmuted".into();
    active.rows[0].detail = "Test app 2 · Active".into();
    manager
        .present(
            OverlayRequest::toast(OverlayKey::ApplicationToast(2), active)
                .replacing(OverlayKey::ApplicationPermanent(2)),
            config.clone(),
        )
        .unwrap();
    pump_for(Duration::from_millis(350));
    manager.refresh_visuals().unwrap();
    assert_eq!(manager.status().permanent_card_count, 3);
    assert_eq!(manager.test_window_rectangles().len(), 2);
    let remaining = manager.test_window_rectangles()[0];
    assert_eq!(
        remaining.right - remaining.left,
        (140.0 * factor).round() as i32
    );
    capture("multiple-exe-unmuted", &manager.test_window_rectangles());
    manager
        .remove_key(OverlayKey::ApplicationPermanent(1), &config)
        .unwrap();
    pump_for(Duration::from_millis(350));
    assert_eq!(
        manager.status().permanent_card_count,
        2,
        "closing one app keeps the other app and mic"
    );
    manager
        .present(
            OverlayRequest::permanent(OverlayKey::ApplicationPermanent(2), models[1].clone())
                .replacing(OverlayKey::ApplicationToast(2)),
            config.clone(),
        )
        .unwrap();
    pump_for(Duration::from_millis(1500));
    manager.refresh_visuals().unwrap();
    assert_eq!(
        manager.status().permanent_card_count,
        3,
        "remute must keep independent identities"
    );
    assert_eq!(manager.test_window_rectangles().len(), 1);
    manager.clear();
    let cell = unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(host) };
    if let Some(cell) = cell {
        assert_eq!(cell.borrow().graphics.clock.active_targets(), 0);
    }
}

#[test]
#[ignore = "shows test-owned mic/app badges and exercises grouping, peeling, and rapid remute"]
fn native_mute_group_keeps_audio_entries_independent() {
    crate::platform::dpi::set_process_awareness();
    let _com = crate::platform::com::ComApartment::init_sta();
    let mut manager = OverlayManager::create().unwrap();
    let mut config = crate::config::Config::default().overlay;
    config.position = match std::env::var("WINSHORT_OVERLAY_TEST_POSITION").as_deref() {
        Ok("top-left") => OverlayPosition::TopLeft,
        Ok("center") => OverlayPosition::Center,
        _ => OverlayPosition::TopRight,
    };
    config.hover_opacity = 1.0;
    config.duration_ms = 5000;
    let mic = || {
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(microphone_row(&crate::audio::AudioState::Muted {
                volume_pct: 100,
            })),
        )
        .replacing(OverlayKey::MicrophoneToast)
    };
    let app = || {
        OverlayRequest::permanent(
            OverlayKey::CurrentAppAudioPermanent,
            OverlayModel::single(application_row(&crate::audio::AppAudioState {
                app_name: Some("Example player".into()),
                aggregate: crate::audio::Aggregate::AllMuted,
                sessions: 1,
                error: None,
            })),
        )
        .replacing(OverlayKey::CurrentAppAudio)
    };
    let app_first = std::env::var_os("WINSHORT_GROUP_TEST_APP_FIRST").is_some();
    manager
        .present(if app_first { app() } else { mic() }, config.clone())
        .unwrap();
    pump_for(Duration::from_millis(1500));
    manager.refresh_visuals().unwrap();
    pump_for(Duration::from_millis(180));
    let first_hwnd = manager.test_hwnds()[0];
    let first = manager.test_window_rectangles()[0];
    assert_eq!(first.right - first.left, first.bottom - first.top);
    manager
        .present(if app_first { mic() } else { app() }, config.clone())
        .unwrap();
    let peer_hwnd = manager.test_hwnds()[1];
    pump_for(Duration::from_millis(300));
    assert_eq!(
        manager.test_window_rectangles().len(),
        2,
        "new mute must show its full card before grouping"
    );
    let unchanged = manager.test_window_rectangles()[0];
    assert_eq!(
        (unchanged.left, unchanged.top),
        (first.left, first.top),
        "the existing badge must stay anchored"
    );
    // Keep the timing check free of the screenshot helper's blocking child
    // process; slow captures can consume the entire contraction interval.
    let full = manager.test_window_rectangles()[1];
    assert!(
        full.top > first.bottom,
        "new mute feedback must use the same vertical lane as unmute"
    );
    let full_surface = {
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(peer_hwnd) }
                .unwrap();
        let state = cell.borrow();
        match state.surface.as_ref().unwrap() {
            super::backend::OverlaySurface::Composition(host) => Some(host.test_surface()),
            super::backend::OverlaySurface::Hwnd(_) => None,
        }
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let started = {
            let cell = unsafe {
                crate::platform::window::state_cell::<super::state::OverlayState>(peer_hwnd)
            }
            .unwrap();
            cell.borrow().badge.collapse_started().is_some()
        };
        if started {
            break;
        }
        assert!(Instant::now() < deadline, "peer did not begin contracting");
        pump_for(Duration::from_millis(5));
    }
    manager.refresh_visuals().unwrap();
    pump_for(Duration::from_millis(35));
    {
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(peer_hwnd) }
                .unwrap();
        let state = cell.borrow();
        if let Some(full_surface) = &full_surface {
            let super::backend::OverlaySurface::Composition(host) = state.surface.as_ref().unwrap()
            else {
                panic!("Composition must not fall back while contracting");
            };
            assert_eq!(host.test_surface(), *full_surface,
                "contraction must retain the populated canvas instead of publishing empty replacements");
        }
        let compact = state.badge.value(Instant::now());
        let collapse_start = state.badge.collapse_started().unwrap();
        let join = state.join.expect("joining must start during contraction");
        assert!(
            join.position(collapse_start).y >= full.top,
            "joining must begin at the full card below the badge"
        );
        assert!(
            join.request.position.y < join.position(collapse_start).y,
            "contraction must move toward the adjacent badge slot"
        );
        assert_eq!(
            join.request.started, collapse_start,
            "join must use the contraction clock, not a second animation after it"
        );
        if collapse_start.elapsed() < Duration::from_millis(super::group::GROUP_MS) {
            assert!(compact > 0.0 && compact < 1.0);
        } else {
            // Some fallback drivers block Resize/EndDraw for the whole tween.
            // Catch-up must go straight to the shared final position.
            assert_eq!(compact, 1.0);
            assert!(join.finished(Instant::now()));
        }
    }
    // Reproduce a host frame arriving after the peer frame: neither moving
    // nor resizing the host may jump it above the joining icon.
    let peer_above_host = || {
        let mut previous = unsafe { GetWindow(first_hwnd, GW_HWNDPREV) }.unwrap();
        while !previous.is_invalid() {
            if previous == peer_hwnd {
                return true;
            }
            previous = unsafe { GetWindow(previous, GW_HWNDPREV) }.unwrap();
        }
        false
    };
    assert!(peer_above_host());
    {
        let cell = unsafe {
            crate::platform::window::state_cell::<super::state::OverlayState>(first_hwnd)
        }
        .unwrap();
        let state = cell.borrow();
        let plan = state.frame_plan();
        let apply_region = state.requires_window_region();
        drop(state);
        super::window::apply_frame_plan(first_hwnd, plan, apply_region).unwrap();
    }
    assert!(
        peer_above_host(),
        "animation frames must preserve the joining peer above the host"
    );
    pump_for(Duration::from_millis(250));
    manager.refresh_visuals().unwrap();
    let grouped = manager.test_window_rectangles();
    assert_eq!(
        grouped.len(),
        1,
        "two compact mute entries must share one visible surface"
    );
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(first_hwnd) } as f32 / 96.0;
    assert_eq!(
        grouped[0].right - grouped[0].left,
        (96.0 * dpi).round() as i32
    );
    assert_eq!(
        grouped[0].bottom - grouped[0].top,
        (52.0 * dpi).round() as i32
    );
    assert_eq!(manager.status().permanent_card_count, 2);
    assert_eq!(manager.test_hwnds(), vec![first_hwnd, peer_hwnd]);
    assert!(!unsafe { IsWindowVisible(peer_hwnd).as_bool() });
    {
        let cell = unsafe {
            crate::platform::window::state_cell::<super::state::OverlayState>(first_hwnd)
        }
        .unwrap();
        let state = cell.borrow();
        assert_eq!(
            state.graphics.clock.active_targets(),
            0,
            "settled groups must stop all frame wakes"
        );
    }
    let hit = unsafe {
        WindowFromPoint(windows::Win32::Foundation::POINT {
            x: grouped[0].left + 10,
            y: grouped[0].top + 25,
        })
    };
    assert!(
        !manager.test_hwnds().contains(&hit),
        "group must remain click-through"
    );
    capture("group-joined", &grouped);
    let old_generation = {
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(peer_hwnd) }
                .unwrap();
        cell.borrow().generation
    };
    let refreshed_row = if app_first {
        microphone_row(&crate::audio::AudioState::Muted { volume_pct: 80 })
    } else {
        application_row(&crate::audio::AppAudioState {
            app_name: Some("Refreshed player".into()),
            aggregate: crate::audio::Aggregate::AllMuted,
            sessions: 1,
            error: None,
        })
    };
    let refreshed_detail = refreshed_row.detail.clone();
    manager
        .present(
            OverlayRequest::permanent(
                if app_first {
                    OverlayKey::MicrophonePermanent
                } else {
                    OverlayKey::CurrentAppAudioPermanent
                },
                OverlayModel::single(refreshed_row),
            ),
            config.clone(),
        )
        .unwrap();
    {
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(peer_hwnd) }
                .unwrap();
        let state = cell.borrow();
        assert!(state.parked);
        assert_ne!(
            state.generation, old_generation,
            "parked entries must receive current assignment metadata"
        );
        assert_eq!(state.model.rows[0].detail, refreshed_detail);
    }
    config.duration_ms = 700;
    let (toast_key, permanent_key, row) = if app_first {
        (
            OverlayKey::MicrophoneToast,
            OverlayKey::MicrophonePermanent,
            microphone_row(&crate::audio::AudioState::Active { volume_pct: 100 }),
        )
    } else {
        (
            OverlayKey::CurrentAppAudio,
            OverlayKey::CurrentAppAudioPermanent,
            application_row(&crate::audio::AppAudioState {
                app_name: Some("Example player".into()),
                aggregate: crate::audio::Aggregate::AllActive,
                sessions: 1,
                error: None,
            }),
        )
    };
    manager
        .present(
            OverlayRequest::toast(toast_key, OverlayModel::single(row)).replacing(permanent_key),
            config.clone(),
        )
        .unwrap();
    pump_for(Duration::from_millis(320));
    assert!(
        unsafe { IsWindowVisible(peer_hwnd).as_bool() },
        "unmute must reuse the parked peer HWND"
    );
    assert_eq!(manager.status().permanent_card_count, 1);
    assert_eq!(manager.test_window_rectangles().len(), 2);
    capture("group-peer-unmuted", &manager.test_window_rectangles());
    manager
        .present(if app_first { mic() } else { app() }, config.clone())
        .unwrap();
    pump_for(Duration::from_millis(1600));
    manager.refresh_visuals().unwrap();
    pump_for(Duration::from_millis(300));
    manager.refresh_visuals().unwrap();
    assert_eq!(
        manager.test_window_rectangles().len(),
        1,
        "remute must regroup without stale toast expiry"
    );
    assert_eq!(manager.status().permanent_card_count, 2);
    // The hosting member must peel too, while the former peer becomes a
    // standalone badge above the expanding feedback surface.
    let (host_toast, host_key, host_row) = if app_first {
        (
            OverlayKey::CurrentAppAudio,
            OverlayKey::CurrentAppAudioPermanent,
            application_row(&crate::audio::AppAudioState {
                app_name: Some("Example player".into()),
                aggregate: crate::audio::Aggregate::AllActive,
                sessions: 1,
                error: None,
            }),
        )
    } else {
        (
            OverlayKey::MicrophoneToast,
            OverlayKey::MicrophonePermanent,
            microphone_row(&crate::audio::AudioState::Active { volume_pct: 100 }),
        )
    };
    manager
        .present(
            OverlayRequest::toast(host_toast, OverlayModel::single(host_row)).replacing(host_key),
            config.clone(),
        )
        .unwrap();
    pump_for(Duration::from_millis(30));
    {
        let cell = unsafe {
            crate::platform::window::state_cell::<super::state::OverlayState>(first_hwnd)
        }
        .unwrap();
        assert_eq!(cell.borrow().behind_badge, Some(peer_hwnd));
    }
    pump_for(Duration::from_millis(300));
    assert_eq!(manager.test_window_rectangles().len(), 2);
    assert_eq!(manager.status().permanent_card_count, 1);
    capture("group-host-unmuted", &manager.test_window_rectangles());
    manager
        .present(if app_first { app() } else { mic() }, config.clone())
        .unwrap();
    pump_for(Duration::from_millis(1500));
    manager.refresh_visuals().unwrap();
    pump_for(Duration::from_millis(300));
    manager.refresh_visuals().unwrap();
    assert_eq!(manager.test_window_rectangles().len(), 1);
    // Removing the currently hosting member (foreground changes or category
    // disabling) promotes the parked member without replaying its full hold.
    let host_hwnd = manager
        .test_hwnds()
        .into_iter()
        .find(|hwnd| unsafe { IsWindowVisible(*hwnd).as_bool() })
        .unwrap();
    let (remove_key, survivor) = if host_hwnd == first_hwnd {
        (host_key, peer_hwnd)
    } else {
        (permanent_key, first_hwnd)
    };
    manager.remove_key(remove_key, &config).unwrap();
    pump_for(Duration::from_millis(300));
    let remaining = manager.test_window_rectangles();
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        remaining[0].right - remaining[0].left,
        remaining[0].bottom - remaining[0].top
    );
    assert!(unsafe { IsWindowVisible(survivor).as_bool() });
    manager.shutdown();
}

#[test]
#[ignore = "shows a test-owned microphone popup and rapidly reverses its text/width motion"]
fn native_mic_content_motion_reverses_without_replacing_the_window() {
    crate::platform::dpi::set_process_awareness();
    let _com = crate::platform::com::ComApartment::init_sta();
    let mut manager = OverlayManager::create().unwrap();
    let mut config = crate::config::Config::default().overlay;
    config.position = OverlayPosition::TopRight;
    config.duration_ms = 700;
    config.hover_opacity = 1.0;
    let show = |manager: &mut OverlayManager, muted: bool| {
        let state = if muted {
            crate::audio::AudioState::Muted { volume_pct: 100 }
        } else {
            crate::audio::AudioState::Active { volume_pct: 100 }
        };
        let model = OverlayModel::single(microphone_row(&state));
        let request = if muted {
            OverlayRequest::permanent(OverlayKey::MicrophonePermanent, model)
                .replacing(OverlayKey::MicrophoneToast)
        } else {
            OverlayRequest::toast(OverlayKey::MicrophoneToast, model)
                .replacing(OverlayKey::MicrophonePermanent)
        };
        manager.present(request, config.clone()).unwrap();
    };
    show(&mut manager, true);
    pump_for(Duration::from_millis(300));
    let hwnd = manager.test_hwnds()[0];
    let original = manager.test_window_rectangles()[0];
    show(&mut manager, false);
    {
        // SAFETY: this test owns the HWND and inspects its state on the same UI
        // thread between native dispatches; no borrow spans a presentation.
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(hwnd) }
                .unwrap();
        let state = cell.borrow();
        assert!(state.content.is_animating());
        assert_eq!(
            state.content.previous.as_ref().unwrap().rows[0].title,
            "Microphone muted"
        );
        assert!(state.content.text_alpha(Instant::now()) < 1.0);
        assert!(state.frame_plan().animation_active);
    }
    for muted in [true, false, true, false, true, false] {
        pump_for(Duration::from_millis(25));
        show(&mut manager, muted);
        assert_eq!(manager.test_hwnds(), vec![hwnd]);
        assert_eq!(manager.status().active_card_count, 1);
        let rect = manager.test_window_rectangles()[0];
        assert_eq!(
            rect.right, original.right,
            "width changes must retain the right anchor"
        );
        assert_eq!(rect.top, original.top);
    }
    pump_for(Duration::from_millis(210));
    {
        let cell =
            unsafe { crate::platform::window::state_cell::<super::state::OverlayState>(hwnd) }
                .unwrap();
        let state = cell.borrow();
        assert_eq!(state.model.rows[0].title, "Microphone unmuted");
        assert!(state.content.previous.is_none());
        assert!(!state.content.is_animating());
        assert!(
            !state.frame_plan().animation_active,
            "content must stop frame wakes when settled"
        );
    }
    pump_for(Duration::from_millis(1150));
    assert!(
        !unsafe { IsWindowVisible(hwnd).as_bool() },
        "the final unmute toast must still expire"
    );
    manager.shutdown();
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
    pump_for(Duration::from_millis(300));
    manager.refresh_visuals().unwrap();
    let cards = manager.test_window_rectangles();
    assert_eq!(cards.len(), 3);
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
            .filter(|hwnd| unsafe { IsWindowVisible(*hwnd).as_bool() })
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
