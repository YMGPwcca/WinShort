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


# The pass-1 source driver uses the real LISTBOX creation marker and then this
# finalize step makes the theme API call explicit about its Win32 unsafe boundary.
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

# The available Special Workspace state is normal, so remove the Ready badge
# and center the useful title/description as one block.
replace_once(
    "src/ui/control_center.rs",
    '''        renderer.text(\n            "Special Workspace",\n            UiRect::new(rect.x + 16.0, rect.y + 12.0, rect.w - 180.0, 22.0).d2d(),\n            TextStyle::BodyStrong,\n            BrushRole::Text,\n        );\n        renderer.text_clipped(\n            detail,\n            UiRect::new(rect.x + 16.0, rect.y + 40.0, rect.w - 180.0, 22.0).d2d(),\n            TextStyle::Caption,\n            BrushRole::TextSecondary,\n        );\n''',
    '''        let text_width = if status.is_empty() { rect.w - 32.0 } else { rect.w - 180.0 };\n        let stack_top = rect.y + (rect.h - 40.0) * 0.5;\n        renderer.text(\n            "Special Workspace",\n            UiRect::new(rect.x + 16.0, stack_top, text_width, 20.0).d2d(),\n            TextStyle::BodyStrong,\n            BrushRole::Text,\n        );\n        renderer.text_clipped(\n            detail,\n            UiRect::new(rect.x + 16.0, stack_top + 22.0, text_width, 18.0).d2d(),\n            TextStyle::Caption,\n            BrushRole::TextSecondary,\n        );\n''',
)

print("P0 UX pass 1 final corrections applied")
