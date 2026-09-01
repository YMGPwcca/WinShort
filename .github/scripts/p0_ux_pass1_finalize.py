from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8", newline="\n")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    found = text.count(old)
    if found != 1:
        raise SystemExit(f"{path}: expected one match, found {found}: {old[:100]!r}")
    write(path, text.replace(old, new))


def scoped_replace(path: str, start: str, end: str, old: str, new: str) -> None:
    text = read(path)
    a = text.index(start)
    b = text.index(end, a)
    chunk = text[a:b]
    found = chunk.count(old)
    if found != 1:
        raise SystemExit(f"{path}: scoped expected one match, found {found}: {start!r}")
    write(path, text[:a] + chunk.replace(old, new) + text[b:])


# Native LISTBOX theme API is a Win32 unsafe boundary.
replace_once(
    "src/ui/picker.rs",
    "            let _ = SetWindowTheme(list, PCWSTR(theme_name.as_ptr()), PCWSTR::null());",
    "            let _ = unsafe { SetWindowTheme(list, PCWSTR(theme_name.as_ptr()), PCWSTR::null()) };",
)

# External Windows audio changes use the Windows OSD. Keep WinShort overlays for
# WinShort actions/status queries, but do not mirror external state changes.
for start, end in [
    (
        "            AppEvent::MicrophoneStateChanged { state, origin } => {",
        "            AppEvent::OutputStateChanged { state, origin } => {",
    ),
    (
        "            AppEvent::OutputStateChanged { state, origin } => {",
        "            AppEvent::DefaultOutputChanged(",
    ),
    (
        "            AppEvent::ForegroundAudioChanged { state, origin } => {",
        "            AppEvent::ForegroundVolumeChanged {",
    ),
]:
    scoped_replace(
        "src/app.rs",
        start,
        end,
        "                let config = crate::app::config();\n",
        "",
    )

replace_once(
    "src/app.rs",
    '''            AppEvent::DefaultOutputChanged(device) => {\n                let row = crate::ui::overlay::output_changed_row(&device);\n                self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));\n            }\n''',
    '''            AppEvent::DefaultOutputChanged(_device) => {}\n''',
)
replace_once(
    "src/ui/overlay.rs",
    '''/// Transient "speaker changed" card (#17b).\npub fn output_changed_row(device: &crate::audio::state::DeviceId) -> OverlayRow {\n    OverlayRow {\n        icon: OverlayIcon::Output,\n        tone: OverlayTone::Changed,\n        title: "Speaker changed".into(),\n        detail: concise(&device.name),\n    }\n}\n\n''',
    "",
)

# The available Special Workspace state is normal, so remove the Ready badge
# and center the useful title/description as one block.
replace_once(
    "src/ui/control_center.rs",
    '''        renderer.text(\n            "Special Workspace",\n            UiRect::new(rect.x + 16.0, rect.y + 12.0, rect.w - 180.0, 22.0).d2d(),\n            TextStyle::BodyStrong,\n            BrushRole::Text,\n        );\n        renderer.text_clipped(\n            detail,\n            UiRect::new(rect.x + 16.0, rect.y + 40.0, rect.w - 180.0, 22.0).d2d(),\n            TextStyle::Caption,\n            BrushRole::TextSecondary,\n        );\n''',
    '''        let text_width = if status.is_empty() { rect.w - 32.0 } else { rect.w - 180.0 };\n        let stack_top = rect.y + (rect.h - 40.0) * 0.5;\n        renderer.text(\n            "Special Workspace",\n            UiRect::new(rect.x + 16.0, stack_top, text_width, 20.0).d2d(),\n            TextStyle::BodyStrong,\n            BrushRole::Text,\n        );\n        renderer.text_clipped(\n            detail,\n            UiRect::new(rect.x + 16.0, stack_top + 22.0, text_width, 18.0).d2d(),\n            TextStyle::Caption,\n            BrushRole::TextSecondary,\n        );\n''',
)

# Contract tests: the selected row is enough to communicate the current system
# endpoint. Do not reintroduce redundant default/explicit badges in text.
replace_once(
    "src/ui/control_center.rs",
    '''        assert_eq!(\n            choices[0].label,\n            "Current microphone · Currently Windows default"\n        );\n''',
    '''        assert_eq!(choices[0].label, "Current microphone");\n''',
)
scoped_replace(
    "src/ui/control_center.rs",
    "    fn value_for_uses_cached_devices_without_reacquiring_app() {",
    "    #[test]\n    fn high_contrast_settings_theme_uses_system_pairs_for_hover_and_focus() {",
    "            input_defaults: Default::default(),\n",
    '''            input_defaults: crate::audio::devices::DefaultDevices {\n                console: Some(crate::audio::DeviceId {\n                    endpoint: "input".into(),\n                    name: "Cached microphone".into(),\n                }),\n                ..Default::default()\n            },\n''',
)

replace_once(
    "src/ui/presentation.rs",
    '''    #[test]\n    fn explicit_selection_badge_is_distinct_from_following_default() {\n        let current = device("current");\n        let following = device_selection_presentation(\n            &DeviceSelection::Default,\n            std::slice::from_ref(&current),\n            Some(&current),\n            AudioDeviceKind::Speaker,\n        );\n        assert_eq!(following.primary, "current");\n        assert_eq!(following.secondary, None);\n        assert_eq!(following.status.as_deref(), Some("Windows system default"));\n\n        let explicit = device_selection_presentation(\n            &DeviceSelection::Endpoint("current".into()),\n            std::slice::from_ref(&current),\n            Some(&current),\n            AudioDeviceKind::Speaker,\n        );\n        assert_eq!(explicit.primary, "current");\n        assert_eq!(explicit.secondary, None);\n        assert_eq!(\n            explicit.status.as_deref(),\n            Some("Currently Windows default")\n        );\n    }\n''',
    '''    #[test]\n    fn default_and_legacy_explicit_render_the_same_system_endpoint() {\n        let current = device("current");\n        let following = device_selection_presentation(\n            &DeviceSelection::Default,\n            std::slice::from_ref(&current),\n            Some(&current),\n            AudioDeviceKind::Speaker,\n        );\n        let legacy_explicit = device_selection_presentation(\n            &DeviceSelection::Endpoint("current".into()),\n            std::slice::from_ref(&current),\n            Some(&current),\n            AudioDeviceKind::Speaker,\n        );\n        assert_eq!(following.primary, "current");\n        assert_eq!(following.secondary, None);\n        assert_eq!(following.status, None);\n        assert_eq!(legacy_explicit.primary, following.primary);\n        assert_eq!(legacy_explicit.secondary, following.secondary);\n        assert_eq!(legacy_explicit.status, None);\n    }\n''',
)
replace_once(
    "src/ui/presentation.rs",
    '''    #[test]\n    fn audio_selection_keeps_metadata_and_status_as_separate_fields() {\n        let current = device("speaker");\n        let mut named = current.clone();\n        named.name = "3 - SAMSUNG (2- AMD High Definition Audio Device)".into();\n        let devices = [named.clone()];\n        let following = device_selection_presentation(\n            &DeviceSelection::Default,\n            &devices,\n            Some(&named),\n            AudioDeviceKind::Speaker,\n        );\n        assert_eq!(following.primary, "SAMSUNG");\n        assert_eq!(\n            following.secondary.as_deref(),\n            Some("AMD High Definition Audio Device")\n        );\n        assert_eq!(following.status.as_deref(), Some("Windows system default"));\n\n        let explicit = device_selection_presentation(\n            &DeviceSelection::Endpoint(named.endpoint.clone()),\n            &devices,\n            Some(&named),\n            AudioDeviceKind::Speaker,\n        );\n        assert_eq!(explicit.primary, "SAMSUNG");\n        assert_eq!(\n            explicit.secondary.as_deref(),\n            Some("AMD High Definition Audio Device")\n        );\n        assert_eq!(\n            explicit.status.as_deref(),\n            Some("Currently Windows default")\n        );\n        assert!(explicit\n            .accessible_value()\n            .contains("Currently Windows default"));\n        assert_eq!(\n            device_choice_label(&named, Some(&named), AudioDeviceKind::Speaker),\n            "SAMSUNG · Currently Windows default"\n        );\n    }\n''',
    '''    #[test]\n    fn audio_selection_keeps_device_identity_without_redundant_status_badges() {\n        let current = device("speaker");\n        let mut named = current.clone();\n        named.name = "3 - SAMSUNG (2- AMD High Definition Audio Device)".into();\n        let devices = [named.clone()];\n        for selection in [\n            DeviceSelection::Default,\n            DeviceSelection::Endpoint(named.endpoint.clone()),\n        ] {\n            let presentation = device_selection_presentation(\n                &selection,\n                &devices,\n                Some(&named),\n                AudioDeviceKind::Speaker,\n            );\n            assert_eq!(presentation.primary, "SAMSUNG");\n            assert_eq!(\n                presentation.secondary.as_deref(),\n                Some("AMD High Definition Audio Device")\n            );\n            assert_eq!(presentation.status, None);\n            assert!(!presentation.accessible_value().contains("default"));\n            assert!(!presentation.accessible_value().contains("Explicit"));\n        }\n        assert_eq!(\n            device_choice_label(&named, Some(&named), AudioDeviceKind::Speaker),\n            "SAMSUNG"\n        );\n    }\n''',
)

print("P0 UX pass 1 final corrections applied")
