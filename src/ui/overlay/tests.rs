use super::manager::{OverlayKey, OverlayLifetime, OverlayRegistry};
use super::*;
use std::time::{Duration, Instant};
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

fn registry_entry_title(registry: &OverlayRegistry, key: OverlayKey) -> &str {
    registry
        .entries()
        .iter()
        .find(|entry| entry.key() == key)
        .expect("registry entry should exist")
        .model()
        .rows[0]
        .title
        .as_str()
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
fn permanent_microphone_entry_is_unique_and_never_expires() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    let first = registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "muted")),
        ),
        Duration::from_millis(1300),
        now,
    );
    let refreshed = registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "still muted")),
        ),
        Duration::from_millis(1300),
        now + Duration::from_secs(2),
    );

    assert!(first.inserted);
    assert!(!refreshed.inserted);
    assert!(!refreshed.restart_appearance);
    assert_eq!(first.id, refreshed.id);
    assert_eq!(registry.entries().len(), 1);
    let entry = &registry.entries()[0];
    assert_eq!(entry.key(), OverlayKey::MicrophonePermanent);
    assert_eq!(entry.lifetime(), OverlayLifetime::Permanent);
    assert_eq!(entry.expires_at(), None);
    assert_eq!(entry.model().rows[0].title, "still muted");
}

#[test]
fn unmute_removes_permanent_entry_before_creating_a_new_toast() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "muted")),
        ),
        Duration::from_millis(1300),
        now,
    );
    assert_eq!(
        registry.remove_key(OverlayKey::MicrophonePermanent),
        vec![1]
    );
    let toast = registry.present(
        OverlayRequest::toast(
            OverlayKey::MicrophoneToast,
            OverlayModel::single(row(OverlayIcon::Microphone, "unmuted")),
        ),
        Duration::from_millis(1300),
        now,
    );

    assert!(toast.inserted);
    assert_eq!(registry.entries().len(), 1);
    assert_eq!(registry.entries()[0].key(), OverlayKey::MicrophoneToast);
    assert!(matches!(
        registry.entries()[0].lifetime(),
        OverlayLifetime::Toast
    ));
}

#[test]
fn permanent_microphone_and_speaker_toast_are_independent() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    let microphone = registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "muted")),
        ),
        Duration::from_millis(1300),
        now,
    );
    let speaker = registry.present(
        OverlayRequest::toast(
            OverlayKey::Speaker,
            OverlayModel::single(row(OverlayIcon::Output, "speaker")),
        ),
        Duration::from_millis(1300),
        now,
    );

    assert_eq!(registry.entries().len(), 2);
    assert_eq!(
        registry
            .remove_expired(now + Duration::from_millis(1300))
            .len(),
        1
    );
    assert!(registry.entries().iter().any(|entry| {
        entry.id() == microphone.id
            && entry.key() == OverlayKey::MicrophonePermanent
            && entry.expires_at().is_none()
    }));
    assert_eq!(speaker.id, 2);
}

#[test]
fn permanent_microphone_and_workspace_toast_are_independent() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    let microphone = registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "muted")),
        ),
        Duration::from_millis(1300),
        now,
    );
    let workspace = registry.present(
        OverlayRequest::toast(
            OverlayKey::Workspace,
            OverlayModel::single(row(OverlayIcon::Workspace, "workspace")),
        ),
        Duration::from_millis(1300),
        now,
    );

    assert_eq!(registry.entries().len(), 2);
    assert_eq!(
        registry.remove_id(workspace.id).unwrap().key(),
        OverlayKey::Workspace
    );
    assert_eq!(registry.entries().len(), 1);
    assert_eq!(registry.entries()[0].id(), microphone.id);
    assert!(registry.entries()[0].expires_at().is_none());
}

#[test]
fn workspace_toast_replaces_the_single_semantic_entry() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    let first = registry.present(
        OverlayRequest::toast(
            OverlayKey::Workspace,
            OverlayModel::single(row(OverlayIcon::Workspace, "first")),
        ),
        Duration::from_millis(1300),
        now,
    );
    let second = registry.present(
        OverlayRequest::toast(
            OverlayKey::Workspace,
            OverlayModel::single(row(OverlayIcon::Workspace, "second")),
        ),
        Duration::from_millis(1300),
        now + Duration::from_millis(100),
    );

    assert!(first.inserted);
    assert!(!second.inserted);
    assert_eq!(first.id, second.id);
    assert_eq!(registry.entries().len(), 1);
    assert_eq!(registry.entries()[0].model().rows[0].title, "second");
}

#[test]
fn repeated_device_presentations_replace_one_entry_for_each_device_key() {
    let now = Instant::now();
    for key in [OverlayKey::InputDevice, OverlayKey::OutputDevice] {
        let mut registry = OverlayRegistry::default();
        let icon = if key == OverlayKey::InputDevice {
            OverlayIcon::Microphone
        } else {
            OverlayIcon::Output
        };
        let mut last = None;
        for index in 0..100 {
            last = Some(registry.present(
                OverlayRequest::toast(
                    key,
                    OverlayModel::single(row(icon, &format!("device-{index}"))),
                ),
                Duration::from_millis(1300),
                now + Duration::from_millis(index),
            ));
        }
        let last = last.expect("device presentations should produce an outcome");
        assert_eq!(registry.entries().len(), 1);
        assert_eq!(registry.entries()[0].key(), key);
        assert_eq!(registry.entries()[0].model().rows[0].title, "device-99");
        assert_eq!(
            registry.entries()[0].expires_at(),
            Some(now + Duration::from_millis(99 + 1300))
        );
        assert_eq!(registry.entries()[0].sequence(), 100);
        assert_eq!(last.id, 1, "same key must reuse its card identity");
        assert!(registry.has_unique_keys());
    }
}

#[test]
fn repeated_display_and_speaker_presentations_keep_singleton_keys() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    for (key, icon) in [
        (OverlayKey::DisplayProfile, OverlayIcon::Info),
        (OverlayKey::Speaker, OverlayIcon::Output),
    ] {
        registry.present(
            OverlayRequest::toast(key, OverlayModel::single(row(icon, "first"))),
            Duration::from_millis(1000),
            now,
        );
        registry.present(
            OverlayRequest::toast(key, OverlayModel::single(row(icon, "latest"))),
            Duration::from_millis(1000),
            now + Duration::from_millis(100),
        );
    }
    assert_eq!(registry.entries().len(), 2);
    assert!(registry
        .entries()
        .iter()
        .all(|entry| entry.model().rows[0].title == "latest"));
    assert!(registry.has_unique_keys());
}

#[test]
fn semantic_key_set_is_the_registry_card_upper_bound() {
    let keys = OverlayKey::ALL;
    let unique = keys
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(keys.len(), 10);
    assert_eq!(unique.len(), keys.len());
}

#[test]
fn one_toast_expiry_does_not_remove_another_toast() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    registry.present(
        OverlayRequest::toast(
            OverlayKey::Speaker,
            OverlayModel::single(row(OverlayIcon::Output, "speaker")),
        ),
        Duration::from_millis(1000),
        now,
    );
    registry.present(
        OverlayRequest::toast(
            OverlayKey::Workspace,
            OverlayModel::single(row(OverlayIcon::Workspace, "workspace")),
        ),
        Duration::from_millis(1000),
        now + Duration::from_millis(300),
    );

    let expired = registry.remove_expired(now + Duration::from_millis(1100));
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].key(), OverlayKey::Speaker);
    assert_eq!(registry.entries().len(), 1);
    assert_eq!(registry.entries()[0].key(), OverlayKey::Workspace);
}

#[test]
fn replace_same_key_updates_one_toast_and_resets_only_its_deadline() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    let first = registry.present(
        OverlayRequest::toast(
            OverlayKey::Speaker,
            OverlayModel::single(row(OverlayIcon::Output, "old speaker")),
        ),
        Duration::from_millis(1000),
        now,
    );
    let other = registry.present(
        OverlayRequest::toast(
            OverlayKey::Workspace,
            OverlayModel::single(row(OverlayIcon::Workspace, "workspace")),
        ),
        Duration::from_millis(1600),
        now,
    );
    let replaced = registry.present(
        OverlayRequest::toast(
            OverlayKey::Speaker,
            OverlayModel::single(row(OverlayIcon::Output, "new speaker")),
        ),
        Duration::from_millis(2000),
        now + Duration::from_millis(500),
    );

    assert!(!replaced.inserted);
    assert!(replaced.restart_appearance);
    assert_eq!(replaced.id, first.id);
    assert_eq!(registry.entries().len(), 2);
    let speaker = registry
        .entries()
        .iter()
        .find(|entry| entry.key() == OverlayKey::Speaker)
        .unwrap();
    let workspace = registry
        .entries()
        .iter()
        .find(|entry| entry.key() == OverlayKey::Workspace)
        .unwrap();
    assert_eq!(speaker.model().rows[0].title, "new speaker");
    assert_eq!(
        speaker.expires_at(),
        Some(now + Duration::from_millis(2500))
    );
    assert_eq!(workspace.id(), other.id);
    assert_eq!(
        workspace.expires_at(),
        Some(now + Duration::from_millis(1600))
    );
    assert!(speaker.sequence() > workspace.sequence());
}

#[test]
fn status_and_preview_are_separate_transient_entries() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "muted")),
        ),
        Duration::from_millis(1300),
        now,
    );
    registry.present(
        OverlayRequest::toast(
            OverlayKey::Status,
            OverlayModel::from_rows(vec![
                row(OverlayIcon::Microphone, "mic"),
                row(OverlayIcon::Output, "speaker"),
            ]),
        ),
        Duration::from_millis(1300),
        now,
    );
    registry.present(
        OverlayRequest::toast(
            OverlayKey::Preview,
            OverlayModel::preview(OverlayRow::preview("preview", "preview detail")),
        ),
        Duration::from_millis(1300),
        now,
    );

    assert_eq!(registry.entries().len(), 3);
    assert_eq!(registry.entries()[0].key(), OverlayKey::MicrophonePermanent);
    assert_eq!(registry.entries()[1].model().rows.len(), 2);
    assert_eq!(registry.entries()[2].key(), OverlayKey::Preview);
    assert!(registry.entries()[0].expires_at().is_none());
    assert!(registry.entries()[1].expires_at().is_some());
    assert!(registry.entries()[2].expires_at().is_some());
}

#[test]
fn status_and_preview_replacements_are_independent_singletons() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    let status = registry.present(
        OverlayRequest::toast(
            OverlayKey::Status,
            OverlayModel::single(row(OverlayIcon::Info, "status one")),
        ),
        Duration::from_millis(1000),
        now,
    );
    let preview = registry.present(
        OverlayRequest::toast(
            OverlayKey::Preview,
            OverlayModel::preview(OverlayRow::preview("preview one", "detail")),
        ),
        Duration::from_millis(1000),
        now,
    );
    let status_replacement = registry.present(
        OverlayRequest::toast(
            OverlayKey::Status,
            OverlayModel::single(row(OverlayIcon::Info, "status two")),
        ),
        Duration::from_millis(2000),
        now + Duration::from_millis(100),
    );
    let preview_replacement = registry.present(
        OverlayRequest::toast(
            OverlayKey::Preview,
            OverlayModel::preview(OverlayRow::preview("preview two", "detail")),
        ),
        Duration::from_millis(2000),
        now + Duration::from_millis(100),
    );

    assert_eq!(registry.entries().len(), 2);
    assert_eq!(status.id, status_replacement.id);
    assert_eq!(preview.id, preview_replacement.id);
    assert_eq!(
        registry_entry_title(&registry, OverlayKey::Status),
        "status two"
    );
    assert_eq!(
        registry_entry_title(&registry, OverlayKey::Preview),
        "preview two"
    );
}

#[test]
fn status_and_preview_expiry_leave_permanent_microphone_untouched() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "muted")),
        ),
        Duration::from_millis(1300),
        now,
    );
    registry.present(
        OverlayRequest::toast(
            OverlayKey::Status,
            OverlayModel::from_rows(vec![row(OverlayIcon::Microphone, "mic")]),
        ),
        Duration::from_millis(1000),
        now,
    );
    registry.present(
        OverlayRequest::toast(
            OverlayKey::Preview,
            OverlayModel::preview(OverlayRow::preview("preview", "detail")),
        ),
        Duration::from_millis(1500),
        now,
    );

    let expired = registry.remove_expired(now + Duration::from_millis(1100));
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].key(), OverlayKey::Status);
    assert!(registry
        .entries()
        .iter()
        .any(|entry| entry.key() == OverlayKey::MicrophonePermanent));
    assert!(registry
        .entries()
        .iter()
        .any(|entry| entry.key() == OverlayKey::Preview));
}

#[test]
fn disabled_notification_categories_are_removed_before_rendering() {
    let model = OverlayModel::from_rows(vec![
        row(OverlayIcon::Microphone, "mic"),
        row(OverlayIcon::Output, "speaker"),
        row(OverlayIcon::Application, "app"),
    ]);
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
        let filtered = OverlayModel::single(microphone_row(&state)).filter_enabled(notifications);
        assert!(filtered.rows.is_empty());
    }
}

#[test]
fn microphone_category_filter_removes_the_permanent_entry() {
    let model = OverlayModel::single(row(OverlayIcon::Microphone, "muted"));
    let notifications = crate::config::model::OverlayNotifications {
        microphone: false,
        ..Default::default()
    };

    assert!(model.filter_enabled(notifications).rows.is_empty());
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
fn category_filter_removes_matching_entries_but_keeps_preview() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    let microphone = registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "muted")),
        ),
        Duration::from_millis(1300),
        now,
    );
    let microphone_toast = registry.present(
        OverlayRequest::toast(
            OverlayKey::MicrophoneToast,
            OverlayModel::single(row(OverlayIcon::Microphone, "unmuted")),
        ),
        Duration::from_millis(1300),
        now,
    );
    let speaker = registry.present(
        OverlayRequest::toast(
            OverlayKey::Speaker,
            OverlayModel::single(row(OverlayIcon::Output, "speaker")),
        ),
        Duration::from_millis(1300),
        now,
    );
    let preview = registry.present(
        OverlayRequest::toast(
            OverlayKey::Preview,
            OverlayModel::preview(OverlayRow::preview("preview", "detail")),
        ),
        Duration::from_millis(1300),
        now,
    );

    let notifications = crate::config::model::OverlayNotifications {
        microphone: false,
        ..Default::default()
    };
    assert_eq!(
        registry.filter_notifications(notifications),
        vec![microphone.id, microphone_toast.id]
    );
    assert!(!registry
        .entries()
        .iter()
        .any(|entry| entry.key() == OverlayKey::MicrophonePermanent));
    assert!(registry
        .entries()
        .iter()
        .any(|entry| entry.id() == speaker.id));
    assert!(registry
        .entries()
        .iter()
        .any(|entry| entry.id() == preview.id));
}

#[test]
fn global_clear_then_restore_rebuilds_permanent_state_without_old_toasts() {
    let now = Instant::now();
    let mut registry = OverlayRegistry::default();
    registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "muted")),
        ),
        Duration::from_millis(1300),
        now,
    );
    registry.present(
        OverlayRequest::toast(
            OverlayKey::Workspace,
            OverlayModel::single(row(OverlayIcon::Workspace, "expired later")),
        ),
        Duration::from_millis(1300),
        now,
    );
    registry.clear();
    assert!(registry.entries().is_empty());

    registry.present(
        OverlayRequest::permanent(
            OverlayKey::MicrophonePermanent,
            OverlayModel::single(row(OverlayIcon::Microphone, "muted")),
        ),
        Duration::from_millis(1300),
        now + Duration::from_secs(2),
    );
    assert_eq!(registry.entries().len(), 1);
    assert_eq!(registry.entries()[0].key(), OverlayKey::MicrophonePermanent);
    assert!(registry.entries()[0].expires_at().is_none());
}

#[test]
fn microphone_row_uses_volume_terminology() {
    let rendered = microphone_row(&crate::audio::AudioState::Active { volume_pct: 42 });
    assert!(rendered.detail.contains("input volume"));
    assert!(!rendered.detail.contains("input level"));
}

#[test]
fn card_timing_is_independent_and_relayout_does_not_restart_animation() {
    let fresh = timing_after_show(Phase::Hidden, MotionPolicy::Animated, ShowMode::Present);
    assert_eq!(fresh.phase, Phase::Appearing);
    assert!(fresh.restart_phase);

    let moved = timing_after_show(Phase::Holding, MotionPolicy::Animated, ShowMode::Relayout);
    assert_eq!(moved.phase, Phase::Holding);
    assert!(!moved.restart_phase);

    let replaced = timing_after_show(Phase::Leaving, MotionPolicy::Animated, ShowMode::Present);
    assert_eq!(replaced.phase, Phase::Appearing);
    assert!(replaced.restart_phase);

    let reduced = timing_after_show(Phase::Hidden, MotionPolicy::Reduced, ShowMode::Present);
    assert_eq!(reduced.phase, Phase::Holding);
    assert!(reduced.restart_phase);
}

#[test]
fn toast_deadline_reserves_appear_only_for_animated_motion() {
    let started = Instant::now();
    let hold = Duration::from_millis(1000);
    assert_eq!(
        toast_deadline(started, MotionPolicy::Animated, hold),
        started + Duration::from_millis(APPEAR_MS + 1000)
    );
    assert_eq!(
        toast_deadline(started, MotionPolicy::Reduced, hold),
        started + hold
    );
}

#[test]
fn stationary_holding_toast_arms_its_deadline_instead_of_animation_ticks() {
    let now = Instant::now();
    let deadline = now + Duration::from_secs(5);
    assert_eq!(
        timer_interval_for_card(
            Phase::Holding,
            MotionPolicy::Animated,
            false,
            Some(deadline),
            now,
        ),
        Some(5_000)
    );
}

#[test]
fn animation_and_position_tween_keep_the_frame_timer() {
    let now = Instant::now();
    let deadline = now + Duration::from_secs(5);
    for phase in [Phase::Appearing, Phase::Leaving] {
        assert_eq!(
            timer_interval_for_card(
                phase,
                MotionPolicy::Animated,
                false,
                Some(deadline),
                now,
            ),
            Some(TIMER_MS)
        );
    }
    assert_eq!(
        timer_interval_for_card(
            Phase::Holding,
            MotionPolicy::Animated,
            true,
            Some(deadline),
            now,
        ),
        Some(TIMER_MS)
    );
}

#[test]
fn permanent_holding_card_has_no_timer_and_reduced_motion_waits_for_expiry() {
    let now = Instant::now();
    assert_eq!(
        timer_interval_for_card(Phase::Holding, MotionPolicy::Animated, false, None, now),
        None
    );
    assert_eq!(
        timer_interval_for_card(
            Phase::Holding,
            MotionPolicy::Reduced,
            false,
            Some(now + Duration::from_millis(2_500)),
            now,
        ),
        Some(2_500)
    );
}

#[test]
fn expired_holding_deadline_rearms_promptly_for_transition() {
    let now = Instant::now();
    assert_eq!(
        timer_interval_for_card(
            Phase::Holding,
            MotionPolicy::Animated,
            false,
            Some(now),
            now,
        ),
        Some(1)
    );
}

#[test]
fn relayout_position_tween_is_smooth_and_reduced_motion_is_immediate() {
    let started = Instant::now();
    let from = POINT { x: 0, y: 0 };
    let to = POINT { x: 100, y: 100 };
    let tween = PositionTween::start(from, to, MotionPolicy::Animated, started)
        .expect("animated movement should create a tween");
    let middle = tween.position_at(started + Duration::from_millis(POSITION_TWEEN_MS / 2));
    assert!(middle.x > from.x && middle.x < to.x);
    assert_eq!(
        tween.position_at(started + Duration::from_millis(POSITION_TWEEN_MS)),
        to
    );
    assert!(PositionTween::start(from, to, MotionPolicy::Reduced, started).is_none());
}

#[test]
fn timer_identity_changes_with_card_generation() {
    assert_eq!(
        super::timeline::timer_id_for_generation(0),
        super::timeline::TIMER_ID
    );
    assert_ne!(
        super::timeline::timer_id_for_generation(1),
        super::timeline::timer_id_for_generation(2)
    );
}

#[test]
fn permanent_and_toast_layout_has_stable_slots_and_no_overlap() {
    let work = RECT {
        left: 0,
        top: 0,
        right: 1000,
        bottom: 800,
    };
    let inputs = [
        LayoutInput {
            size: SIZE { cx: 100, cy: 60 },
        },
        LayoutInput {
            size: SIZE { cx: 100, cy: 60 },
        },
        LayoutInput {
            size: SIZE { cx: 100, cy: 90 },
        },
    ];
    let top = layout_cards(work, OverlayPosition::TopLeft, 96, 1.0, &inputs);
    let top_without_toasts = layout_cards(work, OverlayPosition::TopLeft, 96, 1.0, &inputs[..1]);

    assert_eq!(top[0].position, top_without_toasts[0].position);
    assert!(top[0].position.y < top[1].position.y);
    assert!(top[1].position.y < top[2].position.y);
    for (index, pair) in top.windows(2).enumerate() {
        let height = [60, 60][index];
        assert!(pair[0].position.y + height + 16 <= pair[1].position.y);
    }

    let bottom_inputs = [inputs[1], inputs[2]];
    let bottom = layout_cards(work, OverlayPosition::BottomRight, 96, 1.0, &bottom_inputs);
    assert!(bottom[0].position.y > bottom[1].position.y);
    assert!(bottom[1].position.y + 90 + 16 <= bottom[0].position.y);

    let center = layout_cards(work, OverlayPosition::Center, 96, 1.0, &inputs[..2]);
    assert!(center[1].position.y > center[0].position.y);
    assert!(center[0].position.y + 60 + 16 <= center[1].position.y);
}

#[test]
fn oversized_card_is_clamped_inside_the_work_area() {
    let work = RECT {
        left: 10,
        top: 20,
        right: 100,
        bottom: 90,
    };
    let placements = layout_cards(
        work,
        OverlayPosition::BottomRight,
        96,
        1.0,
        &[LayoutInput {
            size: SIZE { cx: 200, cy: 200 },
        }],
    );
    assert_eq!(placements[0].position, POINT { x: 10, y: 20 });
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
            timer_id: super::timeline::TIMER_ID,
            timer_interval: Some(TIMER_MS),
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
