from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


def write(path, text):
    (ROOT / path).write_text(text, encoding="utf-8", newline="\n")


def replace(path, old, new, count=1):
    text = read(path)
    found = text.count(old)
    if found != count:
        raise SystemExit(f"{path}: expected {count} matches, found {found}: {old[:120]!r}")
    write(path, text.replace(old, new))


# Generic settings rows used fixed top offsets, so the description sat visibly
# low inside taller rows. Center the useful two-line block as a unit.
replace(
    "src/ui/controls.rs",
    '''    let text_width = (rect.w - 260.0).max(110.0);\n    r.text_clipped(\n        &element.label,\n        Rect::new(rect.x + BODY_LEFT, rect.y + 8.0, text_width, 22.0).d2d(),\n        TextStyle::BodyStrong,\n        label_role,\n    );\n    r.text_clipped(\n        &element.description,\n        Rect::new(rect.x + BODY_LEFT, rect.y + 31.0, text_width, 18.0).d2d(),\n        TextStyle::Caption,\n        secondary_role,\n    );\n''',
    '''    let text_width = (rect.w - 260.0).max(110.0);\n    let stack_top = rect.y + (rect.h - 40.0) * 0.5;\n    r.text_clipped(\n        &element.label,\n        Rect::new(rect.x + BODY_LEFT, stack_top, text_width, 20.0).d2d(),\n        TextStyle::BodyStrong,\n        label_role,\n    );\n    r.text_clipped(\n        &element.description,\n        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, text_width, 18.0).d2d(),\n        TextStyle::Caption,\n        secondary_role,\n    );\n''',
)

# Windows already presents endpoint mute/volume state through its own system UI.
# Keep state/cache refreshes, but never stack a second WinShort OSD on top of it.
replace(
    "src/app.rs",
    '''            AppEvent::MicrophoneStateChanged { state, origin } => {\n                let changed = self.microphone_state != state;\n                let should_show = Self::should_show_audio_overlay(\n                    origin,\n                    self.microphone_seen,\n                    changed,\n                    false,\n                    false,\n                );\n                self.microphone_state = state;\n                self.microphone_seen = true;\n                if should_show {\n                    self.show_microphone_overlay();\n                }\n                self.refresh_settings_runtime();\n            }\n            AppEvent::OutputStateChanged { state, origin } => {\n                let changed = self.output_state != state;\n                let should_show = Self::should_show_audio_overlay(\n                    origin,\n                    self.output_seen,\n                    changed,\n                    false,\n                    false,\n                );\n                self.output_state = state;\n                self.output_seen = true;\n                if should_show {\n                    self.show_output_overlay();\n                }\n                self.refresh_settings_runtime();\n            }\n''',
    '''            AppEvent::MicrophoneStateChanged { state, origin: _ } => {\n                self.microphone_state = state;\n                self.microphone_seen = true;\n                self.refresh_settings_runtime();\n            }\n            AppEvent::OutputStateChanged { state, origin: _ } => {\n                self.output_state = state;\n                self.output_seen = true;\n                self.refresh_settings_runtime();\n            }\n''',
)
replace(
    "src/app.rs",
    '''    fn show_microphone_overlay(&mut self) {\n        let row = crate::ui::overlay::microphone_row(&self.microphone_state);\n        self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));\n    }\n\n    fn show_output_overlay(&mut self) {\n        let row = crate::ui::overlay::output_row(&self.output_state);\n        self.show_overlay_model(crate::ui::overlay::OverlayModel::single(row));\n    }\n\n''',
    "",
)

# Make the product contract explicit: system endpoint feedback belongs to Windows;
# WinShort's overlay remains for app/workspace/display/status features where it adds value.
replace(
    "docs/UI_DESIGN.md",
    "The speaker and microphone pickers expose only active real endpoints; selecting one changes the Windows system default through the audio worker. The selected row itself identifies the current endpoint, so the Control Center does not add redundant default/explicit badges. Opaque endpoint strings and the internal follow-default binding stay out of the picker.",
    "The speaker and microphone pickers expose only active real endpoints; selecting one changes the Windows system default through the audio worker. The selected row itself identifies the current endpoint, so the Control Center does not add redundant default/explicit badges. System speaker/microphone mute and volume feedback is left to Windows instead of stacking a duplicate WinShort OSD. Opaque endpoint strings and the internal follow-default binding stay out of the picker.",
)

print("P0 UX pass 3 patch applied")
