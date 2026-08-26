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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, KillTimer, SetTimer, SetWindowPos, ShowWindow, CREATESTRUCTW, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_CLOSE, WM_COMMAND,
    WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_HSCROLL, WM_KEYDOWN, WM_KEYUP,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
    WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WS_OVERLAPPEDWINDOW,
};

use crate::config::model::{
    Config, DeviceSelection, EndpointRole, MonitorChoice, OverlayAppearance, OverlayPosition,
};
use crate::config::validate::Violation;
use crate::error::{Error, Result};
use crate::keyboard::binding::{Hotkey, ModifierMask, VirtualKey};
use crate::platform::window as win;
use crate::ui::animation::Motion;
use crate::ui::controls::{self, ControlValue, Interaction};
use crate::ui::layout::{ElementId, Rect as UiRect, SettingsLayout};
use crate::ui::picker::{PickerChoice, PickerKind, PickerPopup, PickerValue, PopupRect};
use crate::ui::renderer::{rect, BrushRole, Renderer, TextStyle};
use crate::ui::settings_accessibility::SettingsAccessibility;
use crate::ui::theme::{Theme, ThemeMode};

pub const CLASS_NAME: &str = "WinShort.Settings";
pub const DESIGN_WIDTH: f32 = 610.0;
pub const DESIGN_HEIGHT: f32 = 720.0;
const UI_TIMER: usize = 1;
const UI_TIMER_MS: u32 = 16;

static REGISTERED: OnceLock<u16> = OnceLock::new();
const WM_MOUSELEAVE: u32 = 0x02A3;
static CONFIG_SEQ: AtomicU64 = AtomicU64::new(1);

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
    recording: Option<ElementId>,
    capture_armed: bool,
    reset_confirm: bool,
    scroll: f32,
    motion: Motion,
    applied_until: Option<Instant>,
    mouse_tracking: bool,
    accessibility: Option<SettingsAccessibility>,
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
            focused: None,
            recording_modifiers: ModifierMask::NONE,
            recording: None,
            capture_armed: false,
            reset_confirm: false,
            scroll: 0.0,
            motion: Motion::default(),
            applied_until: None,
            mouse_tracking: false,
            accessibility: None,
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

    fn paint(&mut self, hwnd: HWND) -> Result<()> {
        self.rebuild_layout(hwnd);
        if self.accessibility.is_none() {
            self.install_accessibility(hwnd);
        }
        self.refresh_focused_state();
        let dirty = self.dirty();
        let now = Instant::now();
        if self.applied_until.is_some_and(|until| now >= until) {
            self.applied_until = None;
        }

        let renderer = match self.renderer.take() {
            Some(renderer) => renderer,
            None => Renderer::new(hwnd, self.dpi, Theme::current())?,
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
                "Applied — hotkeys are live",
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
            self.sync_accessibility();
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
            hover_t: self
                .motion
                .value(id, if self.hovered == Some(id) { 1.0 } else { 0.0 }),
            state_t: self.motion.value(id, if toggle_value { 1.0 } else { 0.0 }),
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

    fn update_hover(&mut self, hwnd: HWND, x: f32, y: f32) {
        let next = self.layout.hit_test(x, y);
        if next != self.hovered {
            if let Some(old) = self.hovered {
                self.motion.animate_to(old, 0.0, 140);
            }
            if let Some(new) = next {
                self.motion.animate_to(new, 1.0, 140);
            }
            self.hovered = next;
            start_timer(hwnd);
            invalidate(hwnd);
        }
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
        if self.is_disabled(id) {
            return;
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
            ElementId::MicHotkey | ElementId::OutputHotkey | ElementId::ForegroundHotkey => {
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
                    self.draft = Config::default();
                    self.validation.clear();
                    crate::keyboard::hook::end_capture();
                    self.capture_armed = false;
                    self.recording = None;
                }
            }
            ElementId::Cancel => {
                self.draft = (*crate::app::config()).clone();
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
    }

    fn animate_toggle(&mut self, hwnd: HWND, id: ElementId, value: bool) {
        self.motion
            .animate_to(id, if value { 1.0 } else { 0.0 }, 160);
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
        if let Err(e) = crate::config::save::save(&crate::config::data_dir(), &self.draft) {
            self.validation.push(Violation {
                field: "config.toml".into(),
                message: e.to_string(),
            });
            invalidate(hwnd);
            return;
        }

        if let Some(handle) = crate::app::CONFIG.get() {
            handle.replace(self.draft.clone());
        }
        let seq = CONFIG_SEQ.fetch_add(1, Ordering::Relaxed);
        post_main(crate::event::AppEvent::ConfigApplied(seq));
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

    fn focus_next(&mut self, hwnd: HWND, reverse: bool) -> Option<HWND> {
        let order = ElementId::FOCUS_ORDER;
        let next = Self::next_focus_index(&order, self.focused, reverse, |id| self.is_disabled(id));
        self.scroll_focus_into_view(next);
        self.rebuild_layout(hwnd);
        self.focused = Some(next);
        self.accessibility
            .as_ref()
            .and_then(|accessibility| accessibility.focus_hwnd(next))
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
    fn install_accessibility(&mut self, hwnd: HWND) {
        if self.accessibility.is_none() {
            self.accessibility = Some(SettingsAccessibility::create(hwnd));
        }
    }

    fn refresh_focused_state(&mut self) {
        if let Some(accessibility) = &self.accessibility {
            if let Some(focused) = accessibility.focused_id() {
                self.focused = Some(focused);
            }
        }
    }

    fn sync_accessibility(&mut self) {
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
        if let Some(accessibility) = &mut self.accessibility {
            accessibility.sync(&self.layout, &values, self.dpi);
            if let Some(focused) = accessibility.focused_id() {
                self.focused = Some(focused);
            }
        }
    }

    fn accessibility_id_for(&self, hwnd: HWND) -> Option<ElementId> {
        self.accessibility.as_ref()?.id_for(hwnd)
    }

    fn accessibility_slider_ratio(&self, hwnd: HWND) -> Option<f32> {
        self.accessibility.as_ref()?.slider_ratio(hwnd)
    }
    fn accessibility_focus_hwnd(&self, id: ElementId) -> Option<HWND> {
        self.accessibility.as_ref()?.focus_hwnd(id)
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
        apply_chrome(hwnd, Theme::current());
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
            if !ui.dirty() {
                ui.draft = (*crate::app::config()).clone();
            }
            ui.validation.clear();
            invalidate(self.hwnd);
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(self.hwnd);
        }
        Ok(())
    }

    pub fn refresh_devices(&mut self, devices: crate::audio::devices::DeviceLists) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().devices = devices;
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
        self.cancel_picker();
        let Some(cell) = (unsafe { win::state_cell::<SettingsUi>(self.hwnd) }) else {
            return Err(Error::internal("settings state missing"));
        };
        let (draft, element_rect, dpi) = {
            let ui = cell.borrow();
            let element = picker_element(kind)
                .and_then(|id| ui.layout.element(id))
                .ok_or_else(|| Error::internal("settings picker row missing"))?;
            (ui.draft.clone(), element.rect, ui.dpi)
        };
        let anchor = screen_rect(self.hwnd, element_rect, dpi)?;
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
        let width = (300.0 * scale).round() as i32;
        let height = ((choices.len().min(10) as f32 * 30.0 + 8.0) * scale).round() as i32;
        let geometry = crate::ui::picker::place_popup(anchor, work, width, height);
        self.picker = Some(PickerPopup::create(
            self.hwnd, kind, choices, current, geometry,
        )?);
        self.picker_owner = picker_element(kind);
        Ok(())
    }

    pub fn commit_picker(&mut self, kind: PickerKind, value: PickerValue) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().apply_picker(kind, value);
        }
        invalidate(self.hwnd);
        self.cancel_picker();
    }

    pub fn cancel_picker(&mut self) {
        let owner = self.picker_owner.take();
        let _ = self.picker.take();
        let focus_target = owner.and_then(|owner| {
            let cell = unsafe { win::state_cell::<SettingsUi>(self.hwnd) }?;
            let mut ui = cell.borrow_mut();
            ui.focused = Some(owner);
            ui.accessibility_focus_hwnd(owner)
        });
        if let Some(child) = focus_target {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(child));
            }
            return;
        }
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(self.hwnd));
        }
    }

    pub fn focus_next_from_child(&mut self, reverse: bool) {
        let focus_target = unsafe { win::state_cell::<SettingsUi>(self.hwnd) }
            .and_then(|cell| cell.borrow_mut().focus_next(self.hwnd, reverse));
        invalidate(self.hwnd);
        if let Some(child) = focus_target {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(child));
            }
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
                cell.borrow_mut().install_accessibility(hwnd);
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
            WM_DRAWITEM => LRESULT(1),
            WM_COMMAND => {
                let source = HWND(lparam.0 as *mut _);
                let _control_id = crate::ui::picker::loword(wparam.0);
                let notification = crate::ui::picker::hiword(wparam.0);
                let handled = if notification == 0 {
                    let mut ui = cell.borrow_mut();
                    if let Some(id) = ui.accessibility_id_for(source) {
                        ui.focused = Some(id);
                        ui.activate(hwnd, id);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if handled {
                    let _ = SetFocus(Some(hwnd));
                    return LRESULT(0);
                }
                win::def_proc(hwnd, msg, wparam, lparam)
            }
            WM_HSCROLL => {
                let source = HWND(lparam.0 as *mut _);
                let mut ui = cell.borrow_mut();
                if let Some(id) = ui.accessibility_id_for(source) {
                    if let Some(ratio) = ui.accessibility_slider_ratio(source) {
                        ui.focused = Some(id);
                        ui.set_slider_from_ratio(id, ratio);
                        invalidate(hwnd);
                        return LRESULT(0);
                    }
                }
                win::def_proc(hwnd, msg, wparam, lparam)
            }
            WM_CLOSE => {
                crate::event::post_main(crate::event::AppEvent::SettingsWindowClosed);
                let mut ui = cell.borrow_mut();
                if ui.recording.is_some() || ui.capture_armed {
                    crate::keyboard::hook::end_capture();
                }
                ui.capture_armed = false;
                ui.recording = None;
                let _ = ShowWindow(hwnd, SW_HIDE);
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
                ui.sync_accessibility();
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
                ui.sync_accessibility();
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                let theme = Theme::current();
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
                    invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                let mut ui = cell.borrow_mut();
                ui.mouse_tracking = false;
                if let Some(old) = ui.hovered.take() {
                    ui.motion.animate_to(old, 0.0, 140);
                    start_timer(hwnd);
                    invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let mut ui = cell.borrow_mut();
                let (x, y) = mouse_point(lparam, ui.dpi);
                ui.rebuild_layout(hwnd);
                if let Some(id) = ui.layout.hit_test(x, y) {
                    if !ui.is_disabled(id) {
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
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let mut ui = cell.borrow_mut();
                let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
                ui.scroll = (ui.scroll - delta / 120.0 * 64.0).clamp(0.0, ui.layout.max_scroll);
                ui.rebuild_layout(hwnd);
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = wparam.0 as u16;
                {
                    let mut ui = cell.borrow_mut();
                    if ui.record_key(hwnd, vk, true) {
                        return LRESULT(0);
                    }
                    if ui.adjust_focused_slider(hwnd, vk) {
                        return LRESULT(0);
                    }
                }
                match vk {
                    0x09 => {
                        let focus_target = cell.borrow_mut().focus_next(hwnd, key_down(0x10));
                        if let Some(child) = focus_target {
                            let _ = SetFocus(Some(child));
                        }
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
                        invalidate(hwnd);
                        LRESULT(0)
                    }
                    0x1B => {
                        let reset_confirm = cell.borrow().reset_confirm;
                        if reset_confirm {
                            cell.borrow_mut().reset_confirm = false;
                            invalidate(hwnd);
                        } else {
                            crate::event::post_main(crate::event::AppEvent::SettingsWindowClosed);
                            let _ = ShowWindow(hwnd, SW_HIDE);
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
}
