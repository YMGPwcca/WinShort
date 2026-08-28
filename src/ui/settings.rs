//! Owner-drawn native Settings window.
//!
//! THESIS: a compact Fluent operator surface, not a legacy dialog or a wall of
//! equal cards. OWN-WORLD: Windows 11 dark/light system surfaces, Segoe UI
//! Variable, one cyan system accent, quiet elevation. STORY: scan live state,
//! edit a typed draft, validate, apply without restart. FIRST VIEWPORT: product
//! identity and General/Hotkeys first; fixed Save footer; remaining sections
//! scroll. FORM: native settings list, event-driven motion. FINISH: unreviewed
//! and undocumented is unfinished; this build ends with visual QA and docs.

use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, KillTimer, SetTimer, SetWindowPos, ShowWindow, CREATESTRUCTW, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_CLOSE,
    WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_GETOBJECT, WM_KEYDOWN, WM_KEYUP,
    WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_NCDESTROY, WM_PAINT, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_TIMER, WS_OVERLAPPEDWINDOW,
};

use crate::config::model::{
    Config, DeviceSelection, EndpointRole, MonitorChoice, OverlayAppearance, OverlayPosition,
};
use crate::config::validate::Violation;
use crate::error::{Error, Result};
use crate::keyboard::binding::{Hotkey, ModifierMask, VirtualKey};
use crate::platform::visual::SystemVisualPreferences;
use crate::platform::window as win;
use crate::ui::animation::{Motion, MotionChannel};
use crate::ui::controls::{self, ControlValue, Interaction};
use crate::ui::layout::{ElementId, Rect as UiRect, SettingsLayout};
use crate::ui::picker::{PickerChoice, PickerKind, PickerPopup, PickerValue, PopupRect};
use crate::ui::renderer::{rect, BrushRole, Renderer, TextStyle};
use crate::ui::settings_automation::{
    node_has_invoke, snapshot_from_settings, AutomationFocusOwner, SettingsAutomation,
    SettingsAutomationAction, WM_APP_SETTINGS_AUTOMATION, WM_APP_SETTINGS_AUTOMATION_EVENTS,
};
use crate::ui::theme::{Color, Theme, ThemeMode};

pub const CLASS_NAME: &str = "WinShort.Settings";
pub const DESIGN_WIDTH: f32 = 610.0;
pub const DESIGN_HEIGHT: f32 = 720.0;
const UI_TIMER: usize = 1;
const UI_TIMER_MS: u32 = 16;
const APPLIED_STATUS: &str = "Changes applied";

static REGISTERED: OnceLock<u16> = OnceLock::new();
const WM_MOUSELEAVE: u32 = 0x02A3;
fn settings_theme() -> Theme {
    settings_theme_for(Theme::current(), SystemVisualPreferences::query())
}

fn settings_theme_for(theme: Theme, visual: SystemVisualPreferences) -> Theme {
    if !visual.high_contrast {
        return theme;
    }
    let background = Color::rgb(
        visual.high_contrast_background.r,
        visual.high_contrast_background.g,
        visual.high_contrast_background.b,
    );
    let foreground = Color::rgb(
        visual.high_contrast_foreground.r,
        visual.high_contrast_foreground.g,
        visual.high_contrast_foreground.b,
    );
    let highlight = Color::rgb(
        visual.high_contrast_highlight.r,
        visual.high_contrast_highlight.g,
        visual.high_contrast_highlight.b,
    );
    let highlight_text = Color::rgb(
        visual.high_contrast_highlight_foreground.r,
        visual.high_contrast_highlight_foreground.g,
        visual.high_contrast_highlight_foreground.b,
    );
    Theme {
        mode: theme.mode,
        bg: background,
        bg_subtle: background,
        card: background,
        card_hover: background,
        control_hover: background,
        picker_hover: background,
        card_pressed: background,
        border: foreground,
        border_strong: foreground,
        text: foreground,
        text_secondary: foreground,
        text_disabled: foreground,
        accent: highlight,
        accent_hover: highlight,
        accent_pressed: highlight,
        accent_text: highlight_text,
        danger: foreground,
        warning: foreground,
        success: foreground,
        focus: foreground,
        shadow: Color::rgba(0, 0, 0, 0),
    }
}

pub struct SettingsUi {
    dpi: u32,
    renderer: Option<Renderer>,
    layout: SettingsLayout,
    draft: Config,
    devices: crate::audio::devices::DeviceLists,
    validation: Vec<Violation>,
    hovered: Option<ElementId>,
    pressed: Option<ElementId>,
    recording_modifiers: ModifierMask,
    focused: Option<ElementId>,
    focus_owner: AutomationFocusOwner,
    picker_owner: Option<ElementId>,
    picker_hwnd: Option<HWND>,
    picker_list_hwnd: Option<HWND>,
    recording: Option<ElementId>,
    capture_armed: bool,
    reset_confirm: bool,
    scroll: f32,
    motion: Motion,
    applied_until: Option<Instant>,
    closing: bool,
    mouse_tracking: bool,
    automation: Option<SettingsAutomation>,
}

impl SettingsUi {
    fn new(dpi: u32, devices: crate::audio::devices::DeviceLists) -> Self {
        let draft = (*crate::app::config()).clone();
        Self {
            dpi,
            renderer: None,
            layout: SettingsLayout::build(DESIGN_WIDTH, DESIGN_HEIGHT, 0.0),
            draft,
            devices,
            validation: Vec::new(),
            hovered: None,
            pressed: None,
            recording_modifiers: ModifierMask::NONE,
            focused: None,
            focus_owner: AutomationFocusOwner::Outside,
            picker_list_hwnd: None,
            picker_owner: None,
            picker_hwnd: None,
            recording: None,
            capture_armed: false,
            reset_confirm: false,
            scroll: 0.0,
            motion: Motion::default(),
            applied_until: None,
            closing: false,
            mouse_tracking: false,
            automation: None,
        }
    }

    fn rebuild_layout(&mut self, hwnd: HWND) {
        let (width, height) = self
            .renderer
            .as_ref()
            .map(Renderer::client_size_dip)
            .unwrap_or_else(|| client_size_dip(hwnd, self.dpi));
        self.layout = SettingsLayout::build(width, height, self.scroll);
        self.scroll = self.layout.scroll;
    }

    fn dirty(&self) -> bool {
        self.draft != *crate::app::config()
    }
    fn replace_draft(&mut self, draft: Config) {
        self.draft = draft;
        self.motion.clear_channel(MotionChannel::ToggleState);
    }

    fn repair_focus(&mut self) {
        if self.focus_owner != AutomationFocusOwner::Settings {
            return;
        }
        let Some(current) = self.focused else {
            return;
        };
        if self.layout.element(current).is_some() && !self.is_disabled(current) {
            return;
        }
        let next = Self::next_focus_index(&ElementId::FOCUS_ORDER, Some(current), false, |id| {
            self.is_disabled(id)
        });
        self.focused = (!self.is_disabled(next)).then_some(next);
    }

    fn begin_close(&mut self) -> bool {
        if self.closing {
            false
        } else {
            self.closing = true;
            true
        }
    }

    fn picker_activation_allowed(&self) -> bool {
        !self.closing
    }

    fn paint(&mut self, hwnd: HWND) -> Result<()> {
        self.rebuild_layout(hwnd);
        let dirty = self.dirty();
        let now = Instant::now();
        if self.applied_until.is_some_and(|until| now >= until) {
            self.applied_until = None;
        }

        let renderer = match self.renderer.take() {
            Some(renderer) => renderer,
            None => Renderer::new(hwnd, self.dpi, settings_theme())?,
        };
        renderer.begin();

        // Scrollable body.
        renderer.push_clip(self.layout.content_clip.d2d());
        controls::draw_app_mark(&renderer, UiRect::new(24.0, 22.0 - self.scroll, 34.0, 34.0));
        renderer.text(
            "WinShort",
            rect(
                70.0,
                16.0 - self.scroll,
                self.layout.width - 24.0,
                44.0 - self.scroll,
            ),
            TextStyle::Title,
            BrushRole::Text,
        );
        renderer.text(
            "Audio controls and desktop shortcuts, ready from the tray",
            rect(
                70.0,
                42.0 - self.scroll,
                self.layout.width - 24.0,
                64.0 - self.scroll,
            ),
            TextStyle::Subtitle,
            BrushRole::TextSecondary,
        );

        for section in &self.layout.sections {
            if section.y > self.layout.content_clip.y - 28.0
                && section.y < self.layout.content_clip.bottom()
            {
                renderer.text(
                    section.title,
                    rect(25.0, section.y, self.layout.width - 25.0, section.y + 24.0),
                    TextStyle::Section,
                    BrushRole::Text,
                );
            }
        }

        for element in &self.layout.elements {
            if !element.scrolls || !element.rect.intersects(self.layout.content_clip) {
                continue;
            }
            let disabled = self.is_disabled(element.id);
            let interaction = self.interaction(element.id, disabled);
            controls::draw_row(&renderer, element, self.value_for(element.id), interaction);
        }
        controls::draw_scrollbar(
            &renderer,
            self.layout.content_clip,
            self.scroll,
            self.layout.max_scroll,
        );
        renderer.pop_clip();

        // Fixed footer: a distinct surface, no floating-card chrome.
        renderer.fill_rect(self.layout.footer.d2d(), BrushRole::BackgroundSubtle);
        renderer.line(
            0.0,
            self.layout.footer.y,
            self.layout.width,
            self.layout.footer.y,
            BrushRole::Border,
            1.0,
        );

        let status_rect = UiRect::new(
            24.0,
            self.layout.footer.y + 17.0,
            (self.layout.width - 240.0).max(120.0),
            34.0,
        );
        if let Some(first) = self.validation.first() {
            renderer.text(
                &format!("{} — {}", first.field, first.message),
                status_rect.d2d(),
                TextStyle::Caption,
                BrushRole::Danger,
            );
        } else if self.applied_until.is_some() {
            renderer.ellipse(
                status_rect.x + 5.0,
                status_rect.y + 17.0,
                4.0,
                4.0,
                BrushRole::Success,
                true,
                0.0,
            );
            renderer.text(
                APPLIED_STATUS,
                UiRect::new(
                    status_rect.x + 16.0,
                    status_rect.y,
                    status_rect.w - 16.0,
                    status_rect.h,
                )
                .d2d(),
                TextStyle::Caption,
                BrushRole::Success,
            );
        } else if dirty {
            renderer.ellipse(
                status_rect.x + 5.0,
                status_rect.y + 17.0,
                3.5,
                3.5,
                BrushRole::Accent,
                true,
                0.0,
            );
            renderer.text(
                "Unsaved changes",
                UiRect::new(
                    status_rect.x + 16.0,
                    status_rect.y,
                    status_rect.w - 16.0,
                    status_rect.h,
                )
                .d2d(),
                TextStyle::Caption,
                BrushRole::TextSecondary,
            );
        } else {
            renderer.text(
                "Everything is up to date",
                status_rect.d2d(),
                TextStyle::Caption,
                BrushRole::TextSecondary,
            );
        }

        for id in [ElementId::Cancel, ElementId::Save] {
            let element = self.layout.element(id).expect("footer element");
            let disabled = id == ElementId::Save && !dirty;
            controls::draw_button(
                &renderer,
                element.rect,
                element.label,
                id == ElementId::Save,
                self.interaction(id, disabled),
            );
        }

        let result = renderer.end();
        if result.is_ok() {
            self.renderer = Some(renderer);
            self.publish_automation_snapshot(hwnd);
        }
        result
    }

    fn value_for(&self, id: ElementId) -> ControlValue<'_> {
        match id {
            ElementId::StartWithWindows => {
                ControlValue::Toggle(crate::platform::startup::is_enabled())
            }
            ElementId::StartHotkeysEnabled => {
                ControlValue::Toggle(self.draft.general.start_hotkeys_enabled)
            }
            ElementId::MicHotkey => self.hotkey_value(id, self.draft.hotkeys.toggle_microphone),
            ElementId::OutputHotkey => self.hotkey_value(id, self.draft.hotkeys.toggle_output),
            ElementId::ForegroundHotkey => {
                self.hotkey_value(id, self.draft.hotkeys.toggle_foreground_audio)
            }
            ElementId::CycleInputHotkey => {
                self.hotkey_value(id, self.draft.hotkeys.cycle_input_device)
            }
            ElementId::CycleOutputHotkey => {
                self.hotkey_value(id, self.draft.hotkeys.cycle_output_device)
            }
            ElementId::ForegroundVolumeUpHotkey => {
                self.hotkey_value(id, self.draft.hotkeys.foreground_volume_up)
            }
            ElementId::ForegroundVolumeDownHotkey => {
                self.hotkey_value(id, self.draft.hotkeys.foreground_volume_down)
            }
            ElementId::InputDevice => ControlValue::Text(Cow::Owned(device_label(
                &self.draft.audio.input_device,
                &self.devices.inputs,
            ))),
            ElementId::OutputDevice => ControlValue::Text(Cow::Owned(device_label(
                &self.draft.audio.output_device,
                &self.devices.outputs,
            ))),
            ElementId::InputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.input_role.label()))
            }
            ElementId::OutputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.output_role.label()))
            }
            ElementId::DesktopsEnabled => ControlValue::Toggle(self.draft.virtual_desktops.enabled),
            ElementId::WinNumberEnabled => {
                ControlValue::Toggle(self.draft.virtual_desktops.win_number_switching)
            }
            ElementId::OverlayEnabled => ControlValue::Toggle(self.draft.overlay.enabled),
            ElementId::OverlayAppearance => {
                ControlValue::Text(Cow::Borrowed(self.draft.overlay.appearance.label()))
            }
            ElementId::OverlayExternalChanges => {
                ControlValue::Toggle(self.draft.overlay.show_external_audio_changes)
            }
            ElementId::OverlayPosition => {
                ControlValue::Text(Cow::Borrowed(self.draft.overlay.position.label()))
            }
            ElementId::OverlayMonitor => {
                ControlValue::Text(Cow::Owned(self.draft.overlay.monitor.label()))
            }
            ElementId::OverlayDuration => ControlValue::Slider {
                ratio: (self.draft.overlay.duration_ms.saturating_sub(500) as f32 / 9500.0)
                    .clamp(0.0, 1.0),
                label: Cow::Owned(format!(
                    "{:.1}s",
                    self.draft.overlay.duration_ms as f32 / 1000.0
                )),
            },
            ElementId::OverlayOpacity => ControlValue::Slider {
                ratio: ((self.draft.overlay.opacity - 0.3) / 0.7).clamp(0.0, 1.0),
                label: Cow::Owned(format!(
                    "{}%",
                    (self.draft.overlay.opacity * 100.0).round() as u32
                )),
            },
            ElementId::OverlayScale => ControlValue::Slider {
                ratio: ((self.draft.overlay.scale - 0.7) / 0.9).clamp(0.0, 1.0),
                label: Cow::Owned(format!("{:.1}×", self.draft.overlay.scale)),
            },
            ElementId::OverlayPreview => ControlValue::Action(Cow::Borrowed("Preview")),
            ElementId::DebugLogging => {
                ControlValue::Toggle(crate::diagnostics::logging::debug_logging_enabled())
            }
            ElementId::DiagnosticsStatus => ControlValue::Action(Cow::Borrowed("Open")),
            ElementId::OpenConfigFolder => ControlValue::Action(Cow::Borrowed("Open folder")),
            ElementId::ResetSettings => {
                ControlValue::Action(Cow::Borrowed(if self.reset_confirm {
                    "Confirm reset"
                } else {
                    "Reset draft"
                }))
            }
            ElementId::Cancel | ElementId::Save => ControlValue::Action(Cow::Borrowed("")),
        }
    }

    fn hotkey_value(&self, id: ElementId, hotkey: Option<Hotkey>) -> ControlValue<'_> {
        if self.recording == Some(id) {
            if self.recording_modifiers.is_empty() {
                ControlValue::Text(Cow::Borrowed("Press a shortcut…"))
            } else {
                ControlValue::Text(Cow::Owned(format!("{}…", self.recording_modifiers)))
            }
        } else {
            ControlValue::Text(Cow::Owned(
                hotkey.map_or_else(|| "Not assigned".into(), |h| h.to_string()),
            ))
        }
    }

    fn interaction(&self, id: ElementId, disabled: bool) -> Interaction {
        let toggle_value = match id {
            ElementId::StartWithWindows => crate::platform::startup::is_enabled(),
            ElementId::StartHotkeysEnabled => self.draft.general.start_hotkeys_enabled,
            ElementId::DesktopsEnabled => self.draft.virtual_desktops.enabled,
            ElementId::WinNumberEnabled => self.draft.virtual_desktops.win_number_switching,
            ElementId::OverlayEnabled => self.draft.overlay.enabled,
            ElementId::OverlayExternalChanges => self.draft.overlay.show_external_audio_changes,
            ElementId::DebugLogging => crate::diagnostics::logging::debug_logging_enabled(),
            _ => false,
        };
        Interaction {
            hovered: self.hovered == Some(id),
            pressed: self.pressed == Some(id),
            focused: self.focused == Some(id),
            disabled,
            hover_t: self.motion.value(
                id,
                MotionChannel::Hover,
                if self.hovered == Some(id) { 1.0 } else { 0.0 },
            ),
            state_t: self.motion.value(
                id,
                MotionChannel::ToggleState,
                if toggle_value { 1.0 } else { 0.0 },
            ),
        }
    }

    fn endpoint_role_enabled(selection: &DeviceSelection) -> bool {
        matches!(selection, DeviceSelection::Default)
    }

    fn is_disabled(&self, id: ElementId) -> bool {
        match id {
            ElementId::WinNumberEnabled => !self.draft.virtual_desktops.enabled,
            ElementId::InputRole => !Self::endpoint_role_enabled(&self.draft.audio.input_device),
            ElementId::OutputRole => !Self::endpoint_role_enabled(&self.draft.audio.output_device),
            ElementId::OverlayAppearance
            | ElementId::OverlayExternalChanges
            | ElementId::OverlayPosition
            | ElementId::OverlayMonitor
            | ElementId::OverlayDuration
            | ElementId::OverlayOpacity
            | ElementId::OverlayScale
            | ElementId::OverlayPreview => !self.draft.overlay.enabled,
            ElementId::Save => !self.dirty(),
            _ => false,
        }
    }

    fn set_hover(&mut self, hwnd: HWND, next: Option<ElementId>) {
        if next == self.hovered {
            return;
        }
        if let Some(old) = self.hovered {
            self.motion.animate_to(old, MotionChannel::Hover, 0.0, 140);
        }
        if let Some(new) = next {
            self.motion.animate_to(new, MotionChannel::Hover, 1.0, 140);
        }
        self.hovered = next;
        start_timer(hwnd);
        invalidate(hwnd);
    }

    fn update_hover(&mut self, hwnd: HWND, x: f32, y: f32) {
        self.set_hover(hwnd, self.layout.hit_test(x, y));
    }

    fn set_slider_from_ratio(&mut self, id: ElementId, ratio: f32) {
        let ratio = ratio.clamp(0.0, 1.0);
        match id {
            ElementId::OverlayDuration => {
                self.draft.overlay.duration_ms =
                    ((500.0 + ratio * 9500.0) / 100.0).round() as u32 * 100;
            }
            ElementId::OverlayOpacity => {
                self.draft.overlay.opacity = ((0.3 + ratio * 0.7) * 20.0).round() / 20.0;
            }
            ElementId::OverlayScale => {
                self.draft.overlay.scale = ((0.7 + ratio * 0.9) * 10.0).round() / 10.0;
            }
            _ => return,
        }
        self.validation.clear();
    }
    fn set_slider_from_value(&mut self, id: ElementId, value: f64) -> bool {
        let ratio = match id {
            ElementId::OverlayDuration => (value - 500.0) / 9500.0,
            ElementId::OverlayOpacity => (value - 0.3) / 0.7,
            ElementId::OverlayScale => (value - 0.7) / 0.9,
            _ => return false,
        };
        self.set_slider_from_ratio(id, ratio as f32);
        true
    }

    fn set_slider_from_x(&mut self, id: ElementId, x: f32) {
        let Some(element) = self.layout.element(id) else {
            return;
        };
        let row = element.rect.inset(1.0);
        let control_x = row.right() - 18.0 - 190.0;
        let track_w = 190.0 - 52.0 - 12.0;
        let ratio = (x - control_x) / track_w;
        self.set_slider_from_ratio(id, ratio);
    }

    fn slider_value(id: ElementId, current: f32, step: f32) -> f32 {
        if step.is_infinite() {
            return match (id, step.is_sign_negative()) {
                (ElementId::OverlayDuration, true) => 500.0,
                (ElementId::OverlayDuration, false) => 10_000.0,
                (ElementId::OverlayOpacity, true) => 0.3,
                (ElementId::OverlayOpacity, false) => 1.0,
                (ElementId::OverlayScale, true) => 0.7,
                (ElementId::OverlayScale, false) => 1.6,
                _ => current,
            };
        }
        match id {
            ElementId::OverlayDuration => (current + step * 100.0).round().clamp(500.0, 10_000.0),
            ElementId::OverlayOpacity => (current + step * 0.05).clamp(0.3, 1.0),
            ElementId::OverlayScale => (current + step * 0.1).clamp(0.7, 1.6),
            _ => current,
        }
    }

    fn adjust_focused_slider(&mut self, hwnd: HWND, vk: u16) -> bool {
        let Some(
            id @ (ElementId::OverlayDuration | ElementId::OverlayOpacity | ElementId::OverlayScale),
        ) = self.focused
        else {
            return false;
        };
        let step = match vk {
            0x25 | 0x28 => -1.0,
            0x27 | 0x26 => 1.0,
            0x21 => 5.0,
            0x22 => -5.0,
            0x24 => f32::NEG_INFINITY,
            0x23 => f32::INFINITY,
            _ => return false,
        };
        match id {
            ElementId::OverlayDuration => {
                self.draft.overlay.duration_ms =
                    Self::slider_value(id, self.draft.overlay.duration_ms as f32, step) as u32;
            }
            ElementId::OverlayOpacity => {
                self.draft.overlay.opacity =
                    Self::slider_value(id, self.draft.overlay.opacity, step);
            }
            ElementId::OverlayScale => {
                self.draft.overlay.scale = Self::slider_value(id, self.draft.overlay.scale, step);
            }
            _ => return false,
        }
        self.validation.clear();
        invalidate(hwnd);
        true
    }

    fn consume_reset_confirmation(confirm: &mut bool) -> bool {
        if *confirm {
            *confirm = false;
            true
        } else {
            *confirm = true;
            false
        }
    }

    fn activate(&mut self, hwnd: HWND, id: ElementId) {
        if self.closing || self.is_disabled(id) {
            return;
        }
        if self
            .layout
            .element(id)
            .is_some_and(|element| node_has_invoke(element.kind))
        {
            if let Some(automation) = &self.automation {
                automation.queue_invoked(id);
            }
        }
        if id != ElementId::ResetSettings {
            self.reset_confirm = false;
        }
        match id {
            ElementId::StartWithWindows => {
                // Registry-backed toggle (#16): applies immediately, not on Save.
                let enable = !crate::platform::startup::is_enabled();
                if let Err(e) = crate::platform::startup::set_enabled(enable) {
                    self.validation.push(Violation {
                        field: "general.start_with_windows".into(),
                        message: e.to_string(),
                    });
                    invalidate(hwnd);
                    return;
                }
                self.animate_toggle(hwnd, id, enable);
            }
            ElementId::StartHotkeysEnabled => {
                self.draft.general.start_hotkeys_enabled =
                    !self.draft.general.start_hotkeys_enabled;
                self.animate_toggle(hwnd, id, self.draft.general.start_hotkeys_enabled);
            }
            ElementId::MicHotkey
            | ElementId::OutputHotkey
            | ElementId::ForegroundHotkey
            | ElementId::CycleInputHotkey
            | ElementId::CycleOutputHotkey
            | ElementId::ForegroundVolumeUpHotkey
            | ElementId::ForegroundVolumeDownHotkey => {
                self.recording = Some(id);
                self.recording_modifiers = ModifierMask::NONE;
                self.validation.clear();
                // Capture mode (#14): the global hook forwards the chord here
                // instead of dispatching it, so re-recording an active hotkey
                // no longer triggers its action. Without a hook (keyboard
                // subsystem down) the local WM_KEY* fallback still records.
                crate::keyboard::hook::begin_capture();
                self.capture_armed = true;
                start_timer(hwnd);
            }
            ElementId::InputDevice => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::InputDevice,
                ));
            }
            ElementId::OutputDevice => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::OutputDevice,
                ));
            }
            ElementId::InputRole => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::InputRole,
                ));
            }
            ElementId::OutputRole => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::OutputRole,
                ));
            }
            ElementId::DesktopsEnabled => {
                self.draft.virtual_desktops.enabled = !self.draft.virtual_desktops.enabled;
                self.animate_toggle(hwnd, id, self.draft.virtual_desktops.enabled);
            }
            ElementId::WinNumberEnabled => {
                self.draft.virtual_desktops.win_number_switching =
                    !self.draft.virtual_desktops.win_number_switching;
                self.animate_toggle(hwnd, id, self.draft.virtual_desktops.win_number_switching);
            }
            ElementId::OverlayEnabled => {
                self.draft.overlay.enabled = !self.draft.overlay.enabled;
                self.animate_toggle(hwnd, id, self.draft.overlay.enabled);
            }
            ElementId::OverlayAppearance => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::OverlayAppearance,
                ));
            }
            ElementId::OverlayExternalChanges => {
                self.draft.overlay.show_external_audio_changes =
                    !self.draft.overlay.show_external_audio_changes;
                self.animate_toggle(hwnd, id, self.draft.overlay.show_external_audio_changes);
            }
            ElementId::OverlayPosition => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::OverlayPosition,
                ));
            }
            ElementId::OverlayMonitor => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::OverlayMonitor,
                ));
            }
            ElementId::DebugLogging => {
                let enabled = !crate::diagnostics::logging::debug_logging_enabled();
                crate::diagnostics::logging::set_debug_logging(enabled);
                self.animate_toggle(hwnd, id, enabled);
            }
            ElementId::OverlayPreview => post_main(crate::event::AppEvent::PreviewOverlay {
                config: self.draft.overlay.clone(),
            }),
            ElementId::OpenConfigFolder => open_config_folder(),
            ElementId::ResetSettings => {
                if Self::consume_reset_confirmation(&mut self.reset_confirm) {
                    self.replace_draft(Config::default());
                    self.validation.clear();
                    crate::keyboard::hook::end_capture();
                    self.capture_armed = false;
                    self.recording = None;
                }
            }
            ElementId::Cancel => {
                self.replace_draft((*crate::app::config()).clone());
                self.validation.clear();
                crate::keyboard::hook::end_capture();
                self.capture_armed = false;
                self.recording = None;
            }
            ElementId::Save => self.save(hwnd),
            ElementId::OverlayDuration | ElementId::OverlayOpacity | ElementId::OverlayScale => {}
            ElementId::DiagnosticsStatus => post_main(crate::event::AppEvent::ShowDiagnostics),
        }
        self.validation.clear();
        invalidate(hwnd);
        self.publish_automation_snapshot(hwnd);
    }

    fn animate_toggle(&mut self, hwnd: HWND, id: ElementId, value: bool) {
        self.motion.animate_to(
            id,
            MotionChannel::ToggleState,
            if value { 1.0 } else { 0.0 },
            160,
        );
        start_timer(hwnd);
    }

    fn save(&mut self, hwnd: HWND) {
        self.validation = crate::config::validate(&self.draft);
        if !self.validation.is_empty() {
            invalidate(hwnd);
            return;
        }

        // Startup is registry-driven and applied at toggle time (#16); Save
        // persists everything else.
        if let Err(e) = crate::app::commit_config(
            self.draft.clone(),
            crate::event::ConfigCommitOrigin::Settings,
        ) {
            self.validation.push(Violation {
                field: "config.toml".into(),
                message: e.to_string(),
            });
            invalidate(hwnd);
            return;
        }

        self.applied_until = Some(Instant::now() + Duration::from_secs(2));
        start_timer(hwnd);
        invalidate(hwnd);
    }

    fn record_key(&mut self, hwnd: HWND, vk: u16, down: bool) -> bool {
        let Some(id) = self.recording else {
            return false;
        };
        if vk == 0x1B && down {
            self.recording = None;
            self.recording_modifiers = ModifierMask::NONE;
            invalidate(hwnd);
            return true;
        }
        if let Some(modifier) = modifier_for_vk(vk) {
            self.recording_modifiers = if down {
                self.recording_modifiers.union(modifier)
            } else {
                self.recording_modifiers.without(modifier)
            };
            invalidate(hwnd);
            return true;
        }
        if !down {
            return true;
        }
        if self.recording_modifiers.is_empty() {
            self.validation = vec![Violation {
                field: "hotkeys".into(),
                message: "Use Ctrl, Alt, Shift, or Win with the key".into(),
            }];
            invalidate(hwnd);
            return true;
        }

        let hotkey = Hotkey {
            modifiers: self.recording_modifiers,
            key: VirtualKey(vk),
        };
        match id {
            ElementId::MicHotkey => self.draft.hotkeys.toggle_microphone = Some(hotkey),
            ElementId::OutputHotkey => self.draft.hotkeys.toggle_output = Some(hotkey),
            ElementId::ForegroundHotkey => {
                self.draft.hotkeys.toggle_foreground_audio = Some(hotkey)
            }
            ElementId::CycleInputHotkey => self.draft.hotkeys.cycle_input_device = Some(hotkey),
            ElementId::CycleOutputHotkey => self.draft.hotkeys.cycle_output_device = Some(hotkey),
            ElementId::ForegroundVolumeUpHotkey => {
                self.draft.hotkeys.foreground_volume_up = Some(hotkey)
            }
            ElementId::ForegroundVolumeDownHotkey => {
                self.draft.hotkeys.foreground_volume_down = Some(hotkey)
            }
            _ => {}
        }
        self.recording = None;
        self.recording_modifiers = ModifierMask::NONE;
        self.validation = crate::config::validate(&self.draft);
        invalidate(hwnd);
        true
    }

    /// Apply a chord delivered by hook capture mode (#14).
    fn finish_recording(&mut self, chord: crate::keyboard::hook::CapturedChord) {
        self.recording_modifiers = ModifierMask::NONE;
        match chord.key {
            None => {
                // Esc: cancel recording.
                self.recording = None;
                self.validation.clear();
            }
            Some(key) => {
                if let Some(id) = self.recording {
                    let hotkey = Hotkey {
                        modifiers: chord.modifiers,
                        key,
                    };
                    match id {
                        ElementId::MicHotkey => self.draft.hotkeys.toggle_microphone = Some(hotkey),
                        ElementId::OutputHotkey => self.draft.hotkeys.toggle_output = Some(hotkey),
                        ElementId::ForegroundHotkey => {
                            self.draft.hotkeys.toggle_foreground_audio = Some(hotkey)
                        }
                        ElementId::CycleInputHotkey => {
                            self.draft.hotkeys.cycle_input_device = Some(hotkey)
                        }
                        ElementId::CycleOutputHotkey => {
                            self.draft.hotkeys.cycle_output_device = Some(hotkey)
                        }
                        ElementId::ForegroundVolumeUpHotkey => {
                            self.draft.hotkeys.foreground_volume_up = Some(hotkey)
                        }
                        ElementId::ForegroundVolumeDownHotkey => {
                            self.draft.hotkeys.foreground_volume_down = Some(hotkey)
                        }
                        _ => {}
                    }
                }
                self.recording = None;
                self.validation = crate::config::validate(&self.draft);
            }
        }
    }

    fn next_focus_index(
        order: &[ElementId],
        current: Option<ElementId>,
        reverse: bool,
        disabled: impl Fn(ElementId) -> bool,
    ) -> ElementId {
        let current_index = current
            .and_then(|id| order.iter().position(|candidate| *candidate == id))
            .unwrap_or(if reverse { 0 } else { order.len() - 1 });
        let mut index = current_index;
        for _ in 0..order.len() {
            index = if reverse {
                if index == 0 {
                    order.len() - 1
                } else {
                    index - 1
                }
            } else {
                (index + 1) % order.len()
            };
            if !disabled(order[index]) {
                return order[index];
            }
        }
        order[current_index]
    }

    fn focus_next(&mut self, hwnd: HWND, reverse: bool) {
        let order = ElementId::FOCUS_ORDER;
        let next = Self::next_focus_index(&order, self.focused, reverse, |id| self.is_disabled(id));
        self.scroll_focus_into_view(next);
        self.rebuild_layout(hwnd);
        self.focused = Some(next);
        self.publish_automation_snapshot(hwnd);
    }

    fn scroll_focus_into_view(&mut self, id: ElementId) {
        let Some(element) = self.layout.element(id) else {
            return;
        };
        if !element.scrolls {
            return;
        }
        let top = self.layout.content_clip.y + 28.0;
        let bottom = self.layout.content_clip.bottom() - 12.0;
        if element.rect.y < top {
            self.scroll = (self.scroll - (top - element.rect.y)).clamp(0.0, self.layout.max_scroll);
        } else if element.rect.bottom() > bottom {
            self.scroll =
                (self.scroll + (element.rect.bottom() - bottom)).clamp(0.0, self.layout.max_scroll);
        }
    }
    fn apply_picker(&mut self, kind: PickerKind, value: PickerValue) {
        match (kind, value) {
            (PickerKind::InputDevice, PickerValue::Device(value)) => {
                self.draft.audio.input_device = value;
            }
            (PickerKind::OutputDevice, PickerValue::Device(value)) => {
                self.draft.audio.output_device = value;
            }
            (PickerKind::InputRole, PickerValue::Role(value)) => {
                self.draft.audio.input_role = value;
            }
            (PickerKind::OutputRole, PickerValue::Role(value)) => {
                self.draft.audio.output_role = value;
            }
            (PickerKind::OverlayAppearance, PickerValue::Appearance(value)) => {
                self.draft.overlay.appearance = value;
            }
            (PickerKind::OverlayPosition, PickerValue::Position(value)) => {
                self.draft.overlay.position = value;
            }
            (PickerKind::OverlayMonitor, PickerValue::Monitor(value)) => {
                self.draft.overlay.monitor = value;
            }
            _ => {}
        }
        self.reset_confirm = false;
        self.validation.clear();
    }

    fn merge_external_device_cycle(
        &mut self,
        flow: crate::audio::DeviceCycleFlow,
        old_live: &DeviceSelection,
        new_live: &DeviceSelection,
    ) {
        let draft = match flow {
            crate::audio::DeviceCycleFlow::Input => &mut self.draft.audio.input_device,
            crate::audio::DeviceCycleFlow::Output => &mut self.draft.audio.output_device,
        };
        if draft == old_live {
            *draft = new_live.clone();
        }
        self.reset_confirm = false;
        self.validation.clear();
    }
    fn install_automation(&mut self, hwnd: HWND) {
        if self.automation.is_none() {
            self.automation = Some(SettingsAutomation::new(hwnd));
            self.publish_automation_snapshot(hwnd);
        }
    }

    fn publish_automation_snapshot(&mut self, hwnd: HWND) {
        self.repair_focus();
        let values: Vec<(ElementId, String, bool, f32)> = ElementId::FOCUS_ORDER
            .into_iter()
            .map(|id| {
                let value = match self.value_for(id) {
                    ControlValue::Toggle(value) => {
                        if value {
                            "On".into()
                        } else {
                            "Off".into()
                        }
                    }
                    ControlValue::Text(value) => value.into_owned(),
                    ControlValue::Slider { label, .. } => label.into_owned(),
                    ControlValue::Action(value) => value.into_owned(),
                };
                let ratio = match self.value_for(id) {
                    ControlValue::Slider { ratio, .. } => ratio,
                    _ => 0.0,
                };
                (id, value, !self.is_disabled(id), ratio)
            })
            .collect();
        if let Some(automation) = &self.automation {
            let mut snapshot =
                snapshot_from_settings(hwnd, &self.layout, &values, self.focused, self.dpi);
            snapshot.set_focus_state(self.focus_owner, self.picker_owner);
            automation.publish(snapshot);
        }
    }
    fn sync_focus_after_set_focus(&mut self, hwnd: HWND, actual: HWND) {
        self.focus_owner = if actual == hwnd {
            AutomationFocusOwner::Settings
        } else if self.picker_hwnd == Some(actual) || self.picker_list_hwnd == Some(actual) {
            AutomationFocusOwner::Picker
        } else {
            AutomationFocusOwner::Outside
        };
        self.publish_automation_snapshot(hwnd);
    }

    fn on_window_focus(&mut self, hwnd: HWND, focused: bool, next: HWND) {
        self.focus_owner = if focused {
            AutomationFocusOwner::Settings
        } else if self.picker_hwnd == Some(next) || self.picker_list_hwnd == Some(next) {
            AutomationFocusOwner::Picker
        } else {
            AutomationFocusOwner::Outside
        };
        self.publish_automation_snapshot(hwnd);
    }

    fn set_picker_open(
        &mut self,
        hwnd: HWND,
        owner: ElementId,
        picker_hwnd: HWND,
        picker_list_hwnd: HWND,
        actual: HWND,
    ) {
        self.picker_owner = Some(owner);
        self.picker_hwnd = Some(picker_hwnd);
        self.picker_list_hwnd = Some(picker_list_hwnd);
        self.focused = Some(owner);
        self.focus_owner = if actual == picker_hwnd || actual == picker_list_hwnd {
            AutomationFocusOwner::Picker
        } else if actual == hwnd {
            AutomationFocusOwner::Settings
        } else {
            AutomationFocusOwner::Outside
        };
        self.publish_automation_snapshot(hwnd);
    }

    fn sync_picker_focus(&mut self, hwnd: HWND, actual: HWND) {
        self.focus_owner =
            if self.picker_hwnd == Some(actual) || self.picker_list_hwnd == Some(actual) {
                AutomationFocusOwner::Picker
            } else if actual == hwnd {
                AutomationFocusOwner::Settings
            } else {
                AutomationFocusOwner::Outside
            };
        self.publish_automation_snapshot(hwnd);
    }

    fn set_picker_closed(&mut self, hwnd: HWND, owner: Option<ElementId>, actual: HWND) {
        self.picker_owner = None;
        self.picker_hwnd = None;
        self.picker_list_hwnd = None;
        if let Some(owner) = owner {
            self.focused = Some(owner);
        }
        self.focus_owner = if actual == hwnd {
            AutomationFocusOwner::Settings
        } else {
            AutomationFocusOwner::Outside
        };
        self.publish_automation_snapshot(hwnd);
    }

    fn drain_automation_actions(&mut self, hwnd: HWND) -> bool {
        let actions = self
            .automation
            .as_ref()
            .map(SettingsAutomation::drain_actions)
            .unwrap_or_default();
        if self.closing {
            return false;
        }
        let mut focus_requested = false;
        for action in actions {
            match action {
                SettingsAutomationAction::Invoke(id) | SettingsAutomationAction::Toggle(id) => {
                    self.focused = Some(id);
                    self.activate(hwnd, id);
                }
                SettingsAutomationAction::SetSlider { id, value } => {
                    if !self.is_disabled(id) && self.set_slider_from_value(id, value) {
                        self.focused = Some(id);
                        invalidate(hwnd);
                    }
                }
                SettingsAutomationAction::SetWindowFocus => {
                    focus_requested = true;
                }
                SettingsAutomationAction::SetFocus(id) => {
                    if self.layout.element(id).is_some() && !self.is_disabled(id) {
                        self.focused = Some(id);
                        self.scroll_focus_into_view(id);
                        self.rebuild_layout(hwnd);
                        invalidate(hwnd);
                        focus_requested = true;
                    }
                }
            }
        }
        self.publish_automation_snapshot(hwnd);
        focus_requested
    }
}
#[derive(Debug, Clone, Copy)]
struct SavedSettingsRect {
    rect: RECT,
    dpi: u32,
}

pub struct SettingsWindow {
    pub hwnd: HWND,
    picker: Option<PickerPopup>,
    picker_owner: Option<ElementId>,
    last_rect: Option<SavedSettingsRect>,
}

impl SettingsWindow {
    pub fn create(devices: crate::audio::devices::DeviceLists) -> Result<Self> {
        let _atom = *REGISTERED.get_or_init(|| {
            win::register_class(CLASS_NAME, Some(settings_wndproc))
                .expect("register settings class")
        });

        let primary = crate::platform::monitor::primary();
        let primary_dpi = primary.as_ref().map_or(96, |m| m.dpi);
        let scale = primary_dpi as f32 / 96.0;
        let default_width = (DESIGN_WIDTH * scale) as i32;
        let default_height = (DESIGN_HEIGHT * scale) as i32;
        let saved = load_settings_rect();
        let (x, y, w, h, dpi) = window_geometry(
            primary.as_ref().map(|monitor| monitor.work),
            primary_dpi,
            saved,
            default_width,
            default_height,
        );

        let state = Box::new(SettingsUi::new(dpi, devices));
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::PCWSTR(windows::core::HSTRING::from(CLASS_NAME).as_ptr()),
                windows::core::PCWSTR(windows::core::HSTRING::from("WinShort").as_ptr()),
                WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0),
                x,
                y,
                w,
                h,
                None,
                None,
                None,
                Some(Box::into_raw(state).cast()),
            )
        }
        .map_err(|e| Error::win("CreateWindowExW(settings)", &e))?;

        let actual_dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
        if actual_dpi != dpi {
            if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(hwnd) } {
                cell.borrow_mut().dpi = actual_dpi;
            }
        }
        apply_chrome(hwnd, settings_theme());
        Ok(Self {
            hwnd,
            picker: None,
            picker_owner: None,
            last_rect: saved,
        })
    }

    pub fn show(&mut self) -> Result<()> {
        // SAFETY: settings window is owned by this main-thread object.
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.closing = false;
            if !ui.dirty() {
                ui.replace_draft((*crate::app::config()).clone());
            }
            ui.validation.clear();
            invalidate(self.hwnd);
            ui.publish_automation_snapshot(self.hwnd);
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
        }
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut()
                .sync_focus_after_set_focus(self.hwnd, actual);
        }
        Ok(())
    }

    pub fn refresh_devices(&mut self, devices: crate::audio::devices::DeviceLists) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.devices = devices;
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }

    pub fn remember_position(&mut self) {
        let mut rect = RECT::default();
        let ok =
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(self.hwnd, &mut rect) };
        if ok.is_ok() {
            let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(self.hwnd) }.max(96);
            self.last_rect = Some(SavedSettingsRect { rect, dpi });
        }
    }

    pub fn persist_position(&mut self) {
        self.remember_position();
        let Some(saved) = self.last_rect else {
            return;
        };
        let path = crate::config::data_dir().join("settings-window.txt");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let contents = format!(
            "left={}\\ntop={}\\nright={}\\nbottom={}\\ndpi={}\\n",
            saved.rect.left, saved.rect.top, saved.rect.right, saved.rect.bottom, saved.dpi
        );
        let _ = std::fs::write(path, contents);
    }

    pub fn open_picker(
        &mut self,
        kind: PickerKind,
        devices: crate::audio::devices::DeviceLists,
        monitors: Vec<crate::platform::monitor::MonitorGeometry>,
    ) -> Result<()> {
        let Some(cell) = (unsafe { win::state_cell::<SettingsUi>(self.hwnd) }) else {
            return Err(Error::internal("settings state missing"));
        };
        if !cell.borrow().picker_activation_allowed() {
            return Ok(());
        }
        self.cancel_picker();
        let (draft, control_rect, dpi) = {
            let ui = cell.borrow();
            let element = picker_element(kind)
                .and_then(|id| ui.layout.element(id))
                .ok_or_else(|| Error::internal("settings picker row missing"))?;
            (
                ui.draft.clone(),
                controls::value_control_rect(element.rect, element.kind),
                ui.dpi,
            )
        };
        let anchor = screen_rect(self.hwnd, control_rect, dpi)?;
        let (choices, current) = picker_choices(kind, &draft, &devices, &monitors);
        if choices.is_empty() {
            return Err(Error::config("no choices available"));
        }
        let work =
            crate::platform::monitor::info_for(crate::platform::monitor::from_window(self.hwnd))
                .map(|monitor| {
                    PopupRect::new(
                        monitor.work.left,
                        monitor.work.top,
                        monitor.work.right,
                        monitor.work.bottom,
                    )
                })
                .unwrap_or(PopupRect::new(0, 0, 1920, 1080));
        let anchor = PopupRect::new(anchor.left, anchor.top, anchor.right, anchor.bottom);
        let scale = dpi.max(96) as f32 / 96.0;
        let width = (picker_width_dip(control_rect.w, &choices) * scale).round() as i32;
        let height = ((choices.len().min(10) as f32 * 30.0 + 8.0) * scale).round() as i32;
        let geometry = crate::ui::picker::place_popup(anchor, work, width, height);
        let owner =
            picker_element(kind).ok_or_else(|| Error::internal("settings picker row missing"))?;
        let picker = match PickerPopup::create(self.hwnd, kind, choices, current, geometry) {
            Ok(picker) => picker,
            Err(error) => {
                self.cancel_picker();
                return Err(error);
            }
        };
        let picker_hwnd = picker.hwnd;
        let picker_list_hwnd = picker.list;
        self.picker = Some(picker);
        self.picker_owner = Some(owner);
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().set_picker_open(
                self.hwnd,
                owner,
                picker_hwnd,
                picker_list_hwnd,
                actual,
            );
        }
        let can_activate = if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow().picker_activation_allowed()
        } else {
            false
        };
        if !can_activate {
            self.cancel_picker_without_focus();
            return Ok(());
        }
        if let Some(picker) = self.picker.as_ref() {
            picker.activate();
        }
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().sync_picker_focus(self.hwnd, actual);
        }
        Ok(())
    }

    pub fn commit_picker(&mut self, kind: PickerKind, value: PickerValue) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().apply_picker(kind, value);
        }
        invalidate(self.hwnd);
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().publish_automation_snapshot(self.hwnd);
        }
        self.cancel_picker();
    }

    pub fn cancel_picker(&mut self) {
        self.cancel_picker_impl(true);
    }

    pub(crate) fn cancel_picker_without_focus(&mut self) {
        self.cancel_picker_impl(false);
    }

    pub(crate) fn cancel_picker_for_device_cycle(&mut self, flow: crate::audio::DeviceCycleFlow) {
        let owner = match flow {
            crate::audio::DeviceCycleFlow::Input => ElementId::InputDevice,
            crate::audio::DeviceCycleFlow::Output => ElementId::OutputDevice,
        };
        if self.picker_owner == Some(owner) {
            self.cancel_picker_without_focus();
        }
    }

    pub(crate) fn merge_external_device_cycle(
        &mut self,
        flow: crate::audio::DeviceCycleFlow,
        old_live: &DeviceSelection,
        new_live: &DeviceSelection,
    ) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.merge_external_device_cycle(flow, old_live, new_live);
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }

    pub(crate) fn close_for_hide(&mut self) {
        let hwnd = self.hwnd;
        close_picker_before_settings_hide(
            || self.cancel_picker_without_focus(),
            || unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            },
        );
    }

    fn cancel_picker_impl(&mut self, restore_focus: bool) {
        let owner = self.picker_owner.take();
        let _ = self.picker.take();
        if restore_focus {
            unsafe {
                let _ = SetFocus(Some(self.hwnd));
            }
        }
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            let owner = owner.or(ui.picker_owner);
            ui.set_picker_closed(self.hwnd, owner, actual);
        }
    }
    pub fn focus_next_from_picker(&mut self, reverse: bool) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().focus_next(self.hwnd, reverse);
        }
        unsafe {
            let _ = SetFocus(Some(self.hwnd));
        }
        let actual = unsafe { GetFocus() };
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut()
                .sync_focus_after_set_focus(self.hwnd, actual);
        }
    }

    pub fn picker_hwnd(&self) -> Option<HWND> {
        self.picker.as_ref().map(|picker| picker.hwnd)
    }
}

fn load_settings_rect() -> Option<SavedSettingsRect> {
    let path = crate::config::data_dir().join("settings-window.txt");
    let text = std::fs::read_to_string(path).ok()?;
    let mut left = None;
    let mut top = None;
    let mut right = None;
    let mut bottom = None;
    let mut dpi: Option<u32> = None;
    for line in text.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "left" => left = value.parse().ok(),
            "top" => top = value.parse().ok(),
            "right" => right = value.parse().ok(),
            "bottom" => bottom = value.parse().ok(),
            "dpi" => dpi = value.parse().ok(),
            _ => {}
        }
    }
    Some(SavedSettingsRect {
        rect: RECT {
            left: left?,
            top: top?,
            right: right?,
            bottom: bottom?,
        },
        dpi: dpi?.max(96),
    })
}

fn window_geometry(
    primary_work: Option<RECT>,
    primary_dpi: u32,
    saved: Option<SavedSettingsRect>,
    default_width: i32,
    default_height: i32,
) -> (i32, i32, i32, i32, u32) {
    let fallback = primary_work.unwrap_or(RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    });
    if let Some(saved) = saved {
        let target = monitor_for_rect(saved.rect);
        let work = target
            .as_ref()
            .map(|monitor| monitor.work)
            .unwrap_or(fallback);
        let target_dpi = target
            .as_ref()
            .map(|monitor| monitor.dpi)
            .unwrap_or(primary_dpi)
            .max(96);
        let (width, height) = scaled_saved_size(saved.rect, saved.dpi, target_dpi);
        let rect = clamp_window_rect(
            RECT {
                left: saved.rect.left,
                top: saved.rect.top,
                right: saved.rect.left + width,
                bottom: saved.rect.top + height,
            },
            work,
        );
        return (
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            target_dpi,
        );
    }
    (
        fallback.left + ((fallback.right - fallback.left) - default_width) / 2,
        fallback.top + ((fallback.bottom - fallback.top) - default_height) / 2,
        default_width,
        default_height,
        primary_dpi.max(96),
    )
}

fn monitor_for_rect(rect: RECT) -> Option<crate::platform::monitor::MonitorGeometry> {
    let monitor = unsafe {
        windows::Win32::Graphics::Gdi::MonitorFromRect(
            &rect,
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        )
    };
    if monitor.is_invalid() {
        None
    } else {
        crate::platform::monitor::info_for(monitor)
    }
}

pub(crate) fn clamp_window_rect(saved: RECT, work: RECT) -> RECT {
    let width = (saved.right - saved.left)
        .max(320)
        .min((work.right - work.left).max(1));
    let height = (saved.bottom - saved.top)
        .max(260)
        .min((work.bottom - work.top).max(1));
    let left = saved.left.clamp(work.left, work.right - width);
    let top = saved.top.clamp(work.top, work.bottom - height);
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

fn scaled_saved_size(rect: RECT, saved_dpi: u32, target_dpi: u32) -> (i32, i32) {
    let scale = target_dpi.max(96) as f32 / saved_dpi.max(96) as f32;
    (
        ((rect.right - rect.left) as f32 * scale).round() as i32,
        ((rect.bottom - rect.top) as f32 * scale).round() as i32,
    )
}

fn picker_element(kind: PickerKind) -> Option<ElementId> {
    Some(match kind {
        PickerKind::InputDevice => ElementId::InputDevice,
        PickerKind::OutputDevice => ElementId::OutputDevice,
        PickerKind::InputRole => ElementId::InputRole,
        PickerKind::OutputRole => ElementId::OutputRole,
        PickerKind::OverlayAppearance => ElementId::OverlayAppearance,
        PickerKind::OverlayPosition => ElementId::OverlayPosition,
        PickerKind::OverlayMonitor => ElementId::OverlayMonitor,
    })
}

fn screen_rect(hwnd: HWND, rect: UiRect, dpi: u32) -> Result<RECT> {
    let scale = dpi.max(96) as f32 / 96.0;
    let mut top_left = windows::Win32::Foundation::POINT {
        x: (rect.x * scale) as i32,
        y: (rect.y * scale) as i32,
    };
    let mut bottom_right = windows::Win32::Foundation::POINT {
        x: (rect.right() * scale) as i32,
        y: (rect.bottom() * scale) as i32,
    };
    unsafe {
        if !windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut top_left).as_bool()
            || !windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut bottom_right).as_bool()
        {
            return Err(Error::config("settings picker anchor is unavailable"));
        }
    }
    Ok(RECT {
        left: top_left.x,
        top: top_left.y,
        right: bottom_right.x,
        bottom: bottom_right.y,
    })
}
fn picker_width_dip(control_width: f32, choices: &[PickerChoice]) -> f32 {
    const LABEL_ADVANCE_DIP: f32 = 7.5;
    const HORIZONTAL_PADDING_DIP: f32 = 36.0;
    const MAX_WIDTH_DIP: f32 = 440.0;
    let longest = choices
        .iter()
        .map(|choice| choice.label.chars().count() as f32)
        .fold(0.0, f32::max);
    control_width
        .max(longest * LABEL_ADVANCE_DIP + HORIZONTAL_PADDING_DIP)
        .min(MAX_WIDTH_DIP)
}

fn picker_choices(
    kind: PickerKind,
    draft: &Config,
    devices: &crate::audio::devices::DeviceLists,
    monitors: &[crate::platform::monitor::MonitorGeometry],
) -> (Vec<PickerChoice>, usize) {
    let mut choices = Vec::new();
    match kind {
        PickerKind::InputDevice => {
            choices.push(PickerChoice {
                label: "Default device".into(),
                value: PickerValue::Device(DeviceSelection::Default),
            });
            choices.extend(device_choices(&draft.audio.input_device, &devices.inputs));
        }
        PickerKind::OutputDevice => {
            choices.push(PickerChoice {
                label: "Default device".into(),
                value: PickerValue::Device(DeviceSelection::Default),
            });
            choices.extend(device_choices(&draft.audio.output_device, &devices.outputs));
        }
        PickerKind::InputRole => {
            for role in [
                EndpointRole::Console,
                EndpointRole::Multimedia,
                EndpointRole::Communications,
            ] {
                choices.push(PickerChoice {
                    label: role.label().into(),
                    value: PickerValue::Role(role),
                });
            }
        }
        PickerKind::OutputRole => {
            for role in [
                EndpointRole::Console,
                EndpointRole::Multimedia,
                EndpointRole::Communications,
            ] {
                choices.push(PickerChoice {
                    label: role.label().into(),
                    value: PickerValue::Role(role),
                });
            }
        }
        PickerKind::OverlayAppearance => {
            choices.extend(
                OverlayAppearance::ALL
                    .into_iter()
                    .map(|appearance| PickerChoice {
                        label: appearance.label().into(),
                        value: PickerValue::Appearance(appearance),
                    }),
            );
        }
        PickerKind::OverlayPosition => {
            choices.extend(
                OverlayPosition::ALL
                    .into_iter()
                    .map(|position| PickerChoice {
                        label: position.label().into(),
                        value: PickerValue::Position(position),
                    }),
            );
        }
        PickerKind::OverlayMonitor => {
            choices.push(PickerChoice {
                label: "Foreground window's monitor".into(),
                value: PickerValue::Monitor(MonitorChoice::Foreground),
            });
            choices.push(PickerChoice {
                label: "Primary monitor".into(),
                value: PickerValue::Monitor(MonitorChoice::Primary),
            });
            let primary = crate::platform::monitor::primary().map(|monitor| monitor.device_name);
            for monitor in monitors {
                let size = format!(
                    "{}×{}",
                    monitor.work.right - monitor.work.left,
                    monitor.work.bottom - monitor.work.top
                );
                let label = if primary.as_deref() == Some(monitor.device_name.as_str()) {
                    format!("Primary — {size}")
                } else {
                    format!("{} — {size}", monitor.device_name)
                };
                choices.push(PickerChoice {
                    label,
                    value: PickerValue::Monitor(MonitorChoice::Device(monitor.device_name.clone())),
                });
            }
            if let MonitorChoice::Device(name) = &draft.overlay.monitor {
                if !choices.iter().any(|choice| {
                    choice.value == PickerValue::Monitor(MonitorChoice::Device(name.clone()))
                }) {
                    choices.push(PickerChoice {
                        label: "Unavailable monitor — configured device".into(),
                        value: PickerValue::Monitor(MonitorChoice::Device(name.clone())),
                    });
                }
            }
        }
    }
    let current = current_picker_value(kind, draft);
    let current_index = choices
        .iter()
        .position(|choice| choice.value == current)
        .unwrap_or(0);
    (choices, current_index)
}

fn device_choices(
    current: &DeviceSelection,
    devices: &[crate::audio::DeviceId],
) -> Vec<PickerChoice> {
    let mut choices = devices
        .iter()
        .map(|device| PickerChoice {
            label: device.name.clone(),
            value: PickerValue::Device(DeviceSelection::Endpoint(device.endpoint.clone())),
        })
        .collect::<Vec<_>>();
    if let DeviceSelection::Endpoint(endpoint) = current {
        if !devices.iter().any(|device| device.endpoint == *endpoint) {
            choices.push(PickerChoice {
                label: "Selected device unavailable".into(),
                value: PickerValue::Device(DeviceSelection::Endpoint(endpoint.clone())),
            });
        }
    }
    choices
}

fn current_picker_value(kind: PickerKind, draft: &Config) -> PickerValue {
    match kind {
        PickerKind::InputDevice => PickerValue::Device(draft.audio.input_device.clone()),
        PickerKind::OutputDevice => PickerValue::Device(draft.audio.output_device.clone()),
        PickerKind::InputRole => PickerValue::Role(draft.audio.input_role),
        PickerKind::OutputRole => PickerValue::Role(draft.audio.output_role),
        PickerKind::OverlayAppearance => PickerValue::Appearance(draft.overlay.appearance),
        PickerKind::OverlayPosition => PickerValue::Position(draft.overlay.position),
        PickerKind::OverlayMonitor => PickerValue::Monitor(draft.overlay.monitor.clone()),
    }
}

fn apply_chrome(hwnd: HWND, theme: Theme) {
    unsafe {
        let dark: u32 = if theme.mode == ThemeMode::Dark { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let pref = DWMWCP_ROUND.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&pref as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        let c = theme.bg;
        let caption = COLORREF(c.r as u32 | ((c.g as u32) << 8) | ((c.b as u32) << 16));
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            (&caption as *const COLORREF).cast(),
            std::mem::size_of::<COLORREF>() as u32,
        );
    }
}

unsafe extern "system" fn settings_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: settings window is main-thread owned; state via WindowState cell.
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            let ui = Box::from_raw(cs.lpCreateParams as *mut SettingsUi);
            win::store_state_ptr(hwnd, win::WindowState::new(*ui));
            if let Some(cell) = win::state_cell::<SettingsUi>(hwnd) {
                cell.borrow_mut().install_automation(hwnd);
            }
            return win::def_proc(hwnd, msg, wparam, lparam);
        }

        let cell = match win::state_cell::<SettingsUi>(hwnd) {
            Some(cell) => cell,
            None => return win::def_proc(hwnd, msg, wparam, lparam),
        };

        if msg == WM_NCDESTROY {
            drop(win::take_state::<SettingsUi>(hwnd)); // outer unsafe scope
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_GETOBJECT => {
                // Clone only; UiaReturnRawElementProvider must run without a
                // live SettingsUi RefCell borrow.
                let automation = { cell.borrow().automation.clone() };
                automation
                    .and_then(|automation| automation.handle_get_object(hwnd, wparam, lparam))
                    .unwrap_or_else(|| win::def_proc(hwnd, msg, wparam, lparam))
            }
            WM_APP_SETTINGS_AUTOMATION => {
                let focus_requested = cell.borrow_mut().drain_automation_actions(hwnd);
                if focus_requested {
                    let _ = SetFocus(Some(hwnd));
                    let actual = GetFocus();
                    cell.borrow_mut().sync_focus_after_set_focus(hwnd, actual);
                }
                LRESULT(0)
            }
            WM_APP_SETTINGS_AUTOMATION_EVENTS => {
                // The borrow ends before flush_pending_events crosses into UIA.
                let automation = { cell.borrow().automation.clone() };
                if let Some(automation) = automation {
                    automation.flush_pending_events();
                }
                LRESULT(0)
            }
            WM_SETFOCUS => {
                cell.borrow_mut()
                    .on_window_focus(hwnd, true, HWND::default());
                LRESULT(0)
            }
            WM_KILLFOCUS => {
                let next = HWND(wparam.0 as *mut _);
                cell.borrow_mut().on_window_focus(hwnd, false, next);
                LRESULT(0)
            }
            WM_CLOSE => {
                let (should_close, capture_armed) = {
                    let mut ui = cell.borrow_mut();
                    if !ui.begin_close() {
                        (false, false)
                    } else {
                        let capture_armed = ui.recording.is_some() || ui.capture_armed;
                        ui.capture_armed = false;
                        ui.recording = None;
                        (true, capture_armed)
                    }
                };
                if should_close {
                    crate::event::post_main(crate::event::AppEvent::SettingsWindowClosed);
                    if capture_armed {
                        crate::keyboard::hook::end_capture();
                    }
                }
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                if let Err(e) = cell.borrow_mut().paint(hwnd) {
                    crate::error_!("settings paint failed: {e}");
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                let mut ui = cell.borrow_mut();
                if let Some(renderer) = ui.renderer.as_mut() {
                    let _ = renderer.resize();
                }
                ui.rebuild_layout(hwnd);
                ui.publish_automation_snapshot(hwnd);
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                // SetWindowPos below re-enters this proc with WM_SIZE; never hold
                // the state borrow across it.
                let new_dpi = ((wparam.0 >> 16) as u32).max(96);
                {
                    let mut ui = cell.borrow_mut();
                    ui.dpi = new_dpi;
                    if let Some(renderer) = ui.renderer.as_mut() {
                        let _ = renderer.set_dpi(new_dpi);
                    }
                }
                let suggested = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let mut ui = cell.borrow_mut();
                ui.rebuild_layout(hwnd);
                ui.publish_automation_snapshot(hwnd);
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                let theme = settings_theme();
                let mut ui = cell.borrow_mut();
                if let Some(renderer) = ui.renderer.as_mut() {
                    let _ = renderer.set_theme(theme);
                }
                apply_chrome(hwnd, theme);
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let mut ui = cell.borrow_mut();
                let (x, y) = mouse_point(lparam, ui.dpi);
                if !ui.mouse_tracking {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut track);
                    ui.mouse_tracking = true;
                }
                ui.update_hover(hwnd, x, y);
                if let Some(
                    id @ (ElementId::OverlayDuration
                    | ElementId::OverlayOpacity
                    | ElementId::OverlayScale),
                ) = ui.pressed
                {
                    ui.set_slider_from_x(id, x);
                    ui.publish_automation_snapshot(hwnd);
                    invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                let mut ui = cell.borrow_mut();
                ui.mouse_tracking = false;
                if let Some(old) = ui.hovered.take() {
                    ui.motion.animate_to(old, MotionChannel::Hover, 0.0, 140);
                    start_timer(hwnd);
                    invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let focus_requested = {
                    let mut ui = cell.borrow_mut();
                    let (x, y) = mouse_point(lparam, ui.dpi);
                    ui.rebuild_layout(hwnd);
                    let mut focus_requested = false;
                    if let Some(id) = ui.layout.hit_test(x, y) {
                        if !ui.is_disabled(id) {
                            focus_requested = true;
                            ui.pressed = Some(id);
                            ui.focused = Some(id);
                            let _ = SetCapture(hwnd);
                            if matches!(
                                id,
                                ElementId::OverlayDuration
                                    | ElementId::OverlayOpacity
                                    | ElementId::OverlayScale
                            ) {
                                ui.set_slider_from_x(id, x);
                            }
                            invalidate(hwnd);
                        }
                    }
                    ui.publish_automation_snapshot(hwnd);
                    focus_requested
                };
                if focus_requested {
                    let _ = SetFocus(Some(hwnd));
                    let actual = GetFocus();
                    cell.borrow_mut().sync_focus_after_set_focus(hwnd, actual);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let mut ui = cell.borrow_mut();
                let (x, y) = mouse_point(lparam, ui.dpi);
                let pressed = ui.pressed.take();
                let _ = ReleaseCapture();
                if let Some(id) = pressed {
                    if ui.layout.hit_test(x, y) == Some(id) {
                        ui.activate(hwnd, id);
                    }
                }
                invalidate(hwnd);
                ui.publish_automation_snapshot(hwnd);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
                let (picker_hwnd, scroll, max_scroll) = {
                    let ui = cell.borrow();
                    (ui.picker_hwnd, ui.scroll, ui.layout.max_scroll)
                };
                match settings_wheel_action(picker_hwnd, scroll, delta, max_scroll) {
                    SettingsWheelAction::ClosePicker(popup_hwnd) => {
                        crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
                            popup_hwnd: popup_hwnd.0 as isize,
                            restore_focus: true,
                        });
                    }
                    SettingsWheelAction::Scroll(scroll) => {
                        let mut ui = cell.borrow_mut();
                        ui.scroll = scroll;
                        ui.rebuild_layout(hwnd);
                        ui.publish_automation_snapshot(hwnd);
                        invalidate(hwnd);
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = wparam.0 as u16;
                {
                    let mut ui = cell.borrow_mut();
                    if ui.record_key(hwnd, vk, true) {
                        ui.publish_automation_snapshot(hwnd);
                        return LRESULT(0);
                    }
                    if ui.adjust_focused_slider(hwnd, vk) {
                        ui.publish_automation_snapshot(hwnd);
                        return LRESULT(0);
                    }
                }
                match vk {
                    0x09 => {
                        let reverse = key_down(0x10);
                        cell.borrow_mut().focus_next(hwnd, reverse);
                        let _ = SetFocus(Some(hwnd));
                        let actual = GetFocus();
                        cell.borrow_mut().sync_focus_after_set_focus(hwnd, actual);
                        invalidate(hwnd);
                        LRESULT(0)
                    }
                    0x0D | 0x20 => {
                        let focused = cell.borrow_mut().focused;
                        if let Some(id) = focused {
                            cell.borrow_mut().activate(hwnd, id);
                        }
                        LRESULT(0)
                    }
                    0x21 | 0x22 => {
                        let mut ui = cell.borrow_mut();
                        let page = ui.layout.content_clip.h.max(64.0);
                        ui.scroll = if vk == 0x21 {
                            (ui.scroll - page).max(0.0)
                        } else {
                            (ui.scroll + page).min(ui.layout.max_scroll)
                        };
                        ui.rebuild_layout(hwnd);
                        ui.publish_automation_snapshot(hwnd);
                        invalidate(hwnd);
                        LRESULT(0)
                    }
                    0x1B => {
                        let reset_confirm = cell.borrow().reset_confirm;
                        if reset_confirm {
                            cell.borrow_mut().reset_confirm = false;
                            invalidate(hwnd);
                        } else if cell.borrow_mut().begin_close() {
                            crate::event::post_main(crate::event::AppEvent::SettingsWindowClosed);
                        }
                        LRESULT(0)
                    }
                    _ => win::def_proc(hwnd, msg, wparam, lparam),
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                let mut ui = cell.borrow_mut();
                if ui.record_key(hwnd, wparam.0 as u16, false) {
                    return LRESULT(0);
                }
                win::def_proc(hwnd, msg, wparam, lparam)
            }
            WM_TIMER if wparam.0 == UI_TIMER => {
                let mut ui = cell.borrow_mut();
                // Drain the lock-free capture word while recording (#14/#45):
                // the global hook publishes atomically, so chords arrive here
                // instead of via WM_KEYDOWN.
                if ui.recording.is_some() {
                    while let Some(chord) = crate::keyboard::hook::take_captured_chord() {
                        ui.finish_recording(chord);
                    }
                    if ui.recording.is_none() {
                        crate::keyboard::hook::end_capture();
                        ui.capture_armed = false;
                    }
                }
                let active = ui.motion.tick();
                let applied = ui.applied_until.is_some_and(|until| Instant::now() < until);
                invalidate(hwnd);
                if !active && !applied && ui.recording.is_none() {
                    let _ = KillTimer(Some(hwnd), UI_TIMER);
                }
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let info =
                    &mut *(lparam.0 as *mut windows::Win32::UI::WindowsAndMessaging::MINMAXINFO);
                let scale = cell.borrow_mut().dpi as f32 / 96.0;
                info.ptMinTrackSize.x = (520.0 * scale) as i32;
                info.ptMinTrackSize.y = (500.0 * scale) as i32;
                LRESULT(0)
            }
            WM_CHAR => LRESULT(0),
            _ => win::def_proc(hwnd, msg, wparam, lparam),
        }
    }
}

fn client_size_dip(hwnd: HWND, dpi: u32) -> (f32, f32) {
    let mut rect = RECT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
    }
    let scale = 96.0 / dpi.max(96) as f32;
    (
        (rect.right - rect.left).max(0) as f32 * scale,
        (rect.bottom - rect.top).max(0) as f32 * scale,
    )
}

fn mouse_point(lparam: LPARAM, dpi: u32) -> (f32, f32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
    let scale = 96.0 / dpi.max(96) as f32;
    (x * scale, y * scale)
}
fn scroll_after_wheel(scroll: f32, delta: f32, max_scroll: f32) -> f32 {
    (scroll - delta / 120.0 * 64.0).clamp(0.0, max_scroll)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SettingsWheelAction {
    ClosePicker(HWND),
    Scroll(f32),
}

fn settings_wheel_action(
    picker_hwnd: Option<HWND>,
    scroll: f32,
    delta: f32,
    max_scroll: f32,
) -> SettingsWheelAction {
    picker_hwnd.map_or_else(
        || SettingsWheelAction::Scroll(scroll_after_wheel(scroll, delta, max_scroll)),
        SettingsWheelAction::ClosePicker,
    )
}

fn close_picker_before_settings_hide<Cancel, Hide>(cancel_picker: Cancel, hide_settings: Hide)
where
    Cancel: FnOnce(),
    Hide: FnOnce(),
{
    cancel_picker();
    hide_settings();
}
fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn start_timer(hwnd: HWND) {
    unsafe {
        let _ = SetTimer(Some(hwnd), UI_TIMER, UI_TIMER_MS, None);
    }
}

fn device_label(selection: &DeviceSelection, devices: &[crate::audio::DeviceId]) -> String {
    match selection {
        DeviceSelection::Default => "Default device".into(),
        DeviceSelection::Endpoint(id) => devices
            .iter()
            .find(|device| device.endpoint == *id)
            .map(|device| device.name.clone())
            .unwrap_or_else(|| "Selected device unavailable".into()),
    }
}

fn post_main(event: crate::event::AppEvent) {
    if let Some(hwnd) = crate::app::main_hwnd() {
        unsafe {
            let _ = crate::event::post_event(hwnd, event);
        }
    }
}

fn open_config_folder() {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    let folder = crate::config::data_dir();
    let _ = std::fs::create_dir_all(&folder);
    unsafe {
        let operation = HSTRING::from("open");
        let file = HSTRING::from(folder.to_string_lossy().as_ref());
        let _ = ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            None,
            None,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
}

fn modifier_for_vk(vk: u16) -> Option<ModifierMask> {
    Some(match vk {
        0x11 | 0xA2 | 0xA3 => ModifierMask::CTRL,
        0x12 | 0xA4 | 0xA5 => ModifierMask::ALT,
        0x10 | 0xA0 | 0xA1 => ModifierMask::SHIFT,
        0x5B | 0x5C => ModifierMask::WIN,
        _ => return None,
    })
}

unsafe fn key_down(vk: i32) -> bool {
    // SAFETY: GetKeyState is thread-affine read-only state.
    unsafe { (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(vk) as u16 & 0x8000) != 0 }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;

    #[test]
    fn reset_requires_two_explicit_activations() {
        let mut pending = false;
        assert!(!SettingsUi::consume_reset_confirmation(&mut pending));
        assert!(pending);
        assert!(SettingsUi::consume_reset_confirmation(&mut pending));
        assert!(!pending);
    }

    #[test]
    fn restored_window_rect_is_fully_inside_work_area() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 800,
        };
        let saved_rects = [
            RECT {
                left: 100,
                top: -500,
                right: 700,
                bottom: 100,
            },
            RECT {
                left: 100,
                top: 750,
                right: 700,
                bottom: 1350,
            },
            RECT {
                left: -1500,
                top: 100,
                right: -900,
                bottom: 700,
            },
            RECT {
                left: 1500,
                top: 100,
                right: 2100,
                bottom: 700,
            },
            RECT {
                left: -5000,
                top: -5000,
                right: 5000,
                bottom: 5000,
            },
        ];
        for saved in saved_rects {
            let clamped = clamp_window_rect(saved, work);
            assert!(clamped.left >= work.left);
            assert!(clamped.top >= work.top);
            assert!(clamped.right <= work.right);
            assert!(clamped.bottom <= work.bottom);
        }

        let negative_work = RECT {
            left: -1920,
            top: -100,
            right: 0,
            bottom: 980,
        };
        let clamped = clamp_window_rect(
            RECT {
                left: -5000,
                top: -5000,
                right: -4000,
                bottom: -4000,
            },
            negative_work,
        );
        assert!(clamped.left >= negative_work.left);
        assert!(clamped.top >= negative_work.top);
        assert!(clamped.right <= negative_work.right);
        assert!(clamped.bottom <= negative_work.bottom);
    }

    #[test]
    fn explicit_unavailable_device_is_preserved_in_picker() {
        let mut config = Config::default();
        config.audio.input_device = DeviceSelection::Endpoint("missing-endpoint".into());
        let devices = crate::audio::devices::DeviceLists {
            inputs: vec![crate::audio::DeviceId {
                endpoint: "current-endpoint".into(),
                name: "Current microphone".into(),
            }],
            outputs: Vec::new(),
            warnings: Vec::new(),
        };
        let (choices, current) = picker_choices(PickerKind::InputDevice, &config, &devices, &[]);
        assert_eq!(choices[current].label, "Selected device unavailable");
        assert_eq!(
            choices[current].value,
            PickerValue::Device(DeviceSelection::Endpoint("missing-endpoint".into()))
        );
    }
    #[test]
    fn endpoint_roles_apply_only_to_default_selection() {
        assert!(SettingsUi::endpoint_role_enabled(&DeviceSelection::Default));
        assert!(!SettingsUi::endpoint_role_enabled(
            &DeviceSelection::Endpoint("opaque".into(),)
        ));
    }
    #[test]
    fn slider_keyboard_steps_stay_within_validation_ranges() {
        assert_eq!(
            SettingsUi::slider_value(ElementId::OverlayDuration, 500.0, -1.0),
            500.0
        );
        assert_eq!(
            SettingsUi::slider_value(ElementId::OverlayDuration, 500.0, 1.0),
            600.0
        );
        assert_eq!(
            SettingsUi::slider_value(ElementId::OverlayDuration, 500.0, f32::INFINITY),
            10_000.0
        );
        assert!(
            (SettingsUi::slider_value(ElementId::OverlayOpacity, 0.3, -1.0) - 0.3).abs()
                < f32::EPSILON
        );
        assert!(
            (SettingsUi::slider_value(ElementId::OverlayOpacity, 0.3, 1.0) - 0.35).abs()
                < f32::EPSILON
        );
        assert!(
            (SettingsUi::slider_value(ElementId::OverlayScale, 0.7, -1.0) - 0.7).abs()
                < f32::EPSILON
        );
        assert!(
            (SettingsUi::slider_value(ElementId::OverlayScale, 0.7, 1.0) - 0.8).abs()
                < f32::EPSILON
        );
    }
    #[test]
    fn focus_policy_skips_disabled_and_wraps_both_directions() {
        let order = [
            ElementId::StartWithWindows,
            ElementId::InputRole,
            ElementId::OutputRole,
        ];
        let disabled = |id| id == ElementId::InputRole;
        assert_eq!(
            SettingsUi::next_focus_index(
                &order,
                Some(ElementId::StartWithWindows),
                false,
                disabled,
            ),
            ElementId::OutputRole
        );
        assert_eq!(
            SettingsUi::next_focus_index(&order, Some(ElementId::OutputRole), false, disabled,),
            ElementId::StartWithWindows
        );
        assert_eq!(
            SettingsUi::next_focus_index(&order, Some(ElementId::OutputRole), true, disabled,),
            ElementId::StartWithWindows
        );
    }
    #[test]
    fn saved_size_scales_from_saved_to_target_dpi() {
        let rect = RECT {
            left: 0,
            top: 0,
            right: 600,
            bottom: 400,
        };
        assert_eq!(scaled_saved_size(rect, 144, 144), (600, 400));
        assert_eq!(scaled_saved_size(rect, 96, 144), (900, 600));
        assert_eq!(scaled_saved_size(rect, 144, 96), (400, 267));
        assert_eq!(scaled_saved_size(rect, 0, 144), (900, 600));
    }

    #[test]
    fn value_for_uses_cached_devices_without_reacquiring_app() {
        let devices = crate::audio::devices::DeviceLists {
            inputs: vec![crate::audio::DeviceId {
                endpoint: "input".into(),
                name: "Cached microphone".into(),
            }],
            outputs: vec![crate::audio::DeviceId {
                endpoint: "output".into(),
                name: "Cached speakers".into(),
            }],
            warnings: Vec::new(),
        };
        let mut ui = SettingsUi::new(96, devices);
        ui.draft.audio.input_device = DeviceSelection::Endpoint("input".into());
        match ui.value_for(ElementId::InputDevice) {
            ControlValue::Text(value) => assert_eq!(value, "Cached microphone"),
            _ => panic!("unexpected control value variant"),
        }
    }
    #[test]
    fn high_contrast_settings_theme_uses_system_pairs_for_hover_and_focus() {
        let visual = SystemVisualPreferences {
            high_contrast: true,
            high_contrast_background: crate::platform::visual::VisualRgb { r: 8, g: 16, b: 24 },
            high_contrast_foreground: crate::platform::visual::VisualRgb {
                r: 240,
                g: 232,
                b: 224,
            },
            high_contrast_highlight: crate::platform::visual::VisualRgb {
                r: 32,
                g: 96,
                b: 160,
            },
            high_contrast_highlight_foreground: crate::platform::visual::VisualRgb {
                r: 255,
                g: 255,
                b: 255,
            },
            ..SystemVisualPreferences::default()
        };
        let theme = settings_theme_for(Theme::dark(), visual);
        assert_eq!(theme.card, Color::rgb(8, 16, 24));
        assert_eq!(theme.card_hover, theme.card);
        assert_eq!(theme.control_hover, theme.card);
        assert_eq!(theme.picker_hover, theme.card);
        assert_eq!(theme.border_strong, Color::rgb(240, 232, 224));
        assert_eq!(theme.focus, theme.border_strong);
        assert_eq!(theme.accent, Color::rgb(32, 96, 160));
        assert_eq!(theme.accent_text, Color::rgb(255, 255, 255));
    }
    #[test]
    fn picker_anchor_uses_the_value_control_rect() {
        let row = UiRect::new(24.0, 300.0, 560.0, 58.0);
        let control = controls::value_control_rect(row, crate::ui::layout::ElementKind::Value);
        assert!(control.x > row.x);
        let anchor = PopupRect::new(
            control.x as i32,
            control.y as i32,
            control.right() as i32,
            control.bottom() as i32,
        );
        let popup = crate::ui::picker::place_popup(
            anchor,
            PopupRect::new(0, 0, 1200, 900),
            anchor.width(),
            180,
        );
        assert_eq!(popup.left, anchor.left);
        assert_eq!(popup.top, anchor.bottom);
    }

    #[test]
    fn long_picker_labels_expand_width_without_exceeding_cap() {
        let short = vec![PickerChoice {
            label: "Top Left".into(),
            value: PickerValue::Position(OverlayPosition::TopLeft),
        }];
        let long = vec![PickerChoice {
            label: "A deliberately long endpoint name for the default device".into(),
            value: PickerValue::Position(OverlayPosition::TopLeft),
        }];
        assert_eq!(picker_width_dip(190.0, &short), 190.0);
        assert!(picker_width_dip(190.0, &long) > 190.0);
        assert!(picker_width_dip(190.0, &long) <= 440.0);
    }
    fn empty_settings_ui() -> SettingsUi {
        SettingsUi::new(
            96,
            crate::audio::devices::DeviceLists {
                inputs: Vec::new(),
                outputs: Vec::new(),
                warnings: Vec::new(),
            },
        )
    }
    #[test]
    fn snapshot_publication_defers_uia_delivery_past_settings_borrow() {
        use std::cell::{Cell, RefCell};
        use windows::Win32::UI::Accessibility::UIA_ToggleToggleStatePropertyId;

        let hwnd = HWND(std::ptr::dangling_mut());
        let cell = RefCell::new(empty_settings_ui());
        let automation = {
            let mut ui = cell.borrow_mut();
            ui.install_automation(hwnd);
            ui.draft.overlay.enabled = !ui.draft.overlay.enabled;
            ui.publish_automation_snapshot(hwnd);
            ui.automation.clone().expect("automation")
        };
        let delivery_count = Cell::new(0);
        automation.flush_pending_events_for_test(|automation| {
            delivery_count.set(delivery_count.get() + 1);
            assert!(cell.try_borrow().is_ok());
            let provider = automation.provider_for(ElementId::OverlayEnabled);
            let _ = unsafe {
                provider
                    .GetPropertyValue(UIA_ToggleToggleStatePropertyId)
                    .expect("provider re-query")
            };
        });
        assert!(delivery_count.get() > 0);
    }
    #[test]
    fn button_activation_queues_one_deferred_invoke_event() {
        use std::cell::Cell;

        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = empty_settings_ui();
        ui.install_automation(hwnd);
        ui.activate(hwnd, ElementId::InputDevice);
        let automation = ui.automation.clone().expect("automation");
        let delivered = Cell::new(0);
        automation.flush_pending_events_for_test(|automation| {
            delivered.set(delivered.get() + 1);
            let provider = automation.provider_for(ElementId::InputDevice);
            let _ = unsafe {
                provider
                    .GetPropertyValue(
                        windows::Win32::UI::Accessibility::UIA_IsInvokePatternAvailablePropertyId,
                    )
                    .expect("invoke provider re-query")
            };
        });
        assert_eq!(delivered.get(), 1);
    }

    #[test]
    fn picker_focus_state_suppresses_logical_child_focus() {
        let hwnd = HWND(2usize as *mut _);
        let mut ui = empty_settings_ui();
        ui.install_automation(hwnd);
        ui.focused = Some(ElementId::InputDevice);
        let picker_hwnd = HWND::default();
        let picker_list_hwnd = HWND(std::ptr::dangling_mut());
        ui.set_picker_open(
            hwnd,
            ElementId::InputDevice,
            picker_hwnd,
            picker_list_hwnd,
            hwnd,
        );
        let registered = ui.automation.as_ref().expect("automation").snapshot();
        assert_eq!(registered.focus_owner, AutomationFocusOwner::Settings);
        assert_eq!(registered.picker_open_for, Some(ElementId::InputDevice));
        assert!(
            registered
                .nodes
                .iter()
                .find(|node| node.id == ElementId::InputDevice)
                .expect("picker node")
                .focused
        );
        ui.on_window_focus(hwnd, false, picker_list_hwnd);
        let snapshot = ui.automation.as_ref().expect("automation").snapshot();
        assert_eq!(snapshot.focus_owner, AutomationFocusOwner::Picker);
        assert_eq!(snapshot.picker_open_for, Some(ElementId::InputDevice));
        assert!(snapshot.nodes.iter().all(|node| !node.focused));
        ui.set_picker_closed(hwnd, Some(ElementId::InputDevice), HWND::default());
        let snapshot = ui.automation.as_ref().expect("automation").snapshot();
        assert_eq!(snapshot.focus_owner, AutomationFocusOwner::Outside);
        assert_eq!(snapshot.picker_open_for, None);
        assert_eq!(snapshot.focused, Some(ElementId::InputDevice));
    }

    #[test]
    fn cancel_after_toggle_change_restores_visual_toggle_source() {
        let id = ElementId::OverlayEnabled;
        let mut ui = empty_settings_ui();
        ui.motion
            .animate_to(id, MotionChannel::ToggleState, 1.0, 160);
        let mut live = Config::default();
        live.overlay.enabled = false;
        ui.replace_draft(live);
        assert_eq!(ui.motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
    }

    #[test]
    fn reset_after_multiple_toggle_changes_matches_default_values() {
        let ids = [
            ElementId::StartHotkeysEnabled,
            ElementId::DesktopsEnabled,
            ElementId::WinNumberEnabled,
            ElementId::OverlayEnabled,
            ElementId::OverlayExternalChanges,
        ];
        let mut ui = empty_settings_ui();
        for id in ids {
            ui.motion
                .animate_to(id, MotionChannel::ToggleState, 0.0, 160);
        }
        ui.replace_draft(Config::default());
        for id in ids {
            assert_eq!(ui.motion.value(id, MotionChannel::ToggleState, 1.0), 1.0);
        }
    }

    #[test]
    fn active_toggle_animation_cannot_survive_model_replacement() {
        let id = ElementId::OverlayEnabled;
        let mut ui = empty_settings_ui();
        ui.motion
            .animate_to(id, MotionChannel::ToggleState, 1.0, 10_000);
        let mut replacement = Config::default();
        replacement.overlay.enabled = false;
        ui.replace_draft(replacement);
        assert_eq!(ui.motion.value(id, MotionChannel::ToggleState, 0.0), 0.0);
    }
    #[test]
    fn wheel_scroll_policy_is_independent_of_control_region() {
        assert_eq!(scroll_after_wheel(128.0, 120.0, 512.0), 64.0);
        assert_eq!(scroll_after_wheel(128.0, 120.0, 512.0), 64.0);
        assert_eq!(scroll_after_wheel(0.0, 120.0, 512.0), 0.0);
        assert_eq!(scroll_after_wheel(512.0, -120.0, 512.0), 512.0);
    }

    #[test]
    fn parent_scroll_closes_picker_before_scrolling() {
        let popup = HWND(7usize as *mut _);
        assert!(matches!(
            settings_wheel_action(Some(popup), 128.0, 120.0, 512.0),
            SettingsWheelAction::ClosePicker(hwnd) if hwnd == popup
        ));
        assert!(matches!(
            settings_wheel_action(None, 128.0, 120.0, 512.0),
            SettingsWheelAction::Scroll(scroll) if (scroll - 64.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn close_request_blocks_pending_picker_activation() {
        let mut ui = empty_settings_ui();
        assert!(ui.picker_activation_allowed());
        assert!(ui.begin_close());
        assert!(!ui.picker_activation_allowed());
        assert!(!ui.begin_close());
    }

    #[test]
    fn settings_hide_cancels_picker_before_hiding_parent() {
        use std::cell::RefCell;

        let steps = RefCell::new(Vec::new());
        close_picker_before_settings_hide(
            || steps.borrow_mut().push("picker"),
            || steps.borrow_mut().push("settings"),
        );
        assert_eq!(&*steps.borrow(), &["picker", "settings"]);
    }

    #[test]
    fn disabled_save_focus_is_repaired_before_snapshot_publication() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = empty_settings_ui();
        ui.install_automation(hwnd);
        ui.focus_owner = AutomationFocusOwner::Settings;
        ui.focused = Some(ElementId::Save);
        assert!(ui.is_disabled(ElementId::Save));

        ui.publish_automation_snapshot(hwnd);
        let snapshot = ui.automation.as_ref().expect("automation").snapshot();
        assert_ne!(snapshot.focused, Some(ElementId::Save));
        assert!(snapshot
            .nodes
            .iter()
            .all(|node| !node.focused || node.enabled));
        assert!(snapshot.focused.is_none_or(|id| snapshot
            .nodes
            .iter()
            .any(|node| node.id == id && node.enabled)));
    }

    #[test]
    fn dependent_disable_repairs_focus_and_preserves_tab_navigation() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = empty_settings_ui();
        ui.install_automation(hwnd);
        ui.focus_owner = AutomationFocusOwner::Settings;
        ui.focused = Some(ElementId::WinNumberEnabled);
        ui.draft.virtual_desktops.enabled = false;

        ui.publish_automation_snapshot(hwnd);
        let repaired = ui.focused.expect("repaired focus");
        assert_ne!(repaired, ElementId::WinNumberEnabled);
        assert!(!ui.is_disabled(repaired));

        let next =
            SettingsUi::next_focus_index(&ElementId::FOCUS_ORDER, Some(repaired), false, |id| {
                ui.is_disabled(id)
            });
        assert!(!ui.is_disabled(next));
        let previous =
            SettingsUi::next_focus_index(&ElementId::FOCUS_ORDER, Some(next), true, |id| {
                ui.is_disabled(id)
            });
        assert_eq!(previous, repaired);
    }

    #[test]
    fn focus_repair_does_not_duplicate_notifications() {
        use std::cell::Cell;

        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = empty_settings_ui();
        ui.install_automation(hwnd);
        ui.focus_owner = AutomationFocusOwner::Settings;
        ui.focused = Some(ElementId::Save);
        ui.publish_automation_snapshot(hwnd);
        let automation = ui.automation.clone().expect("automation");

        let first = Cell::new(0);
        automation.flush_pending_events_for_test(|_| first.set(first.get() + 1));
        assert_eq!(first.get(), 2);

        ui.publish_automation_snapshot(hwnd);
        let second = Cell::new(0);
        automation.flush_pending_events_for_test(|_| second.set(second.get() + 1));
        assert_eq!(second.get(), 0);
    }

    #[test]
    fn external_device_cycle_updates_clean_draft_and_preserves_unrelated_dirty_fields() {
        let old = DeviceSelection::Default;
        let new = DeviceSelection::Endpoint("next-input".into());
        let mut ui = empty_settings_ui();
        ui.draft.overlay.enabled = false;
        ui.merge_external_device_cycle(crate::audio::DeviceCycleFlow::Input, &old, &new);
        assert_eq!(ui.draft.audio.input_device, new);
        assert!(!ui.draft.overlay.enabled);
    }

    #[test]
    fn external_device_cycle_preserves_same_field_user_draft() {
        let old = DeviceSelection::Default;
        let new = DeviceSelection::Endpoint("live-next".into());
        let user_draft = DeviceSelection::Endpoint("user-choice".into());
        let mut ui = empty_settings_ui();
        ui.draft.audio.input_device = user_draft.clone();
        ui.merge_external_device_cycle(crate::audio::DeviceCycleFlow::Input, &old, &new);
        assert_eq!(ui.draft.audio.input_device, user_draft);
    }

    #[test]
    fn phase_one_hotkey_rows_are_in_focus_order_and_layout() {
        for id in [
            ElementId::CycleInputHotkey,
            ElementId::CycleOutputHotkey,
            ElementId::ForegroundVolumeUpHotkey,
            ElementId::ForegroundVolumeDownHotkey,
        ] {
            assert!(ElementId::FOCUS_ORDER.contains(&id));
            assert!(SettingsLayout::build(610.0, 720.0, 0.0)
                .element(id)
                .is_some());
        }
    }
    #[test]
    fn phase_one_hotkey_capture_maps_to_each_config_field() {
        let hotkey = Hotkey {
            modifiers: ModifierMask::CTRL.union(ModifierMask::ALT),
            key: VirtualKey(0x7C),
        };

        let mut ui = empty_settings_ui();
        for id in [
            ElementId::CycleInputHotkey,
            ElementId::CycleOutputHotkey,
            ElementId::ForegroundVolumeUpHotkey,
            ElementId::ForegroundVolumeDownHotkey,
        ] {
            ui.recording = Some(id);
            ui.finish_recording(crate::keyboard::hook::CapturedChord {
                modifiers: hotkey.modifiers,
                key: Some(hotkey.key),
            });
        }
        assert_eq!(ui.draft.hotkeys.cycle_input_device, Some(hotkey));
        assert_eq!(ui.draft.hotkeys.cycle_output_device, Some(hotkey));
        assert_eq!(ui.draft.hotkeys.foreground_volume_up, Some(hotkey));
        assert_eq!(ui.draft.hotkeys.foreground_volume_down, Some(hotkey));
    }
    #[test]
    fn applied_status_is_generic() {
        let status = APPLIED_STATUS.to_ascii_lowercase();
        assert!(status.contains("applied"));
        assert!(!status.contains("hotkey"));
    }
}
