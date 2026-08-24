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
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, KillTimer, SetTimer, SetWindowPos, ShowWindow, CREATESTRUCTW,
    GWLP_USERDATA, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CHAR, WM_CLOSE, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WM_TIMER, WS_OVERLAPPEDWINDOW,
};

use crate::config::model::{Config, DeviceSelection, EndpointRole, MonitorChoice, OverlayPosition};
use crate::config::validate::Violation;
use crate::error::{Error, Result};
use crate::keyboard::binding::{Hotkey, ModifierMask, VirtualKey};
use crate::platform::window as win;
use crate::ui::animation::Motion;
use crate::ui::controls::{self, ControlValue, Interaction};
use crate::ui::layout::{ElementId, ElementKind, Rect as UiRect, SettingsLayout};
use crate::ui::renderer::{rect, BrushRole, Renderer, TextStyle};
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
    validation: Vec<Violation>,
    hovered: Option<ElementId>,
    pressed: Option<ElementId>,
    recording_modifiers: ModifierMask,
    focused: Option<ElementId>,
    recording: Option<ElementId>,
    scroll: f32,
    motion: Motion,
    applied_until: Option<Instant>,
    mouse_tracking: bool,
}

impl SettingsUi {
    fn new(dpi: u32) -> Self {
        let draft = (*crate::app::config()).clone();
        Self {
            dpi,
            renderer: None,
            layout: SettingsLayout::build(DESIGN_WIDTH, DESIGN_HEIGHT, 0.0),
            draft,
            validation: Vec::new(),
            hovered: None,
            pressed: None,
            focused: None,
            recording_modifiers: ModifierMask::NONE,
            recording: None,
            scroll: 0.0,
            motion: Motion::default(),
            applied_until: None,
            mouse_tracking: false,
        }
    }

    fn ensure_renderer(&mut self, hwnd: HWND) -> Result<&mut Renderer> {
        if self.renderer.is_none() {
            self.renderer = Some(Renderer::new(hwnd, self.dpi, Theme::current())?);
        }
        Ok(self.renderer.as_mut().expect("created"))
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
            rect(70.0, 16.0 - self.scroll, self.layout.width - 24.0, 44.0 - self.scroll),
            TextStyle::Title,
            BrushRole::Text,
        );
        renderer.text(
            "Audio controls and desktop shortcuts, ready from the tray",
            rect(70.0, 42.0 - self.scroll, self.layout.width - 24.0, 64.0 - self.scroll),
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
                UiRect::new(status_rect.x + 16.0, status_rect.y, status_rect.w - 16.0, status_rect.h)
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
                UiRect::new(status_rect.x + 16.0, status_rect.y, status_rect.w - 16.0, status_rect.h)
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
        }
        result
    }

    fn value_for(&self, id: ElementId) -> ControlValue<'_> {
        match id {
            ElementId::StartWithWindows => ControlValue::Toggle(self.draft.general.start_with_windows),
            ElementId::StartHotkeysEnabled => {
                ControlValue::Toggle(self.draft.general.start_hotkeys_enabled)
            }
            ElementId::MicHotkey => self.hotkey_value(id, self.draft.hotkeys.toggle_microphone),
            ElementId::OutputHotkey => self.hotkey_value(id, self.draft.hotkeys.toggle_output),
            ElementId::ForegroundHotkey => {
                self.hotkey_value(id, self.draft.hotkeys.toggle_foreground_audio)
            }
            ElementId::InputDevice => {
                let devices = crate::app::with_app(|app| app.audio_devices().inputs)
                    .unwrap_or_default();
                ControlValue::Text(Cow::Owned(device_label(
                    &self.draft.audio.input_device,
                    &devices,
                )))
            }
            ElementId::OutputDevice => {
                let devices = crate::app::with_app(|app| app.audio_devices().outputs)
                    .unwrap_or_default();
                ControlValue::Text(Cow::Owned(device_label(
                    &self.draft.audio.output_device,
                    &devices,
                )))
            }
            ElementId::InputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.input_role.label()))
            }
            ElementId::OutputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.output_role.label()))
            }
            ElementId::DesktopsEnabled => {
                ControlValue::Toggle(self.draft.virtual_desktops.enabled)
            }
            ElementId::WinNumberEnabled => {
                ControlValue::Toggle(self.draft.virtual_desktops.win_number_switching)
            }
            ElementId::OverlayEnabled => ControlValue::Toggle(self.draft.overlay.enabled),
            ElementId::OverlayPosition => {
                ControlValue::Text(Cow::Borrowed(self.draft.overlay.position.label()))
            }
            ElementId::OverlayMonitor => {
                ControlValue::Text(Cow::Owned(self.draft.overlay.monitor.label()))
            }
            ElementId::OverlayDuration => ControlValue::Slider {
                ratio: (self.draft.overlay.duration_ms.saturating_sub(500) as f32 / 4500.0)
                    .clamp(0.0, 1.0),
                label: Cow::Owned(format!("{:.1}s", self.draft.overlay.duration_ms as f32 / 1000.0)),
            },
            ElementId::OverlayOpacity => ControlValue::Slider {
                ratio: ((self.draft.overlay.opacity - 0.3) / 0.7).clamp(0.0, 1.0),
                label: Cow::Owned(format!("{}%", (self.draft.overlay.opacity * 100.0).round() as u32)),
            },
            ElementId::OverlayScale => ControlValue::Slider {
                ratio: ((self.draft.overlay.scale - 0.7) / 0.9).clamp(0.0, 1.0),
                label: Cow::Owned(format!("{:.1}×", self.draft.overlay.scale)),
            },
            ElementId::OverlayPreview => ControlValue::Action(Cow::Borrowed("Preview")),
            ElementId::DiagnosticsStatus => {
                let text = crate::app::with_app(|app| {
                    let status = app.desktop_status();
                    let count = status
                        .desktop_count
                        .map(|count| format!(" • {count} desktops"))
                        .unwrap_or_default();
                    format!(
                        "{}{} • {}",
                        status.active.label(),
                        count,
                        status.native.label()
                    )
                })
                .unwrap_or_else(|| "Detecting virtual desktop backend…".into());
                ControlValue::Text(Cow::Owned(text))
            }
            ElementId::OpenConfigFolder => ControlValue::Action(Cow::Borrowed("Open folder")),
            ElementId::ResetSettings => ControlValue::Action(Cow::Borrowed("Reset draft")),
            ElementId::Cancel | ElementId::Save => ControlValue::Action(Cow::Borrowed("")),
        }
    }

    fn hotkey_value(&self, id: ElementId, hotkey: Option<Hotkey>) -> ControlValue<'_> {
        if self.recording == Some(id) {
            if self.recording_modifiers.is_empty() {
                ControlValue::Text(Cow::Borrowed("Press a shortcut…"))
            } else {
                ControlValue::Text(Cow::Owned(format!(
                    "{}…",
                    self.recording_modifiers
                )))
            }
        } else {
            ControlValue::Text(Cow::Owned(
                hotkey.map_or_else(|| "Not assigned".into(), |h| h.to_string()),
            ))
        }
    }

    fn interaction(&self, id: ElementId, disabled: bool) -> Interaction {
        let toggle_value = match id {
            ElementId::StartWithWindows => self.draft.general.start_with_windows,
            ElementId::StartHotkeysEnabled => self.draft.general.start_hotkeys_enabled,
            ElementId::DesktopsEnabled => self.draft.virtual_desktops.enabled,
            ElementId::WinNumberEnabled => self.draft.virtual_desktops.win_number_switching,
            ElementId::OverlayEnabled => self.draft.overlay.enabled,
            _ => false,
        };
        Interaction {
            hovered: self.hovered == Some(id),
            pressed: self.pressed == Some(id),
            focused: self.focused == Some(id),
            disabled,
            hover_t: self.motion.value(id, if self.hovered == Some(id) { 1.0 } else { 0.0 }),
            state_t: self.motion.value(id, if toggle_value { 1.0 } else { 0.0 }),
        }
    }

    fn is_disabled(&self, id: ElementId) -> bool {
        match id {
            ElementId::WinNumberEnabled => !self.draft.virtual_desktops.enabled,
            ElementId::OverlayPosition
            | ElementId::OverlayMonitor
            | ElementId::OverlayDuration
            | ElementId::OverlayOpacity
            | ElementId::OverlayScale
            | ElementId::OverlayPreview => !self.draft.overlay.enabled,
            ElementId::DiagnosticsStatus => true,
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

    fn set_slider_from_x(&mut self, id: ElementId, x: f32) {
        let Some(element) = self.layout.element(id) else { return };
        let row = element.rect.inset(1.0);
        let control_x = row.right() - 18.0 - 190.0;
        let track_w = 190.0 - 52.0 - 12.0;
        let ratio = ((x - control_x) / track_w).clamp(0.0, 1.0);
        match id {
            ElementId::OverlayDuration => {
                let raw = 500.0 + ratio * 4500.0;
                self.draft.overlay.duration_ms = (raw / 100.0).round() as u32 * 100;
            }
            ElementId::OverlayOpacity => {
                self.draft.overlay.opacity = ((0.3 + ratio * 0.7) * 20.0).round() / 20.0;
            }
            ElementId::OverlayScale => {
                self.draft.overlay.scale = ((0.7 + ratio * 0.9) * 10.0).round() / 10.0;
            }
            _ => {}
        }
        self.validation.clear();
    }

    fn activate(&mut self, hwnd: HWND, id: ElementId) {
        if self.is_disabled(id) {
            return;
        }
        match id {
            ElementId::StartWithWindows => {
                self.draft.general.start_with_windows = !self.draft.general.start_with_windows;
                self.animate_toggle(hwnd, id, self.draft.general.start_with_windows);
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
            }
            ElementId::InputDevice => {
                let devices = crate::app::with_app(|app| app.audio_devices().inputs)
                    .unwrap_or_default();
                self.draft.audio.input_device =
                    next_device(&self.draft.audio.input_device, &devices);
            }
            ElementId::OutputDevice => {
                let devices = crate::app::with_app(|app| app.audio_devices().outputs)
                    .unwrap_or_default();
                self.draft.audio.output_device =
                    next_device(&self.draft.audio.output_device, &devices);
            }
            ElementId::InputRole => {
                self.draft.audio.input_role = next_role(self.draft.audio.input_role);
            }
            ElementId::OutputRole => {
                self.draft.audio.output_role = next_role(self.draft.audio.output_role);
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
            ElementId::OverlayPosition => {
                self.draft.overlay.position = next_position(self.draft.overlay.position);
            }
            ElementId::OverlayMonitor => {
                self.draft.overlay.monitor = match self.draft.overlay.monitor {
                    MonitorChoice::Foreground => MonitorChoice::Primary,
                    MonitorChoice::Primary | MonitorChoice::Index(_) => MonitorChoice::Foreground,
                };
            }
            ElementId::OverlayPreview => post_main(crate::event::AppEvent::ShowStatusOverlay),
            ElementId::OpenConfigFolder => open_config_folder(),
            ElementId::ResetSettings => {
                self.draft = Config::default();
                self.validation.clear();
                self.recording = None;
            }
            ElementId::Cancel => {
                self.draft = (*crate::app::config()).clone();
                self.validation.clear();
                self.recording = None;
            }
            ElementId::Save => self.save(hwnd),
            ElementId::OverlayDuration
            | ElementId::OverlayOpacity
            | ElementId::OverlayScale
            | ElementId::DiagnosticsStatus => {}
        }
        self.validation.clear();
        invalidate(hwnd);
    }

    fn animate_toggle(&mut self, hwnd: HWND, id: ElementId, value: bool) {
        self.motion.animate_to(id, if value { 1.0 } else { 0.0 }, 160);
        start_timer(hwnd);
    }

    fn save(&mut self, hwnd: HWND) {
        self.validation = crate::config::validate(&self.draft);
        if !self.validation.is_empty() {
            invalidate(hwnd);
            return;
        }

        let old = crate::app::config();
        let startup_changed = old.general.start_with_windows != self.draft.general.start_with_windows;
        if startup_changed {
            if let Err(e) = crate::platform::startup::set_enabled(self.draft.general.start_with_windows) {
                self.validation.push(Violation {
                    field: "general.start_with_windows".into(),
                    message: e.to_string(),
                });
                invalidate(hwnd);
                return;
            }
        }

        if let Err(e) = crate::config::save::save(&crate::config::data_dir(), &self.draft) {
            if startup_changed {
                let _ = crate::platform::startup::set_enabled(old.general.start_with_windows);
            }
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
        let Some(id) = self.recording else { return false };
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
            ElementId::ForegroundHotkey => self.draft.hotkeys.toggle_foreground_audio = Some(hotkey),
            _ => {}
        }
        self.recording = None;
        self.recording_modifiers = ModifierMask::NONE;
        self.validation = crate::config::validate(&self.draft);
        invalidate(hwnd);
        true
    }

    fn focus_next(&mut self, reverse: bool) {
        let order = ElementId::FOCUS_ORDER;
        let current = self
            .focused
            .and_then(|id| order.iter().position(|candidate| *candidate == id));
        let mut index = current.unwrap_or(if reverse { 0 } else { order.len() - 1 });
        for _ in 0..order.len() {
            index = if reverse {
                if index == 0 { order.len() - 1 } else { index - 1 }
            } else {
                (index + 1) % order.len()
            };
            if !self.is_disabled(order[index]) {
                self.focused = Some(order[index]);
                self.scroll_focus_into_view(order[index]);
                break;
            }
        }
    }

    fn scroll_focus_into_view(&mut self, id: ElementId) {
        let Some(element) = self.layout.element(id) else { return };
        if !element.scrolls {
            return;
        }
        let top = self.layout.content_clip.y + 28.0;
        let bottom = self.layout.content_clip.bottom() - 12.0;
        if element.rect.y < top {
            self.scroll = (self.scroll - (top - element.rect.y)).clamp(0.0, self.layout.max_scroll);
        } else if element.rect.bottom() > bottom {
            self.scroll = (self.scroll + (element.rect.bottom() - bottom))
                .clamp(0.0, self.layout.max_scroll);
        }
    }
}

pub struct SettingsWindow {
    pub hwnd: HWND,
}

impl SettingsWindow {
    pub fn create() -> Result<Self> {
        let _atom = *REGISTERED.get_or_init(|| {
            win::register_class::<SettingsUi>(CLASS_NAME, Some(settings_wndproc))
                .expect("register settings class")
        });

        let primary = crate::platform::monitor::primary();
        let dpi = primary.as_ref().map_or(96, |m| m.dpi);
        let scale = dpi as f32 / 96.0;
        let w = (DESIGN_WIDTH * scale) as i32;
        let h = (DESIGN_HEIGHT * scale) as i32;
        let (x, y) = match &primary {
            Some(m) => (
                m.work.left + ((m.work.right - m.work.left) - w) / 2,
                m.work.top + ((m.work.bottom - m.work.top) - h) / 2,
            ),
            None => (0, 0),
        };

        let state = Box::new(SettingsUi::new(dpi));
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

        apply_chrome(hwnd, Theme::current());
        Ok(Self { hwnd })
    }

    pub fn show(&mut self) -> Result<()> {
        if let Some(ui) = unsafe { win::userdata::<SettingsUi>(self.hwnd) } {
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

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn is_visible(&self) -> bool {
        unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(self.hwnd).as_bool() }
    }

    pub fn refresh(&self) {
        invalidate(self.hwnd);
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
    if msg == WM_NCCREATE {
        let cs = &*(lparam.0 as *const CREATESTRUCTW);
        let ui = Box::from_raw(cs.lpCreateParams as *mut SettingsUi);
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            Box::into_raw(ui) as isize,
        );
        return win::def_proc(hwnd, msg, wparam, lparam);
    }

    let Some(ui) = win::userdata::<SettingsUi>(hwnd) else {
        return win::def_proc(hwnd, msg, wparam, lparam);
    };

    match msg {
        WM_CLOSE => {
            ui.recording = None;
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            if let Err(e) = ui.paint(hwnd) {
                crate::error_!("settings paint failed: {e}");
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            if let Some(renderer) = ui.renderer.as_mut() {
                let _ = renderer.resize();
            }
            ui.rebuild_layout(hwnd);
            invalidate(hwnd);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let new_dpi = ((wparam.0 >> 16) as u32).max(96);
            ui.dpi = new_dpi;
            if let Some(renderer) = ui.renderer.as_mut() {
                let _ = renderer.set_dpi(new_dpi);
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
            ui.rebuild_layout(hwnd);
            invalidate(hwnd);
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            let theme = Theme::current();
            if let Some(renderer) = ui.renderer.as_mut() {
                let _ = renderer.set_theme(theme);
            }
            apply_chrome(hwnd, theme);
            invalidate(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
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
            if let Some(id @ (ElementId::OverlayDuration | ElementId::OverlayOpacity | ElementId::OverlayScale)) = ui.pressed {
                ui.set_slider_from_x(id, x);
                invalidate(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            ui.mouse_tracking = false;
            if let Some(old) = ui.hovered.take() {
                ui.motion.animate_to(old, 0.0, 140);
                start_timer(hwnd);
                invalidate(hwnd);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = mouse_point(lparam, ui.dpi);
            ui.rebuild_layout(hwnd);
            if let Some(id) = ui.layout.hit_test(x, y) {
                if !ui.is_disabled(id) {
                    ui.pressed = Some(id);
                    ui.focused = Some(id);
                    let _ = SetCapture(hwnd);
                    if matches!(id, ElementId::OverlayDuration | ElementId::OverlayOpacity | ElementId::OverlayScale) {
                        ui.set_slider_from_x(id, x);
                    }
                    invalidate(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
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
            let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
            ui.scroll = (ui.scroll - delta / 120.0 * 64.0).clamp(0.0, ui.layout.max_scroll);
            ui.rebuild_layout(hwnd);
            invalidate(hwnd);
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = wparam.0 as u16;
            if ui.record_key(hwnd, vk, true) {
                return LRESULT(0);
            }
            match vk {
                0x09 => {
                    ui.focus_next(key_down(0x10));
                    invalidate(hwnd);
                    LRESULT(0)
                }
                0x0D | 0x20 => {
                    if let Some(id) = ui.focused {
                        ui.activate(hwnd, id);
                    }
                    LRESULT(0)
                }
                0x1B => {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    LRESULT(0)
                }
                _ => win::def_proc(hwnd, msg, wparam, lparam),
            }
        }
        WM_KEYUP | WM_SYSKEYUP => {
            if ui.record_key(hwnd, wparam.0 as u16, false) {
                return LRESULT(0);
            }
            win::def_proc(hwnd, msg, wparam, lparam)
        }
        WM_TIMER if wparam.0 == UI_TIMER => {
            let active = ui.motion.tick();
            let applied = ui.applied_until.is_some_and(|until| Instant::now() < until);
            invalidate(hwnd);
            if !active && !applied {
                let _ = KillTimer(Some(hwnd), UI_TIMER);
            }
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam.0 as *mut windows::Win32::UI::WindowsAndMessaging::MINMAXINFO);
            let scale = ui.dpi as f32 / 96.0;
            info.ptMinTrackSize.x = (520.0 * scale) as i32;
            info.ptMinTrackSize.y = (500.0 * scale) as i32;
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let ptr = ui as *mut SettingsUi;
            windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(ptr));
            win::def_proc(hwnd, msg, wparam, lparam)
        }
        WM_CHAR => LRESULT(0),
        _ => win::def_proc(hwnd, msg, wparam, lparam),
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

fn next_role(role: EndpointRole) -> EndpointRole {
    match role {
        EndpointRole::Console => EndpointRole::Multimedia,
        EndpointRole::Multimedia => EndpointRole::Communications,
        EndpointRole::Communications => EndpointRole::Console,
    }
}

fn next_position(position: OverlayPosition) -> OverlayPosition {
    let index = OverlayPosition::ALL
        .iter()
        .position(|candidate| *candidate == position)
        .unwrap_or(0);
    OverlayPosition::ALL[(index + 1) % OverlayPosition::ALL.len()]
}

fn device_label(
    selection: &DeviceSelection,
    devices: &[crate::audio::DeviceId],
) -> String {
    match selection {
        DeviceSelection::Default => "Default device".into(),
        DeviceSelection::Endpoint(id) => devices
            .iter()
            .find(|device| device.endpoint == *id)
            .map(|device| device.name.clone())
            .unwrap_or_else(|| "Selected device unavailable".into()),
    }
}

fn next_device(
    selection: &DeviceSelection,
    devices: &[crate::audio::DeviceId],
) -> DeviceSelection {
    match selection {
        DeviceSelection::Default => devices
            .first()
            .map(|device| DeviceSelection::Endpoint(device.endpoint.clone()))
            .unwrap_or(DeviceSelection::Default),
        DeviceSelection::Endpoint(current) => {
            let next = devices
                .iter()
                .position(|device| device.endpoint == *current)
                .and_then(|index| devices.get(index + 1));
            next.map(|device| DeviceSelection::Endpoint(device.endpoint.clone()))
                .unwrap_or(DeviceSelection::Default)
        }
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
    (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(vk) as u16 & 0x8000) != 0
}
