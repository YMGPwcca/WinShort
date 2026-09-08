use super::model::OverlayLifetime;
use super::*;
use windows::Win32::Foundation::RECT;

fn row(icon: OverlayIcon, title: &str) -> OverlayRow {
    let category = match icon {
        OverlayIcon::Microphone => crate::config::model::OverlayNotificationCategory::Microphone,
        OverlayIcon::Output => crate::config::model::OverlayNotificationCategory::Speaker,
        OverlayIcon::Application => {
            crate::config::model::OverlayNotificationCategory::CurrentAppAudio
        }
        OverlayIcon::Workspace | OverlayIcon::Info => {
            crate::config::model::OverlayNotificationCategory::Workspace
        }
    };
    OverlayRow {
        category: Some(category),
        icon,
        tone: OverlayTone::Active,
        title: title.into(),
        detail: "detail".into(),
    }
}

#[test]
fn visual_preferences_select_reduced_motion_and_high_contrast_palette() {
    let preferences = SystemVisualPreferences {
        animations_enabled: false,
        high_contrast: true,
        high_contrast_background: VisualRgb {
            r: 10,
            g: 20,
            b: 30,
        },
        high_contrast_foreground: VisualRgb {
            r: 240,
            g: 200,
            b: 160,
        },
        high_contrast_highlight: VisualRgb {
            r: 50,
            g: 100,
            b: 150,
        },
        high_contrast_highlight_foreground: VisualRgb { r: 1, g: 2, b: 3 },
        ..SystemVisualPreferences::default()
    };

    assert_eq!(motion_policy(preferences), MotionPolicy::Reduced);
    let palette = palette_for(OverlayAppearance::System, preferences);
    assert_eq!(palette.surface, Color::rgb(10, 20, 30));
    assert_eq!(palette.unavailable_text, Color::rgb(240, 200, 160));
    assert_eq!(palette.text, Color::rgb(240, 200, 160));
    assert_eq!(palette.tone_muted, Color::rgb(10, 20, 30));
    assert_eq!(palette.tone_changed, Color::rgb(50, 100, 150));
    assert_eq!(palette.icon, Color::rgb(240, 200, 160));
    assert_eq!(palette.changed_icon, Color::rgb(1, 2, 3));
    assert!(palette.opaque);
}

#[test]
fn backdrop_policy_falls_back_for_accessibility_or_missing_api() {
    let normal = SystemVisualPreferences::default();
    assert_eq!(backdrop_mode(normal, true), BackdropMode::Acrylic);
    assert_eq!(backdrop_mode(normal, false), BackdropMode::Opaque);
    assert_eq!(
        backdrop_mode(
            SystemVisualPreferences {
                high_contrast: true,
                ..normal
            },
            true
        ),
        BackdropMode::Opaque
    );
    assert_eq!(
        backdrop_mode(
            SystemVisualPreferences {
                disable_overlapped_content: true,
                ..normal
            },
            true
        ),
        BackdropMode::Opaque
    );
}

#[test]
fn appearance_policy_resolves_system_and_explicit_modes() {
    let preferences = SystemVisualPreferences {
        system_theme: ThemeMode::Light,
        ..SystemVisualPreferences::default()
    };

    let system = palette_for(OverlayAppearance::System, preferences);
    let explicit_dark = palette_for(OverlayAppearance::Dark, preferences);
    let explicit_light = palette_for(OverlayAppearance::Light, preferences);
    assert_eq!(system.surface, Color::rgba(255, 255, 255, 248));
    assert_eq!(explicit_dark.surface, Color::rgba(43, 43, 43, 248));
    assert_eq!(explicit_light.surface, Color::rgba(255, 255, 255, 248));
    assert_eq!(explicit_dark.secondary, Color::rgb(230, 236, 240));
    assert_eq!(explicit_light.secondary, Color::rgb(92, 92, 92));
    assert_eq!(
        resolved_theme_mode(OverlayAppearance::System, preferences),
        ThemeMode::Light
    );
    assert_eq!(
        composition_tint_alpha(ThemeMode::Dark, OverlayBlur::BlurMedium),
        96
    );
    assert_eq!(
        composition_tint_alpha(ThemeMode::Light, OverlayBlur::BlurMedium),
        130
    );
}

#[test]
fn light_composition_tint_stays_strong_enough_for_dark_text() {
    assert!(composition_tint_alpha(ThemeMode::Light, OverlayBlur::BlurMedium) >= 130);
    assert_eq!(
        composition_tint_alpha(ThemeMode::Light, OverlayBlur::BlurLight),
        70
    );
}

#[test]
fn dark_composition_tint_keeps_secondary_text_high_contrast() {
    assert!(composition_tint_alpha(ThemeMode::Dark, OverlayBlur::BlurHeavy) >= 128);
    assert_eq!(
        palette_for(OverlayAppearance::Dark, SystemVisualPreferences::default()).secondary,
        Color::rgb(230, 236, 240)
    );
}

#[test]
fn blur_levels_have_distinct_ordered_runtime_amounts() {
    assert_eq!(OverlayBlur::Transparent.blur_amount(), None);
    assert_eq!(OverlayBlur::BlurLight.blur_amount(), Some(4.0));
    assert_eq!(OverlayBlur::BlurMedium.blur_amount(), Some(8.0));
    assert_eq!(OverlayBlur::BlurHeavy.blur_amount(), Some(18.0));
    assert_eq!(OverlayBlur::Solid.blur_amount(), None);
    assert!(OverlayBlur::BlurLight.blur_amount() < OverlayBlur::BlurMedium.blur_amount());
    assert!(OverlayBlur::BlurMedium.blur_amount() < OverlayBlur::BlurHeavy.blur_amount());
}

#[test]
fn composition_tint_levels_follow_blur_intensity() {
    let dark_levels = OverlayBlur::ALL.map(|blur| composition_tint_alpha(ThemeMode::Dark, blur));
    let light_levels = OverlayBlur::ALL.map(|blur| composition_tint_alpha(ThemeMode::Light, blur));
    assert_eq!(dark_levels, [0, 52, 96, 148, 148]);
    assert_eq!(light_levels, [0, 70, 130, 200, 200]);
    assert!(dark_levels[1] < dark_levels[2]);
    assert!(dark_levels[2] < dark_levels[3]);
    assert!(light_levels[1] < light_levels[2]);
    assert!(light_levels[2] < light_levels[3]);
}

#[test]
fn coalescer_replaces_same_icon_and_keeps_deterministic_order() {
    let current = OverlayModel::from_rows_with_lifetime(
        vec![
            row(OverlayIcon::Output, "old output"),
            row(OverlayIcon::Application, "app"),
        ],
        OverlayLifetime::Transient,
    );
    let incoming = OverlayModel::single_with_lifetime(
        row(OverlayIcon::Microphone, "mic"),
        OverlayLifetime::Sticky(OverlayIcon::Microphone),
    );
    let merged = merge_overlay_models(&current, &incoming);
    assert_eq!(
        merged
            .rows
            .iter()
            .map(|value| value.icon)
            .collect::<Vec<_>>(),
        vec![
            OverlayIcon::Microphone,
            OverlayIcon::Output,
            OverlayIcon::Application
        ]
    );

    let replaced = merge_overlay_models(
        &merged,
        &OverlayModel::single(row(OverlayIcon::Output, "new output")),
    );
    assert_eq!(replaced.rows.len(), 3);
    assert_eq!(replaced.rows[1].title, "new output");
    assert!(replaced.is_sticky());
}

#[test]
fn repeated_sticky_microphone_updates_replace_one_row_and_keep_sticky_lifetime() {
    let muted = OverlayModel::single_with_lifetime(
        row(OverlayIcon::Microphone, "muted"),
        OverlayLifetime::Sticky(OverlayIcon::Microphone),
    );
    let refreshed = OverlayModel::single_with_lifetime(
        row(OverlayIcon::Microphone, "still muted"),
        OverlayLifetime::Sticky(OverlayIcon::Microphone),
    );

    let merged = merge_overlay_models(&muted, &refreshed);

    assert_eq!(merged.rows.len(), 1);
    assert_eq!(merged.rows[0].title, "still muted");
    assert!(merged.is_sticky());
}

#[test]
fn sticky_microphone_owner_prunes_speaker_and_workspace_rows() {
    for transient_icon in [OverlayIcon::Output, OverlayIcon::Workspace] {
        let sticky = OverlayModel::from_rows_with_lifetime(
            vec![
                row(OverlayIcon::Microphone, "muted"),
                row(transient_icon, "transient"),
            ],
            OverlayLifetime::Sticky(OverlayIcon::Microphone),
        );

        let pruned = sticky.retain_sticky_owner();

        assert_eq!(pruned.rows.len(), 1);
        assert_eq!(pruned.rows[0].icon, OverlayIcon::Microphone);
        assert!(pruned.is_sticky());
    }
}

#[test]
fn unmuted_microphone_update_changes_sticky_lifetime_to_transient() {
    let muted = OverlayModel::single_with_lifetime(
        row(OverlayIcon::Microphone, "muted"),
        OverlayLifetime::Sticky(OverlayIcon::Microphone),
    );
    let unmuted = OverlayModel::single(row(OverlayIcon::Microphone, "unmuted"));

    let merged = merge_overlay_models(&muted, &unmuted);

    assert!(!merged.is_sticky());
}

#[test]
fn disabled_notification_categories_are_removed_before_rendering() {
    let model = OverlayModel::from_rows_with_lifetime(
        vec![
            row(OverlayIcon::Microphone, "mic"),
            row(OverlayIcon::Output, "speaker"),
            row(OverlayIcon::Application, "app"),
        ],
        OverlayLifetime::Transient,
    );
    let notifications = crate::config::model::OverlayNotifications {
        microphone: false,
        ..Default::default()
    };
    let filtered = model.clone().filter_enabled(notifications);
    assert_eq!(
        filtered.rows.iter().map(|row| row.icon).collect::<Vec<_>>(),
        vec![OverlayIcon::Output, OverlayIcon::Application]
    );

    let notifications = crate::config::model::OverlayNotifications {
        speaker: false,
        ..Default::default()
    };
    let filtered = model.filter_enabled(notifications);
    assert_eq!(
        filtered.rows.iter().map(|row| row.icon).collect::<Vec<_>>(),
        vec![OverlayIcon::Microphone, OverlayIcon::Application]
    );
}

#[test]
fn microphone_category_filter_suppresses_muted_and_unmuted_states() {
    let notifications = crate::config::model::OverlayNotifications {
        microphone: false,
        ..Default::default()
    };

    for state in [
        crate::audio::AudioState::Muted { volume_pct: 42 },
        crate::audio::AudioState::Active { volume_pct: 42 },
    ] {
        let filtered = microphone_overlay_model(&state).filter_enabled(notifications);
        assert!(filtered.rows.is_empty());
        assert!(!filtered.is_sticky());
    }
}

#[test]
fn microphone_filter_removes_sticky_owner_but_keeps_enabled_transient_rows() {
    let sticky = OverlayModel::from_rows_with_lifetime(
        vec![
            row(OverlayIcon::Microphone, "muted"),
            row(OverlayIcon::Output, "speaker"),
        ],
        OverlayLifetime::Sticky(OverlayIcon::Microphone),
    );
    let notifications = crate::config::model::OverlayNotifications {
        microphone: false,
        ..Default::default()
    };

    let filtered = sticky.filter_enabled(notifications);

    assert_eq!(filtered.rows.len(), 1);
    assert_eq!(filtered.rows[0].icon, OverlayIcon::Output);
    assert!(!filtered.is_sticky());
}
#[test]
fn explicit_preview_bypasses_notification_categories() {
    let notifications = crate::config::model::OverlayNotifications {
        microphone: false,
        speaker: false,
        current_app_audio: false,
        workspace: false,
        display_profile: false,
    };
    let normal = OverlayModel::single(row(OverlayIcon::Microphone, "mic"));
    assert!(normal.filter_enabled(notifications).rows.is_empty());

    let preview = OverlayModel::preview(OverlayRow::preview(
        "WinShort overlay preview",
        "Previewing current overlay settings",
    ))
    .filter_enabled(notifications);
    assert_eq!(preview.rows.len(), 1);
    assert!(preview.rows[0].category.is_none());
}

#[test]
fn microphone_row_uses_volume_terminology() {
    let rendered = microphone_row(&crate::audio::AudioState::Active { volume_pct: 42 });
    assert!(rendered.detail.contains("input volume"));
    assert!(!rendered.detail.contains("input level"));
}

#[test]
fn coalesced_timing_preserves_full_settled_hold() {
    let appearing_early = timing_after_show(
        Phase::Appearing,
        10,
        MotionPolicy::Animated,
        true,
        OverlayLifetime::Transient,
        true,
        1300,
    );
    assert_eq!(appearing_early.phase, Phase::Appearing);
    assert!(!appearing_early.restart_phase);
    assert_eq!(appearing_early.hold_after_now_ms, Some(1430));

    let appearing_late = timing_after_show(
        Phase::Appearing,
        139,
        MotionPolicy::Animated,
        true,
        OverlayLifetime::Transient,
        true,
        1300,
    );
    assert_eq!(appearing_late.hold_after_now_ms, Some(1301));

    let holding = timing_after_show(
        Phase::Holding,
        0,
        MotionPolicy::Animated,
        true,
        OverlayLifetime::Transient,
        true,
        1300,
    );
    assert_eq!(holding.phase, Phase::Holding);
    assert!(!holding.restart_phase);
    assert_eq!(holding.hold_after_now_ms, Some(1300));

    let leaving = timing_after_show(
        Phase::Leaving,
        40,
        MotionPolicy::Animated,
        true,
        OverlayLifetime::Transient,
        true,
        1300,
    );
    assert_eq!(leaving.phase, Phase::Holding);
    assert!(leaving.restart_phase);
    assert_eq!(leaving.hold_after_now_ms, Some(1300));

    let reduced = timing_after_show(
        Phase::Appearing,
        10,
        MotionPolicy::Reduced,
        true,
        OverlayLifetime::Transient,
        true,
        1300,
    );
    assert_eq!(reduced.phase, Phase::Holding);
    assert!(reduced.restart_phase);
    assert_eq!(reduced.hold_after_now_ms, Some(1300));

    let fresh = timing_after_show(
        Phase::Hidden,
        0,
        MotionPolicy::Animated,
        false,
        OverlayLifetime::Transient,
        true,
        1300,
    );
    assert_eq!(fresh.phase, Phase::Appearing);
    assert!(fresh.restart_phase);
    assert_eq!(fresh.hold_after_now_ms, Some(1440));
}

#[test]
fn sticky_microphone_timing_has_no_auto_hide_deadline() {
    let timing = timing_after_show(
        Phase::Holding,
        0,
        MotionPolicy::Animated,
        true,
        OverlayLifetime::Sticky(OverlayIcon::Microphone),
        false,
        1300,
    );

    assert_eq!(timing.phase, Phase::Holding);
    assert_eq!(timing.hold_after_now_ms, None);
}

#[test]
fn sticky_microphone_with_transient_rows_uses_configured_expiry() {
    let timing = timing_after_show(
        Phase::Holding,
        0,
        MotionPolicy::Animated,
        true,
        OverlayLifetime::Sticky(OverlayIcon::Microphone),
        true,
        1300,
    );

    assert_eq!(timing.hold_after_now_ms, Some(1300));
}

#[test]
fn repeated_transient_updates_refresh_sticky_owner_expiry() {
    let first = timing_after_show(
        Phase::Holding,
        0,
        MotionPolicy::Animated,
        true,
        OverlayLifetime::Sticky(OverlayIcon::Microphone),
        true,
        1300,
    );
    let refreshed = timing_after_show(
        Phase::Holding,
        0,
        MotionPolicy::Animated,
        true,
        OverlayLifetime::Sticky(OverlayIcon::Microphone),
        true,
        2200,
    );

    assert_eq!(first.hold_after_now_ms, Some(1300));
    assert_eq!(refreshed.hold_after_now_ms, Some(2200));
}

#[test]
fn transient_status_and_unmuted_microphone_use_normal_expiry() {
    for model in [
        OverlayModel::from_rows(vec![row(OverlayIcon::Microphone, "muted")]),
        OverlayModel::single(row(OverlayIcon::Microphone, "unmuted")),
    ] {
        assert!(!model.is_sticky());
        let timing = timing_after_show(
            Phase::Holding,
            0,
            MotionPolicy::Animated,
            true,
            OverlayLifetime::Transient,
            false,
            1300,
        );
        assert_eq!(timing.hold_after_now_ms, Some(1300));
    }
}

#[test]
fn state_plan_preparation_releases_borrow_before_reentrant_window_work() {
    let cell = std::cell::RefCell::new(0_u8);
    let plan = prepare_state_plan(&cell, |state| {
        *state = 1;
        ShowPlan {
            position: POINT { x: 40, y: 80 },
            size: SIZE { cx: 320, cy: 180 },
            region: WindowRegion {
                width: 320,
                height: 180,
                inset: 0,
                corner_diameter: 28,
            },
            alpha: 1.0,
            timer_interval: TIMER_MS,
        }
    });

    assert_eq!(plan.position, POINT { x: 40, y: 80 });
    assert_eq!(plan.size, SIZE { cx: 320, cy: 180 });
    let mut reentrant = cell
        .try_borrow_mut()
        .expect("state borrow must end before window work");
    *reentrant = 2;
    assert_eq!(*reentrant, 2);
}

#[test]
fn surface_geometry_keeps_body_inside_final_surface() {
    for scale in [0.7, 1.0, 1.6] {
        for dpi in [96, 144, 192] {
            let geometry = surface_geometry(scale, 3);
            let right_padding = geometry.width - geometry.body_right;
            let bottom_padding = geometry.height - geometry.body_bottom;
            assert!(geometry.body_left >= 0.0);
            assert!(geometry.body_top >= 0.0);
            assert!(geometry.body_right <= geometry.width);
            assert!(geometry.body_bottom <= geometry.height);
            assert!((geometry.body_left - right_padding).abs() < f32::EPSILON);
            assert!((geometry.body_top - bottom_padding).abs() < f32::EPSILON);
            assert!(geometry.body_left.abs() < f32::EPSILON);
            let pixels = geometry.pixel_size(dpi);
            let dpi_scale = dpi as f32 / 96.0;
            assert!(geometry.body_right * dpi_scale <= pixels.cx as f32);
            assert!(geometry.body_bottom * dpi_scale <= pixels.cy as f32);
        }
    }
}

#[test]
fn device_cycle_rows_report_real_system_endpoint() {
    let device = crate::audio::DeviceId {
        endpoint: "opaque-id".into(),
        name: "USB Microphone".into(),
    };
    let row = device_cycle_row(crate::audio::DeviceCycleFlow::Input, &device);
    assert_eq!(row.title, "Next microphone");
    assert_eq!(row.detail, "USB Microphone");
}

#[test]
fn runtime_audio_rows_use_canonical_device_names() {
    for (name, expected) in [
        (
            "3 - SAMSUNG (2- AMD High Definition Audio Device)",
            "SAMSUNG",
        ),
        ("GS25F2 (AMD High Definition Audio Device)", "GS25F2"),
    ] {
        let row = output_row(&crate::audio::OutputState::Current {
            device: crate::audio::DeviceId {
                endpoint: "opaque-output-id".into(),
                name: name.into(),
            },
            muted: false,
            volume_pct: 50,
        });
        assert_eq!(row.title, expected);
    }

    let row = device_cycle_row(
        crate::audio::DeviceCycleFlow::Input,
        &crate::audio::DeviceId {
            endpoint: "opaque-input-id".into(),
            name: "Microphone (SIMGOT EW300 DSP)".into(),
        },
    );
    assert_eq!(row.detail, "SIMGOT EW300 DSP");
}

#[test]
fn long_output_names_are_bounded_before_overlay_rendering() {
    let row = output_row(&crate::audio::OutputState::Current {
        device: crate::audio::DeviceId {
            endpoint: "opaque-id".into(),
            name: "A".repeat(100),
        },
        muted: false,
        volume_pct: 50,
    });
    assert_eq!(row.title.chars().count(), 58);
    assert!(row.title.ends_with('…'));
}

#[test]
fn volume_rows_show_exact_values_ranges_and_no_session_state() {
    let exact = application_volume_row(&crate::audio::AppVolumeState {
        app_name: Some("Player".into()),
        sessions: 1,
        min_volume_pct: Some(65),
        max_volume_pct: Some(65),
        error: None,
    });
    assert_eq!(exact.detail, "Player · Volume 65%");

    let range = application_volume_row(&crate::audio::AppVolumeState {
        app_name: Some("Player".into()),
        sessions: 2,
        min_volume_pct: Some(45),
        max_volume_pct: Some(70),
        error: None,
    });
    assert_eq!(range.detail, "Player · Volume 45–70%");

    let empty = application_volume_row(&crate::audio::AppVolumeState::no_session(Some(
        "Player".into(),
    )));
    assert_eq!(empty.detail, "Player · No active audio session");
    let no_external = application_volume_row(&crate::audio::AppVolumeState::no_external());
    assert_eq!(no_external.detail, "No current app with audio");
}

#[test]
fn center_edge_positions_use_the_work_area_axes() {
    let work = RECT {
        left: 0,
        top: 0,
        right: 1000,
        bottom: 800,
    };
    let size = SIZE { cx: 100, cy: 80 };
    assert_eq!(
        position_for(work, size, OverlayPosition::CenterLeft, 96),
        POINT { x: 22, y: 360 }
    );
    assert_eq!(
        position_for(work, size, OverlayPosition::CenterRight, 96),
        POINT { x: 878, y: 360 }
    );
}
