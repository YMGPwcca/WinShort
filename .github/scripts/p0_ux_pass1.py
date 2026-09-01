from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]


def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


def write(path, text):
    (ROOT / path).write_text(text, encoding="utf-8", newline="\n")


def replace(path, old, new, count=1):
    text = read(path)
    found = text.count(old)
    if found != count:
        raise SystemExit(f"{path}: expected {count} matches, found {found}: {old[:100]!r}")
    write(path, text.replace(old, new))


def replace_in(path, start, end, old, new, count=1):
    text = read(path)
    a = text.index(start)
    b = text.index(end, a)
    chunk = text[a:b]
    found = chunk.count(old)
    if found != count:
        raise SystemExit(f"{path}: scoped expected {count} matches, found {found}: {old[:100]!r}")
    text = text[:a] + chunk.replace(old, new) + text[b:]
    write(path, text)


def replace_block(path, start, end, new_block):
    text = read(path)
    a = text.index(start)
    b = text.index(end, a)
    write(path, text[:a] + new_block + text[b:])

# ---------------------------------------------------------------------------
# Controls: truthful centering, compact device value, usable scrollbar, slider.
# ---------------------------------------------------------------------------
replace_in(
    "src/ui/controls.rs",
    "pub fn draw_desktop_item(",
    "pub fn draw_display_route_card(",
    "TextStyle::BodyStrong,",
    "TextStyle::Button,",
)
replace_in(
    "src/ui/controls.rs",
    "pub fn draw_position_cell(",
    "fn draw_surface(",
    "TextStyle::Caption,",
    "TextStyle::ButtonSmall,",
)

# Remove redundant Change affordance and center the keycap/text stack vertically.
replace_in(
    "src/ui/controls.rs",
    "pub fn draw_shortcut_card(",
    "pub fn draw_choice(",
    "let keycap = Rect::new(rect.right() - 174.0, rect.y + 12.0, 156.0, 32.0);",
    "let keycap = Rect::new(rect.right() - 174.0, rect.y + (rect.h - 32.0) * 0.5, 156.0, 32.0);",
)
replace_in(
    "src/ui/controls.rs",
    "pub fn draw_shortcut_card(",
    "pub fn draw_choice(",
    "Rect::new(rect.x + BODY_LEFT, rect.y + 12.0, rect.w - 204.0, 20.0)",
    "Rect::new(rect.x + BODY_LEFT, rect.y + (rect.h - 40.0) * 0.5, rect.w - 204.0, 20.0)",
)
replace_in(
    "src/ui/controls.rs",
    "pub fn draw_shortcut_card(",
    "pub fn draw_choice(",
    "Rect::new(rect.x + BODY_LEFT, rect.y + 36.0, rect.w - 204.0, 18.0)",
    "Rect::new(rect.x + BODY_LEFT, rect.y + (rect.h - 40.0) * 0.5 + 22.0, rect.w - 204.0, 18.0)",
)
text = read("src/ui/controls.rs")
a = text.index("pub fn draw_shortcut_card(")
b = text.index("pub fn draw_choice(", a)
chunk = text[a:b]
change_block = '''    r.text(\n        "Change",\n        Rect::new(rect.right() - 174.0, rect.y + 48.0, 156.0, 16.0).d2d(),\n        TextStyle::CaptionRight,\n        if interaction.disabled {\n            BrushRole::TextDisabled\n        } else {\n            BrushRole::Accent\n        },\n    );\n'''
if chunk.count(change_block) != 1:
    raise SystemExit("controls.rs: shortcut Change block mismatch")
chunk = chunk.replace(change_block, "")
write("src/ui/controls.rs", text[:a] + chunk + text[b:])

# Device selector: no third/default-status line; center one/two useful identity lines.
start = "pub fn draw_device_row(\n"
end = "pub fn draw_shortcut_card("
new_device = '''pub fn draw_device_row(\n    r: &Renderer,\n    element: &Element,\n    presentation: &DeviceSelectionPresentation,\n    interaction: Interaction,\n) {\n    let rect = element.rect.inset(1.0);\n    let state = interaction_state(Interaction {\n        focused: false,\n        ..interaction\n    });\n    let surface = match state {\n        InteractionState::Disabled => BrushRole::CardPressed,\n        InteractionState::Pressed => BrushRole::CardPressed,\n        InteractionState::Hovered => BrushRole::CardHover,\n        InteractionState::Focused | InteractionState::Idle => BrushRole::Card,\n    };\n    draw_surface(r, rect, surface, state, ROW_RADIUS);\n    let value_rect = device_value_rect(rect);\n    let text_width = (value_rect.x - rect.x - BODY_LEFT - 14.0).max(110.0);\n    let text_role = if interaction.disabled {\n        BrushRole::TextDisabled\n    } else {\n        BrushRole::Text\n    };\n    let secondary_role = if interaction.disabled {\n        BrushRole::TextDisabled\n    } else {\n        BrushRole::TextSecondary\n    };\n    let stack_top = rect.y + (rect.h - 40.0) * 0.5;\n    r.text_clipped(\n        &element.label,\n        Rect::new(rect.x + BODY_LEFT, stack_top, text_width, 20.0).d2d(),\n        TextStyle::BodyStrong,\n        text_role,\n    );\n    r.text_clipped(\n        &element.description,\n        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, text_width, 18.0).d2d(),\n        TextStyle::Caption,\n        secondary_role,\n    );\n    let value_state = interaction_state(interaction);\n    let value_background = match value_state {\n        InteractionState::Disabled | InteractionState::Pressed => BrushRole::CardPressed,\n        InteractionState::Hovered => BrushRole::ControlHover,\n        InteractionState::Focused | InteractionState::Idle => BrushRole::BackgroundSubtle,\n    };\n    r.fill_rounded(value_rect.d2d(), CONTROL_RADIUS, value_background);\n    r.stroke_rounded(\n        value_rect.d2d(),\n        CONTROL_RADIUS,\n        if interaction.focused { BrushRole::Focus } else { BrushRole::Border },\n        if interaction.focused { 1.5 } else { 1.0 },\n    );\n    let inner = Rect::new(\n        value_rect.x + 10.0,\n        value_rect.y + 5.0,\n        (value_rect.w - 32.0).max(1.0),\n        (value_rect.h - 10.0).max(1.0),\n    );\n    if let Some(secondary) = presentation.secondary.as_deref() {\n        let top = inner.y + (inner.h - 31.0) * 0.5;\n        r.text_clipped(\n            &presentation.primary,\n            Rect::new(inner.x, top, inner.w, 18.0).d2d(),\n            TextStyle::Value,\n            text_role,\n        );\n        r.text_clipped(\n            secondary,\n            Rect::new(inner.x, top + 18.0, inner.w, 13.0).d2d(),\n            TextStyle::Caption,\n            secondary_role,\n        );\n    } else {\n        r.text_clipped(\n            &presentation.primary,\n            inner.d2d(),\n            TextStyle::Value,\n            text_role,\n        );\n    }\n    let x = value_rect.right() - 14.0;\n    let y = value_rect.y + value_rect.h * 0.5;\n    let chevron = if interaction.disabled {\n        BrushRole::TextDisabled\n    } else if interaction.focused {\n        BrushRole::Focus\n    } else {\n        BrushRole::TextSecondary\n    };\n    r.line(x - 3.0, y - 2.0, x, y + 1.0, chevron, 1.25);\n    r.line(x, y + 1.0, x + 3.0, y - 2.0, chevron, 1.25);\n}\n\n'''
replace_block("src/ui/controls.rs", start, end, new_device)

# Make the page scrollbar both visible and geometrically reusable for drag input.
start = "pub fn draw_scrollbar("
end = "fn control_rect("
new_scrollbar = '''pub(crate) fn scrollbar_thumb_rect(\n    viewport: Rect,\n    scroll: f32,\n    max_scroll: f32,\n) -> Option<Rect> {\n    if max_scroll <= 0.0 || viewport.h <= 0.0 {\n        return None;\n    }\n    let total = viewport.h + max_scroll;\n    let thumb_h = (viewport.h * viewport.h / total).clamp(40.0, viewport.h - 8.0);\n    let travel = (viewport.h - thumb_h - 8.0).max(0.0);\n    let ratio = (scroll / max_scroll).clamp(0.0, 1.0);\n    Some(Rect::new(\n        viewport.right() - 10.0,\n        viewport.y + 4.0 + travel * ratio,\n        6.0,\n        thumb_h,\n    ))\n}\n\npub(crate) fn scrollbar_hit_rect(viewport: Rect) -> Rect {\n    Rect::new(viewport.right() - 16.0, viewport.y, 16.0, viewport.h)\n}\n\npub(crate) fn scroll_from_scrollbar_pointer(\n    viewport: Rect,\n    pointer_y: f32,\n    pointer_offset: f32,\n    max_scroll: f32,\n) -> f32 {\n    let Some(thumb) = scrollbar_thumb_rect(viewport, 0.0, max_scroll) else {\n        return 0.0;\n    };\n    let travel = (viewport.h - thumb.h - 8.0).max(0.0);\n    if travel <= f32::EPSILON {\n        return 0.0;\n    }\n    let top = (pointer_y - pointer_offset)\n        .clamp(viewport.y + 4.0, viewport.y + 4.0 + travel);\n    ((top - viewport.y - 4.0) / travel * max_scroll).clamp(0.0, max_scroll)\n}\n\npub fn draw_scrollbar(r: &Renderer, viewport: Rect, scroll: f32, max_scroll: f32) {\n    let Some(thumb) = scrollbar_thumb_rect(viewport, scroll, max_scroll) else {\n        return;\n    };\n    r.fill_rounded(thumb.d2d(), 3.0, BrushRole::BorderStrong);\n}\n\n'''
replace_block("src/ui/controls.rs", start, end, new_scrollbar)

# Slider: plain value text, longer track, no fake text-box at the right.
start = "pub(crate) fn slider_track_rect("
end = "fn page_for_nav("
new_slider = '''pub(crate) fn slider_track_rect(row: Rect) -> Rect {\n    let rect = row.inset(1.0);\n    let label_end = (rect.x + rect.w * 0.34).clamp(rect.x + 150.0, rect.x + 230.0);\n    let value_width = 74.0;\n    let right = rect.right() - 18.0 - value_width;\n    Rect::new(\n        label_end,\n        rect.y + rect.h * 0.5 - 3.0,\n        (right - label_end - 14.0).max(100.0),\n        6.0,\n    )\n}\n\nfn draw_slider_cluster(\n    r: &Renderer,\n    element: &Element,\n    ratio: f32,\n    label: &str,\n    interaction: Interaction,\n) {\n    let rect = element.rect.inset(1.0);\n    let state = interaction_state(Interaction { focused: false, ..interaction });\n    draw_surface(\n        r,\n        rect,\n        if matches!(state, InteractionState::Hovered | InteractionState::Pressed) {\n            BrushRole::CardHover\n        } else {\n            BrushRole::Card\n        },\n        state,\n        ROW_RADIUS,\n    );\n    let stack_top = rect.y + (rect.h - 40.0) * 0.5;\n    r.text_clipped(\n        &element.label,\n        Rect::new(rect.x + BODY_LEFT, stack_top, 150.0, 20.0).d2d(),\n        TextStyle::BodyStrong,\n        if interaction.disabled { BrushRole::TextDisabled } else { BrushRole::Text },\n    );\n    r.text_clipped(\n        &element.description,\n        Rect::new(rect.x + BODY_LEFT, stack_top + 22.0, 150.0, 18.0).d2d(),\n        TextStyle::Caption,\n        if interaction.disabled { BrushRole::TextDisabled } else { BrushRole::TextSecondary },\n    );\n    let track = slider_track_rect(element.rect);\n    r.fill_rounded(\n        track.d2d(),\n        3.0,\n        if interaction.disabled { BrushRole::Border } else { BrushRole::BorderStrong },\n    );\n    let filled = Rect::new(track.x, track.y, track.w * ratio.clamp(0.0, 1.0), track.h);\n    if filled.w > 0.0 {\n        r.fill_rounded(\n            filled.d2d(),\n            3.0,\n            if interaction.disabled {\n                BrushRole::BorderStrong\n            } else if matches!(state, InteractionState::Pressed) {\n                BrushRole::AccentPressed\n            } else {\n                BrushRole::Accent\n            },\n        );\n    }\n    let knob_x = track.x + track.w * ratio.clamp(0.0, 1.0);\n    let radius = if matches!(state, InteractionState::Pressed) { 8.0 } else { 7.0 };\n    r.ellipse(\n        knob_x,\n        track.y + track.h * 0.5,\n        radius,\n        radius,\n        if interaction.disabled { BrushRole::TextDisabled } else { BrushRole::Accent },\n        true,\n        0.0,\n    );\n    let value_rect = Rect::new(rect.right() - 88.0, rect.y, 70.0, rect.h);\n    r.text_clipped(\n        label,\n        value_rect.d2d(),\n        TextStyle::CaptionRight,\n        if interaction.disabled { BrushRole::TextDisabled } else { BrushRole::Text },\n    );\n    if interaction.focused {\n        r.stroke_rounded(rect.inset(-2.0).d2d(), ROW_RADIUS + 2.0, BrushRole::Focus, 1.5);\n    }\n}\n\n'''
replace_block("src/ui/controls.rs", start, end, new_slider)

# ---------------------------------------------------------------------------
# Device presentation: selection itself conveys "current"; no explicit/default badges.
# ---------------------------------------------------------------------------
start = "pub fn device_selection_presentation("
end = "pub fn device_choice_label("
new_presentation = '''pub fn device_selection_presentation(\n    _selection: &DeviceSelection,\n    _devices: &[DeviceId],\n    default: Option<&DeviceId>,\n    kind: AudioDeviceKind,\n) -> DeviceSelectionPresentation {\n    if let Some(device) = default {\n        let label = friendly_device(device, kind);\n        DeviceSelectionPresentation {\n            primary: label.primary,\n            secondary: label.detail,\n            status: None,\n        }\n    } else {\n        DeviceSelectionPresentation {\n            primary: "Windows default unavailable".into(),\n            secondary: None,\n            status: None,\n        }\n    }\n}\n\n'''
replace_block("src/ui/presentation.rs", start, end, new_presentation)
replace(
    "src/ui/presentation.rs",
    '''pub fn device_choice_label(\n    device: &DeviceId,\n    default: Option<&DeviceId>,\n    kind: AudioDeviceKind,\n) -> String {\n    let label = friendly_device(device, kind).primary;\n    if default.is_some_and(|current| current.endpoint == device.endpoint) {\n        format!("{label} · Currently Windows default")\n    } else {\n        label\n    }\n}\n''',
    '''pub fn device_choice_label(\n    device: &DeviceId,\n    _default: Option<&DeviceId>,\n    kind: AudioDeviceKind,\n) -> String {\n    friendly_device(device, kind).primary\n}\n'''
)

# ---------------------------------------------------------------------------
# Layout: Windows already supplies external-audio OSD; remove the duplicate option.
# ---------------------------------------------------------------------------
replace(
    "src/ui/layout.rs",
    '''    add_row(\n        layout,\n        &mut y,\n        ElementId::OverlayExternalChanges,\n        ElementKind::Toggle,\n        "Show Windows audio changes",\n        "Keep the status card in sync with Windows audio",\n    );\n''',
    ""
)

# ---------------------------------------------------------------------------
# Control Center: compact popup height, Special status, draggable scrollbar.
# ---------------------------------------------------------------------------
replace(
    "src/ui/control_center.rs",
    "        let height = ((choices.len().min(10) as f32 * 30.0 + 8.0) * scale).round() as i32;",
    "        let height = ((choices.len().min(10) as f32 * 30.0) * scale).round() as i32 + 2;",
)

# Track scrollbar drag without stealing the normal element pressed state.
replace(
    "src/ui/control_center.rs",
    "    scroll: f32,\n    motion: Motion,",
    "    scroll: f32,\n    scroll_drag_offset: Option<f32>,\n    motion: Motion,",
)
replace(
    "src/ui/control_center.rs",
    "            scroll: 0.0,\n            motion: Motion::default(),",
    "            scroll: 0.0,\n            scroll_drag_offset: None,\n            motion: Motion::default(),",
)

# Available is the normal state, not a status badge.
replace(
    "src/ui/control_center.rs",
    '''        } else if matches!(\n            &self.runtime.desktop.native,\n            crate::desktop::BackendAvailability::Available\n        ) {\n            (\n                "Ready",\n                "A dedicated place for windows kept out of the way.",\n                BrushRole::Success,\n            )\n''',
    '''        } else if matches!(\n            &self.runtime.desktop.native,\n            crate::desktop::BackendAvailability::Available\n        ) {\n            (\n                "",\n                "A dedicated place for windows kept out of the way.",\n                BrushRole::Border,\n            )\n'''
)
replace_in(
    "src/ui/control_center.rs",
    "    fn draw_special_workspace(",
    "    fn draw_current_app_audio(",
    '''        renderer.text(\n            status,\n            UiRect::new(rect.right() - 150.0, rect.y + 28.0, 134.0, 22.0).d2d(),\n            TextStyle::BodyStrong,\n            role,\n        );\n''',
    '''        if !status.is_empty() {\n            renderer.text(\n                status,\n                UiRect::new(rect.right() - 150.0, rect.y + 28.0, 134.0, 22.0).d2d(),\n                TextStyle::BodyStrong,\n                role,\n            );\n        }\n'''
)

# Mouse move: scrollbar drag wins before hover/slider handling.
needle = '''                let mut ui = cell.borrow_mut();\n                ui.update_hover(hwnd, x, y);\n'''
replacement = '''                let mut ui = cell.borrow_mut();\n                if let Some(offset) = ui.scroll_drag_offset {\n                    ui.scroll = controls::scroll_from_scrollbar_pointer(\n                        ui.layout.content_clip,\n                        y,\n                        offset,\n                        ui.layout.max_scroll,\n                    );\n                    ui.rebuild_layout(hwnd);\n                    ui.publish_automation_snapshot(hwnd);\n                    invalidate(hwnd);\n                    return LRESULT(0);\n                }\n                ui.update_hover(hwnd, x, y);\n'''
replace_in("src/ui/control_center.rs", "            WM_MOUSEMOVE => {", "            WM_MOUSELEAVE => {", needle, replacement)

# Mouse down: 16-DIP scrollbar hit target, click track jumps thumb and starts drag.
old = '''                    let mut focus_requested = false;\n                    let mut capture_requested = false;\n                    if let Some(id) = ui.layout.hit_test(x, y) {\n                        if !ui.is_disabled(id) {\n                            focus_requested = true;\n                            capture_requested = true;\n                            ui.pressed = Some(id);\n                            ui.focused = Some(id);\n                            if matches!(\n                                id,\n                                ElementId::OverlayDuration\n                                    | ElementId::OverlayOpacity\n                                    | ElementId::OverlayScale\n                            ) {\n                                ui.set_slider_from_x(id, x);\n                            }\n                            invalidate(hwnd);\n                        }\n                    }\n'''
new = '''                    let mut focus_requested = false;\n                    let mut capture_requested = false;\n                    let scrollbar_hit = ui.layout.max_scroll > 0.0\n                        && controls::scrollbar_hit_rect(ui.layout.content_clip).contains(x, y);\n                    if scrollbar_hit {\n                        if let Some(thumb) = controls::scrollbar_thumb_rect(\n                            ui.layout.content_clip,\n                            ui.scroll,\n                            ui.layout.max_scroll,\n                        ) {\n                            let offset = if thumb.contains(x, y) {\n                                y - thumb.y\n                            } else {\n                                thumb.h * 0.5\n                            };\n                            ui.scroll_drag_offset = Some(offset);\n                            ui.scroll = controls::scroll_from_scrollbar_pointer(\n                                ui.layout.content_clip,\n                                y,\n                                offset,\n                                ui.layout.max_scroll,\n                            );\n                            ui.rebuild_layout(hwnd);\n                            capture_requested = true;\n                            invalidate(hwnd);\n                        }\n                    } else if let Some(id) = ui.layout.hit_test(x, y) {\n                        if !ui.is_disabled(id) {\n                            focus_requested = true;\n                            capture_requested = true;\n                            ui.pressed = Some(id);\n                            ui.focused = Some(id);\n                            if matches!(\n                                id,\n                                ElementId::OverlayDuration\n                                    | ElementId::OverlayOpacity\n                                    | ElementId::OverlayScale\n                            ) {\n                                ui.set_slider_from_x(id, x);\n                            }\n                            invalidate(hwnd);\n                        }\n                    }\n'''
replace_in("src/ui/control_center.rs", "            WM_LBUTTONDOWN => {", "            WM_LBUTTONUP => {", old, new)

# Mouse up: finish scrollbar drag without activating a setting underneath.
old = '''            WM_LBUTTONUP => {\n                let (slider, activate_id) = {\n                    let mut ui = cell.borrow_mut();\n'''
new = '''            WM_LBUTTONUP => {\n                if cell.borrow_mut().scroll_drag_offset.take().is_some() {\n                    let _ = ReleaseCapture();\n                    invalidate(hwnd);\n                    return LRESULT(0);\n                }\n                let (slider, activate_id) = {\n                    let mut ui = cell.borrow_mut();\n'''
replace("src/ui/control_center.rs", old, new)

# ---------------------------------------------------------------------------
# Native LISTBOX scrollbar follows app dark theme instead of bright Explorer chrome.
# ---------------------------------------------------------------------------
replace(
    "src/ui/picker.rs",
    "    DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_FOCUS, ODS_SELECTED, ODT_LISTBOX,\n};",
    "    SetWindowTheme, DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_FOCUS, ODS_SELECTED, ODT_LISTBOX,\n};",
)
# Apply immediately after child LISTBOX creation succeeds.
marker = "        if list.0.is_null() {\n"
text = read("src/ui/picker.rs")
idx = text.index(marker)
# Find end of the null check and the first following stable font setup line.
insert_at = text.index("        let font =", idx)
insert = '''        if !SystemVisualPreferences::query().high_contrast {\n            let theme_name = if Theme::current().mode == crate::ui::theme::ThemeMode::Dark {\n                HSTRING::from("DarkMode_Explorer")\n            } else {\n                HSTRING::from("Explorer")\n            };\n            let _ = SetWindowTheme(list, PCWSTR(theme_name.as_ptr()), PCWSTR::null());\n        }\n'''
if insert in text:
    raise SystemExit("picker.rs: dark theme insert already present")
text = text[:insert_at] + insert + text[insert_at:]
write("src/ui/picker.rs", text)

# ---------------------------------------------------------------------------
# Overlay: clip all DWrite text to its card and shorten device title.
# ---------------------------------------------------------------------------
replace(
    "src/ui/overlay.rs",
    "    D2D1CreateFactory, ID2D1Factory1, ID2D1RenderTarget, D2D1_FACTORY_OPTIONS,\n",
    "    D2D1CreateFactory, ID2D1Factory1, ID2D1RenderTarget, D2D1_DRAW_TEXT_OPTIONS_CLIP,\n    D2D1_FACTORY_OPTIONS,\n",
)
replace_in(
    "src/ui/overlay.rs",
    "pub fn output_row(",
    "pub fn application_row(",
    "                device.name.clone()",
    "                concise(&device.name)",
)
replace(
    "src/ui/overlay.rs",
    "            windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,",
    "            D2D1_DRAW_TEXT_OPTIONS_CLIP,",
)

# ---------------------------------------------------------------------------
# External Windows audio changes already have Windows' OSD: do not mirror them.
# Explicit WinShort actions/status requests still use the WinShort overlay.
# ---------------------------------------------------------------------------
text = read("src/app.rs")
old = "                    config.overlay.show_external_audio_changes,"
found = text.count(old)
if found != 3:
    raise SystemExit(f"app.rs: expected 3 external overlay gates, found {found}")
text = text.replace(old, "                    false,")
write("src/app.rs", text)

# Update copy/docs that would otherwise advertise the removed duplicate-O/S mirroring switch.
replace(
    "docs/UI_DESIGN.md",
    "The speaker and microphone pickers expose only active real endpoints; selecting one changes the Windows system default through the audio worker, while opaque endpoint strings and the internal follow-default binding stay out of the picker.",
    "The speaker and microphone pickers expose only active real endpoints; selecting one changes the Windows system default through the audio worker. The selected row itself identifies the current endpoint, so the Control Center does not add redundant default/explicit badges. Opaque endpoint strings and the internal follow-default binding stay out of the picker.",
)

print("P0 UX pass 1 patch applied")
