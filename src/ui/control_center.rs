//! Owner-drawn native WinShort Control Center.
//!
//! The shell presents user actions first: Home, Shortcuts, Audio, Workspaces,
//! Displays, Overlay, System, and a deliberately separate Advanced surface.
//! Direct2D/DirectWrite rendering, event-driven state, native pickers, and the
//! existing UI Automation lifetime rules remain in one owner-drawn window.

use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, ScreenToClient, PAINTSTRUCT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, KillTimer, SetTimer, SetWindowPos, ShowWindow, CREATESTRUCTW, HTCAPTION,
    HTCLIENT, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_SHOW, WINDOWPOS, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CHAR, WM_CLOSE, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND, WM_GETOBJECT,
    WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SIZE,
    WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_WINDOWPOSCHANGING, WS_CLIPCHILDREN,
    WS_POPUP,
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
use crate::ui::control_center_automation::{
    node_has_invoke, snapshot_from_settings, AutomationFocusOwner, SettingsAutomation,
    SettingsAutomationAction, WM_APP_SETTINGS_AUTOMATION, WM_APP_SETTINGS_AUTOMATION_EVENTS,
};
use crate::ui::controls::{self, ControlValue, Interaction};
use crate::ui::layout::{
    overlay_placement_geometry, overlay_preview_canvas_rect, top_chrome_separator_rect, ElementId,
    ElementKind, HotkeySlot, LayoutContext, Rect as UiRect, RegionKind, SettingsLayout,
};
use crate::ui::navigation::{search, Page};
use crate::ui::picker::{PickerChoice, PickerKind, PickerPopup, PickerValue, PopupRect};
use crate::ui::presentation::{
    allowlist_mode, device_selection_presentation, display_output_label, format_desktop_modifier,
    format_hotkey, format_modifier as format_modifier_display, format_optional_hotkey,
    friendly_device, friendly_device_name, AllowlistMode, AudioDeviceKind,
    DeviceSelectionPresentation, DisplayWizardStep,
};
use crate::ui::prompt::{PromptAction, TextPrompt};
use crate::ui::renderer::{BrushRole, Renderer, TextStyle};
use crate::ui::theme::{Color, Theme, ThemeMode, UiTokens};

pub const CLASS_NAME: &str = "WinShort.ControlCenter";
pub const DESIGN_WIDTH: f32 = 960.0;
pub const DESIGN_HEIGHT: f32 = 660.0;
const UI_TIMER: usize = 1;
const UI_TIMER_MS: u32 = 16;
const WHEEL_SCROLL_DIP: f32 = 80.0;
const SEARCH_CARET_BLINK_MS: u64 = 530;
const APPLIED_STATUS: &str = "Changes applied";
const CONTROL_CENTER_STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_POPUP.0 | WS_CLIPCHILDREN.0);

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

#[derive(Debug, Clone)]
pub(crate) struct ControlCenterRuntimeSnapshot {
    pub microphone: crate::audio::AudioState,
    /// Resolved capture endpoint name, when the audio worker has one.
    pub microphone_name: Option<String>,
    pub output: crate::audio::OutputState,
    pub foreground: crate::audio::AppAudioState,
    pub desktop: crate::desktop::BackendStatus,
    pub degraded: Vec<(String, String)>,
    pub display_rollback_active: bool,
    pub display_keep_available: bool,
    pub display_rollback_error: Option<String>,
}
impl Default for ControlCenterRuntimeSnapshot {
    fn default() -> Self {
        Self {
            microphone: crate::audio::AudioState::Unavailable {
                reason: "Audio is starting".into(),
            },
            microphone_name: None,
            output: crate::audio::OutputState::Unavailable {
                reason: "Audio is starting".into(),
            },
            foreground: crate::audio::AppAudioState::no_external(),
            desktop: crate::desktop::BackendStatus {
                native: crate::desktop::BackendAvailability::Failed {
                    reason: "Workspace service is starting".into(),
                },
                fallback: crate::desktop::BackendAvailability::Available,
                active: crate::desktop::BackendKind::KeyboardFallback,
                desktop_count: None,
                current_desktop: None,
                last_served: None,
            },
            degraded: Vec::new(),
            display_rollback_active: false,

            display_keep_available: false,
            display_rollback_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayEditorState {
    step: DisplayWizardStep,
}
pub struct SettingsUi {
    dpi: u32,

    renderer: Option<Renderer>,
    layout: SettingsLayout,
    page: Page,
    search_query: String,
    draft: Config,
    selected_display_route: usize,
    /// Unsaved topology/name edits stay local until the profile is kept.
    display_draft_dirty: bool,
    /// Display profile editor remains local until the test/keep workflow ends.
    display_editor: Option<DisplayEditorState>,
    /// Connected display routes plus enough metadata for friendly cards.
    display_outputs: Vec<crate::display::DisplayOutput>,
    display_inventory_error: Option<String>,
    display_inventory_loaded: bool,
    onboarding_step: Option<u8>,
    startup_enabled: bool,
    display_rollback_active: bool,
    display_keep_available: bool,
    runtime: ControlCenterRuntimeSnapshot,
    devices: crate::audio::devices::DeviceLists,
    overlay_preview_aspect: (u32, u32),
    validation: Vec<Violation>,
    hovered: Option<ElementId>,
    pressed: Option<ElementId>,
    recording_modifiers: ModifierMask,
    focused: Option<ElementId>,
    /// Whether focus was established by keyboard or automation and should be
    /// rendered as keyboard-visible focus. Pointer focus remains semantically
    /// active without drawing a heavy ring.
    focus_visible: bool,
    search_caret_visible: bool,
    search_caret_deadline: Option<Instant>,
    focus_owner: AutomationFocusOwner,
    picker_owner: Option<ElementId>,
    picker_hwnd: Option<HWND>,
    picker_list_hwnd: Option<HWND>,
    recording: Option<ElementId>,
    capture_armed: bool,
    reset_confirm: bool,
    delete_profile_confirm: bool,
    scroll: f32,
    scroll_drag_offset: Option<f32>,
    motion: Motion,
    applied_until: Option<Instant>,
    closing: bool,
    mouse_tracking: bool,
    automation: Option<SettingsAutomation>,
}

impl SettingsUi {
    fn new(dpi: u32, devices: crate::audio::devices::DeviceLists) -> Self {
        let draft = (*crate::app::config()).clone();
        let data_dir = crate::config::data_dir();
        let page = Page::Home;
        let onboarding_step = crate::ui::first_run::should_show(&data_dir).then_some(1);
        Self {
            dpi,
            renderer: None,
            layout: SettingsLayout::build_shell(
                DESIGN_WIDTH,
                DESIGN_HEIGHT,
                0.0,
                page,
                "",
                draft.display_profiles.profiles.len(),
                onboarding_step,
            ),
            page,
            search_query: String::new(),
            draft,
            selected_display_route: 0,
            display_outputs: Vec::new(),
            display_inventory_error: None,
            display_inventory_loaded: false,
            display_draft_dirty: false,
            display_editor: None,
            onboarding_step,
            startup_enabled: false,
            display_rollback_active: false,
            display_keep_available: false,
            runtime: ControlCenterRuntimeSnapshot::default(),
            devices,
            overlay_preview_aspect: (16, 9),
            validation: Vec::new(),
            hovered: None,
            pressed: None,
            recording_modifiers: ModifierMask::NONE,
            focused: None,
            focus_visible: false,
            search_caret_visible: false,
            search_caret_deadline: None,
            focus_owner: AutomationFocusOwner::Outside,
            picker_list_hwnd: None,
            picker_owner: None,
            picker_hwnd: None,
            recording: None,
            capture_armed: false,
            reset_confirm: false,
            delete_profile_confirm: false,
            scroll: 0.0,
            scroll_drag_offset: None,
            motion: Motion::default(),
            applied_until: None,
            closing: false,
            mouse_tracking: false,
            automation: None,
        }
    }
    fn layout_context(&self) -> LayoutContext {
        LayoutContext {
            profile_count: self.draft.display_profiles.profiles.len(),
            display_output_count: self.display_route_candidates().len(),
            display_route_count: self
                .draft
                .display_profiles
                .active()
                .map_or(0, |profile| profile.routes.len()),
            display_editor_step: self.display_editor.map(|editor| editor.step),
            display_profiles_enabled: self.draft.display_profiles.enabled,
            display_draft_dirty: self.display_draft_dirty,
            display_rollback_active: self.display_rollback_active,
            display_keep_available: self.display_keep_available,
            display_inventory_unknown: self.display_inventory_error.is_some()
                || !self.display_inventory_loaded,
            workspace_enabled: self.draft.virtual_desktops.enabled,
            paused: !self.draft.general.start_hotkeys_enabled,
            current_app_audio_available: !matches!(
                self.runtime.foreground.aggregate,
                crate::audio::Aggregate::NoExternalApp
            ),
            input_cycle_mode: allowlist_mode(self.draft.audio.cycle_input_allowlist.as_deref()),
            output_cycle_mode: allowlist_mode(self.draft.audio.cycle_output_allowlist.as_deref()),
            input_device_count: self.devices.inputs.len(),
            output_device_count: self.devices.outputs.len(),
            overlay_preview_aspect: self.overlay_preview_aspect,
        }
    }
    fn refresh_overlay_preview_aspect(&mut self) {
        self.overlay_preview_aspect = overlay_preview_aspect(&self.draft.overlay.monitor);
    }

    fn rebuild_layout(&mut self, hwnd: HWND) {
        let (width, height) = self
            .renderer
            .as_ref()
            .map(Renderer::client_size_dip)
            .unwrap_or_else(|| client_size_dip(hwnd, self.dpi));
        self.layout = SettingsLayout::build_shell_with_context(
            width,
            height,
            self.scroll,
            self.page,
            &self.search_query,
            self.layout_context(),
            self.onboarding_step,
        );
        self.scroll = self.layout.scroll;
    }

    fn visual_focus(&self, id: ElementId) -> bool {
        self.focus_visible && self.focused == Some(id)
    }
    fn search_has_focus(&self) -> bool {
        self.focused == Some(ElementId::Search)
            && self.focus_owner == AutomationFocusOwner::Settings
    }
    fn set_pointer_focus(&mut self, target: Option<ElementId>) {
        self.focus_visible = false;
        self.focused = target.filter(|id| {
            let Some(element) = self.layout.element(*id) else {
                return false;
            };
            !matches!(element.kind, ElementKind::Card | ElementKind::Info) && !self.is_disabled(*id)
        });
    }
    fn clear_search_focus_without_settings_window(&mut self) {
        if self.focus_owner != AutomationFocusOwner::Settings
            && self.focused == Some(ElementId::Search)
        {
            self.focused = None;
        }
    }

    fn sync_search_caret(&mut self, hwnd: HWND) {
        if self.search_has_focus() {
            if self.search_caret_deadline.is_none() {
                self.search_caret_visible = true;
                self.search_caret_deadline =
                    Some(Instant::now() + Duration::from_millis(SEARCH_CARET_BLINK_MS));
            }
            start_timer(hwnd);
        } else {
            self.search_caret_visible = false;
            self.search_caret_deadline = None;
        }
    }

    fn restart_search_caret(&mut self, hwnd: HWND) {
        if self.search_has_focus() {
            self.search_caret_visible = true;
            self.search_caret_deadline =
                Some(Instant::now() + Duration::from_millis(SEARCH_CARET_BLINK_MS));
            start_timer(hwnd);
        } else {
            self.sync_search_caret(hwnd);
        }
    }

    fn tick_search_caret(&mut self, now: Instant) -> bool {
        if !self.search_has_focus() {
            self.search_caret_visible = false;
            self.search_caret_deadline = None;
            return false;
        }
        let Some(deadline) = self.search_caret_deadline else {
            return false;
        };
        if now < deadline {
            return true;
        }
        self.search_caret_visible = !self.search_caret_visible;
        self.search_caret_deadline = Some(now + Duration::from_millis(SEARCH_CARET_BLINK_MS));
        true
    }

    fn reset_scroll(&mut self) {
        self.scroll = 0.0;
    }

    fn set_scroll(&mut self, target: f32, hwnd: HWND) {
        self.scroll = target.clamp(0.0, self.layout.max_scroll);
        self.rebuild_layout(hwnd);
    }

    fn dirty(&self) -> bool {
        self.draft != *crate::app::config()
    }

    fn replace_draft(&mut self, draft: Config) {
        self.draft = draft;
        self.display_draft_dirty = false;
        self.display_editor = None;
        self.delete_profile_confirm = false;
        self.selected_display_route = 0;
        self.motion.clear_channel(MotionChannel::ToggleState);
    }

    fn discard_uncommitted_draft(&mut self) {
        if self.dirty() || self.display_draft_dirty || self.display_editor.is_some() {
            self.replace_draft((*crate::app::config()).clone());
        }
    }

    fn set_display_rollback_state(&mut self, active: bool, keep_available: bool) {
        self.display_rollback_active = active;
        self.display_keep_available = keep_available;
    }
    fn set_runtime_snapshot(&mut self, snapshot: ControlCenterRuntimeSnapshot) {
        self.display_rollback_active = snapshot.display_rollback_active;
        self.display_keep_available = snapshot.display_keep_available;
        self.runtime = snapshot;
    }

    fn set_page(&mut self, page: Page) {
        if page != Page::Displays && self.display_editor.is_some() {
            self.replace_draft((*crate::app::config()).clone());
        }
        self.page = page;
        if page == Page::Overlay {
            self.refresh_overlay_preview_aspect();
        }
        self.search_query.clear();
        self.reset_scroll();
        self.validation.clear();
    }

    fn commit_local_change(&mut self, hwnd: HWND, before: Config) -> bool {
        if self.display_draft_dirty {
            self.draft = before;
            self.validation = vec![Violation {
                field: "Displays".into(),
                message:
                    "Test or discard the current display edits before changing another setting"
                        .into(),
            }];
            invalidate(hwnd);
            return false;
        }
        let candidate = self.draft.clone();
        match crate::app::commit_config(
            candidate.clone(),
            crate::event::ConfigCommitOrigin::Settings,
        ) {
            Ok(()) => {
                self.draft = candidate;
                self.display_draft_dirty = false;
                self.validation.clear();
                self.applied_until = Some(Instant::now() + Duration::from_secs(2));
                start_timer(hwnd);
                true
            }
            Err(error) => {
                self.draft = before;
                self.validation = vec![Violation {
                    field: "Changes".into(),
                    message: error.to_string(),
                }];
                invalidate(hwnd);
                false
            }
        }
    }

    fn handle_search_key(&mut self, hwnd: HWND, vk: u16) -> bool {
        if !self.search_has_focus() {
            return false;
        }
        match vk {
            0x08 => self.search_query.pop(),
            0x1B if !self.search_query.is_empty() => {
                self.search_query.clear();
                None
            }
            0x0D => {
                if let Some(result) = search(&self.search_query).first() {
                    let page = result.item.page;
                    let target = result.item.target;
                    self.set_page(page);
                    self.focused = Some(target);
                }
                None
            }
            _ => return false,
        };
        self.restart_search_caret(hwnd);
        self.reset_scroll();
        self.rebuild_layout(hwnd);
        invalidate(hwnd);
        self.publish_automation_snapshot(hwnd);
        true
    }

    fn handle_search_char(&mut self, hwnd: HWND, ch: u16) -> bool {
        if !self.search_has_focus() || ch < 0x20 {
            return false;
        }
        if let Some(character) = char::from_u32(u32::from(ch)) {
            if !character.is_control() && self.search_query.chars().count() < 256 {
                self.search_query.push(character);
                self.restart_search_caret(hwnd);
                self.reset_scroll();
                self.rebuild_layout(hwnd);
                invalidate(hwnd);
                self.publish_automation_snapshot(hwnd);
                return true;
            }
        }
        false
    }
    fn active_profile_hotkey(&self) -> Option<Hotkey> {
        let id = self.draft.display_profiles.active()?.id.as_str();
        self.draft
            .hotkeys
            .display_profiles
            .iter()
            .find(|binding| binding.profile_id.eq_ignore_ascii_case(id))
            .map(|binding| binding.hotkey)
    }

    fn set_active_profile_hotkey(&mut self, hotkey: Option<Hotkey>) {
        let Some(id) = self
            .draft
            .display_profiles
            .active()
            .map(|profile| profile.id.clone())
        else {
            return;
        };
        self.draft
            .hotkeys
            .display_profiles
            .retain(|binding| !binding.profile_id.eq_ignore_ascii_case(&id) || hotkey.is_some());
        if let Some(hotkey) = hotkey {
            if let Some(binding) = self
                .draft
                .hotkeys
                .display_profiles
                .iter_mut()
                .find(|binding| binding.profile_id.eq_ignore_ascii_case(&id))
            {
                binding.hotkey = hotkey;
            } else {
                self.draft.hotkeys.display_profiles.push(
                    crate::config::model::DisplayProfileHotkey {
                        profile_id: id,
                        hotkey,
                    },
                );
            }
        }
    }
    fn selected_display_route(
        &self,
    ) -> Option<(
        &crate::display::DisplayProfile,
        &crate::display::DisplayRoute,
    )> {
        let profile = self.draft.display_profiles.active()?;
        profile
            .routes
            .get(self.selected_display_route)
            .map(|route| (profile, route))
    }

    fn refresh_display_outputs(&mut self) {
        self.display_inventory_loaded = true;
        match crate::display::output_inventory() {
            Ok(outputs) => {
                self.display_outputs = outputs;
                self.display_inventory_error = None;
            }
            Err(error) => {
                crate::warn_!("display output inventory unavailable: {error}");
                self.display_outputs.clear();
                self.display_inventory_error = Some(error.to_string());
            }
        }
    }
    fn output_label(&self, route: &crate::display::DisplayRoute) -> String {
        if self.display_inventory_error.is_some() {
            return "Screen status unknown".into();
        }
        if !self.display_inventory_loaded {
            return "Screen status not checked".into();
        }
        self.display_outputs
            .iter()
            .find(|output| crate::display::same_output(&output.route, route))
            .map(|output| {
                display_output_label(
                    &output.monitor_name,
                    &output.adapter_name,
                    &output.connector_name,
                    output.active,
                )
                .compact()
            })
            .unwrap_or_else(|| "Configured screen unavailable".into())
    }

    fn display_route_candidates(
        &self,
    ) -> Vec<(String, String, crate::display::DisplayRoute, bool)> {
        let mut candidates = self
            .display_outputs
            .iter()
            .map(|output| {
                let label = display_output_label(
                    &output.monitor_name,
                    &output.adapter_name,
                    &output.connector_name,
                    output.active,
                );
                (
                    label.primary,
                    label.detail.unwrap_or_else(|| "Connected screen".into()),
                    output.route.clone(),
                    true,
                )
            })
            .collect::<Vec<_>>();
        if let Some(profile) = self.draft.display_profiles.active() {
            for (index, route) in profile.routes.iter().enumerate() {
                if !self
                    .display_outputs
                    .iter()
                    .any(|output| crate::display::same_output(&output.route, route))
                {
                    candidates.push((
                        format!("Saved screen {}", index + 1),
                        if self.display_inventory_error.is_some() {
                            "Status unknown".into()
                        } else {
                            "Unavailable — reconnect this screen".into()
                        },
                        route.clone(),
                        false,
                    ));
                }
            }
        }
        candidates
    }

    fn display_output_card_data(&self, index: usize) -> Option<(String, String, bool, bool)> {
        let (primary, detail, route, available) =
            self.display_route_candidates().into_iter().nth(index)?;
        let selected = self.draft.display_profiles.active().is_some_and(|profile| {
            profile
                .routes
                .iter()
                .any(|configured| crate::display::same_output(configured, &route))
        });
        Some((primary, detail, selected, available))
    }

    fn display_outputs_label(&self) -> String {
        let Some(profile) = self.draft.display_profiles.active() else {
            return "No profile selected".into();
        };
        match profile.routes.as_slice() {
            [] => "No screens selected".into(),
            [route] => self.profile_route_label(0, route),
            routes => format!("{} screens selected", routes.len()),
        }
    }

    fn selected_display_route_label(&self) -> String {
        let Some((_, route)) = self.selected_display_route() else {
            return "No screen selected".into();
        };
        self.output_label(route)
    }

    fn selected_display_route_edit_value(&self) -> Option<String> {
        let (_, route) = self.selected_display_route()?;
        let refresh = if route.refresh_denominator == 1 {
            route.refresh_numerator.to_string()
        } else {
            format!("{}/{}", route.refresh_numerator, route.refresh_denominator)
        };
        Some(format!(
            "{},{},{},{},{},{}",
            route.source_position_x,
            route.source_position_y,
            route.source_width,
            route.source_height,
            refresh,
            rotation_degrees(route.rotation)
        ))
    }

    fn repair_focus(&mut self) {
        if self.focus_owner != AutomationFocusOwner::Settings {
            return;
        }
        let order = self.layout.focus_order();
        if order.is_empty() {
            self.focused = None;
            return;
        }
        let Some(current) = self.focused else {
            return;
        };
        if self.layout.element(current).is_some() && !self.is_disabled(current) {
            return;
        }
        let next = Self::next_focus_index(&order, Some(current), false, |id| self.is_disabled(id));
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
        if self
            .applied_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.applied_until = None;
        }

        let renderer = match self.renderer.take() {
            Some(renderer) => renderer,
            None => Renderer::new(hwnd, self.dpi, settings_theme())?,
        };
        renderer.begin();
        renderer.fill_rect(
            UiRect::new(0.0, 0.0, self.layout.nav_width, self.layout.height).d2d(),
            BrushRole::BackgroundSubtle,
        );
        renderer.line(
            self.layout.nav_width,
            0.0,
            self.layout.nav_width,
            self.layout.height,
            BrushRole::Border,
            1.0,
        );
        let brand = self.layout.brand;
        controls::draw_app_mark(&renderer, brand.icon);
        renderer.text(
            "WinShort",
            brand.text.d2d(),
            TextStyle::Section,
            BrushRole::Text,
        );
        for element in &self.layout.elements {
            if let ElementId::Nav(page) = element.id {
                controls::draw_nav_item(
                    &renderer,
                    element,
                    self.page,
                    self.hovered == Some(element.id),
                    self.pressed == Some(element.id),
                    self.visual_focus(element.id),
                );
                if page == Page::Advanced {
                    renderer.line(
                        28.0,
                        element.rect.y - 12.0,
                        self.layout.nav_width - 28.0,
                        element.rect.y - 12.0,
                        BrushRole::Border,
                        1.0,
                    );
                }
            }
        }

        renderer.push_clip(self.layout.content_clip.d2d());
        for section in &self.layout.sections {
            let section_rect = UiRect::new(
                self.layout.content_column.x,
                section.y,
                self.layout.content_column.w,
                section.height,
            );
            if self.layout.content_clip.intersects(section_rect) {
                if section.page_header {
                    controls::draw_page_header(
                        &renderer,
                        section_rect,
                        &section.title,
                        &section.description,
                    );
                } else {
                    controls::draw_section_header(
                        &renderer,
                        section_rect,
                        &section.title,
                        &section.description,
                    );
                }
            }
        }
        if self.onboarding_step.is_some() {
            self.draw_onboarding(&renderer);
        } else if !self.search_query.trim().is_empty() {
            self.draw_search_results(&renderer);
        } else {
            self.draw_page(&renderer);
        }
        controls::draw_scrollbar(
            &renderer,
            self.layout.content_clip,
            self.scroll,
            self.layout.max_scroll,
        );
        renderer.pop_clip();
        // Paint the persistent top bar after the scrollable viewport. The
        // viewport clip is still authoritative; this final layer also makes
        // the shell visually non-scrollable if a backend render call overdraws.
        self.draw_top_bar(&renderer);
        renderer.fill_rect(self.layout.footer.d2d(), BrushRole::BackgroundSubtle);
        renderer.line(
            0.0,
            self.layout.footer.y,
            self.layout.width,
            self.layout.footer.y,
            BrushRole::Border,
            1.0,
        );
        self.draw_footer(&renderer);
        let result = renderer.end();
        if result.is_ok() {
            self.renderer = Some(renderer);
            self.publish_automation_snapshot(hwnd);
        }
        result
    }
    fn draw_top_bar(&self, renderer: &Renderer) {
        renderer.fill_rect(self.layout.top_bar.d2d(), BrushRole::Background);
        let separator = top_chrome_separator_rect(self.layout.top_bar);
        renderer.line(
            separator.x,
            separator.y,
            separator.right(),
            separator.y,
            BrushRole::Border,
            1.0,
        );
        controls::draw_search_box(
            renderer,
            self.layout.search_rect,
            &self.search_query,
            self.search_has_focus(),
            self.hovered == Some(ElementId::Search),
            self.search_caret_visible,
        );
        if let Some(element) = self
            .layout
            .elements
            .iter()
            .find(|element| element.id == ElementId::WindowClose)
        {
            controls::draw_close_button(
                renderer,
                element,
                self.hovered == Some(ElementId::WindowClose),
                self.pressed == Some(ElementId::WindowClose),
                self.visual_focus(ElementId::WindowClose),
            );
        }
    }

    fn draw_page(&self, renderer: &Renderer) {
        self.draw_visual_regions(renderer);
        for element in &self.layout.elements {
            if !element.scrolls || !self.layout.content_clip.intersects(element.rect) {
                continue;
            }
            let interaction = self.interaction(element.id, self.is_disabled(element.id));
            match element.id {
                ElementId::HomeSpeaker => {
                    let detail = match &self.runtime.output {
                        crate::audio::OutputState::Current { muted: true, .. } => "Muted".into(),
                        crate::audio::OutputState::Current { volume_pct, .. } => {
                            format!("{volume_pct}% volume")
                        }
                        crate::audio::OutputState::Unavailable { .. } => {
                            "Windows Audio is not available".into()
                        }
                    };
                    controls::draw_home_card(
                        renderer,
                        element.rect,
                        "Speakers",
                        &self.current_output_name(),
                        &detail,
                        "Choose",
                        controls::IconKind::Speaker,
                        interaction,
                    );
                }
                ElementId::HomeMicrophone => {
                    let detail = match &self.runtime.microphone {
                        crate::audio::AudioState::Muted { volume_pct } => {
                            format!("Muted · {volume_pct}% input volume")
                        }
                        crate::audio::AudioState::Active { volume_pct } => {
                            format!("{volume_pct}% input volume")
                        }
                        crate::audio::AudioState::Unavailable { .. } => {
                            "Windows Audio is not available".into()
                        }
                    };
                    controls::draw_home_card(
                        renderer,
                        element.rect,
                        "Microphone",
                        &self.current_input_name(),
                        &detail,
                        "Choose",
                        controls::IconKind::Microphone,
                        interaction,
                    );
                }
                ElementId::HomeCurrentDesktop => {
                    let value = self.runtime.desktop.current_desktop.map_or_else(
                        || "Desktop status unavailable".into(),
                        |index| format!("Desktop {}", index + 1),
                    );
                    controls::draw_home_card(
                        renderer,
                        element.rect,
                        "Current desktop",
                        &value,
                        "Normal workspace",
                        "Open",
                        controls::IconKind::Page(Page::Workspaces),
                        interaction,
                    );
                }
                ElementId::HomeSpecial => {
                    let (value, detail, action) = self.special_workspace_summary();
                    controls::draw_home_card(
                        renderer,
                        element.rect,
                        "Special Desktop",
                        &value,
                        &detail,
                        &action,
                        controls::IconKind::Page(Page::Workspaces),
                        interaction,
                    );
                }
                ElementId::HomeDisplayProfile => {
                    let (name, detail) = self.display_summary();
                    let compact_detail = detail
                        .split_once(" · ")
                        .map_or(detail.as_str(), |(summary, _)| summary);
                    controls::draw_home_card(
                        renderer,
                        element.rect,
                        "Display",
                        &name,
                        compact_detail,
                        if self.draft.display_profiles.profiles.is_empty() {
                            "Set up"
                        } else {
                            "Open"
                        },
                        controls::IconKind::Page(Page::Displays),
                        interaction,
                    );
                }
                ElementId::HomeShortcutHealth => {
                    let (value, detail, action) = self.shortcut_health_copy();
                    controls::draw_home_card(
                        renderer,
                        element.rect,
                        "Shortcuts",
                        &value,
                        &detail,
                        &action,
                        controls::IconKind::Page(Page::Shortcuts),
                        interaction,
                    );
                }
                ElementId::HomeDiagnostics => {
                    let (title, value, detail) = self.home_diagnostics_copy();
                    controls::draw_home_card(
                        renderer,
                        element.rect,
                        &title,
                        &value,
                        &detail,
                        "Open",
                        controls::IconKind::Page(Page::System),
                        interaction,
                    );
                }
                ElementId::DisplayProfileCard(index) => {
                    if let Some((name, summary, shortcut, confirmed, needs_attention, selected)) =
                        self.profile_card_data(index as usize)
                    {
                        controls::draw_profile_card(
                            renderer,
                            element.rect,
                            &name,
                            &summary,
                            &shortcut,
                            confirmed,
                            needs_attention,
                            selected,
                            interaction,
                        );
                    }
                }
                ElementId::DisplayOutputCard(index) => {
                    if let Some((primary, detail, selected, available)) =
                        self.display_output_card_data(index as usize)
                    {
                        controls::draw_display_route_card(
                            renderer,
                            element,
                            &primary,
                            &detail,
                            selected,
                            available,
                            interaction,
                        );
                    }
                }
                ElementId::DisplayTopologyChoice(index) => {
                    let topology = if index == 0 {
                        crate::display::DisplayTopology::Extend
                    } else {
                        crate::display::DisplayTopology::Clone
                    };
                    let selected = self
                        .draft
                        .display_profiles
                        .active()
                        .is_some_and(|profile| profile.topology == topology);
                    let output_names = self.selected_display_names();
                    controls::draw_topology_choice(
                        renderer,
                        element,
                        topology.label(),
                        &element.description,
                        &output_names,
                        selected,
                        topology == crate::display::DisplayTopology::Clone,
                        interaction,
                    );
                }
                ElementId::HotkeyCard(slot) => {
                    controls::draw_hotkey_card(renderer, element, self.hotkey_enabled(slot));
                }
                ElementId::HotkeyEnabled(_) | ElementId::HotkeyUnassign(_) => {
                    let label = match self.value_for(element.id) {
                        ControlValue::Action(value) => value.into_owned(),
                        _ => element.label.clone(),
                    };
                    controls::draw_button_style(
                        renderer,
                        element.rect,
                        &label,
                        controls::ButtonStyle::Secondary,
                        interaction,
                    );
                }
                ElementId::MicHotkey
                | ElementId::OutputHotkey
                | ElementId::ForegroundHotkey
                | ElementId::CycleInputHotkey
                | ElementId::CycleOutputHotkey
                | ElementId::ForegroundVolumeUpHotkey
                | ElementId::ForegroundVolumeDownHotkey
                | ElementId::PreviousDesktopHotkey
                | ElementId::AssignScratchpadHotkey
                | ElementId::ToggleScratchpadHotkey
                | ElementId::DisplayProfileHotkey => {
                    let value = match self.value_for(element.id) {
                        ControlValue::Text(value) => value.into_owned(),
                        _ => String::new(),
                    };
                    controls::draw_hotkey_keycap(renderer, element, &value, interaction);
                }
                ElementId::InputDevice | ElementId::OutputDevice => {
                    let presentation = self.device_selection_view(element.id);
                    controls::draw_device_row(renderer, element, &presentation, interaction);
                }
                ElementId::InputCycleMode(_) | ElementId::OutputCycleMode(_) => {
                    controls::draw_choice(
                        renderer,
                        element,
                        self.choice_selected(element.id),
                        interaction,
                        true,
                    );
                }
                ElementId::InputCycleDevice(index) | ElementId::OutputCycleDevice(index) => {
                    let selected = self.cycle_device_selected(element.id, index as usize);
                    let label = self.cycle_device_label(element.id, index as usize);
                    controls::draw_labeled_choice(
                        renderer,
                        element,
                        &label,
                        selected,
                        interaction,
                        false,
                    );
                }
                ElementId::OverlayPositionCell(index) => {
                    controls::draw_position_cell(
                        renderer,
                        element,
                        overlay_position_label(index as usize),
                        self.draft.overlay.position == overlay_position(index as usize),
                        interaction,
                    );
                }
                ElementId::HomePreviousDesktop => {
                    controls::draw_row(renderer, element, self.value_for(element.id), interaction);
                }
                ElementId::RenameDisplayProfile => {
                    let name = self
                        .draft
                        .display_profiles
                        .active()
                        .map(|profile| profile.name.as_str())
                        .unwrap_or("No profile selected");
                    controls::draw_profile_name_row(renderer, element, name, interaction);
                }
                ElementId::DisplayWizardBack
                | ElementId::DisplayWizardNext
                | ElementId::DisplayWizardCancel
                | ElementId::EditDisplayProfile
                | ElementId::NewDisplayProfile
                | ElementId::UpdateDisplayProfile
                | ElementId::DuplicateDisplayProfile
                | ElementId::DeleteDisplayProfile
                | ElementId::TestApplyDisplayProfile
                | ElementId::ApplyDisplayProfile
                | ElementId::KeepDisplayChange
                | ElementId::UndoDisplayChange
                | ElementId::DiscardDisplayEdits
                | ElementId::OverlayPreview => {
                    if (element.id == ElementId::NewDisplayProfile
                        || element.id == ElementId::TestApplyDisplayProfile)
                        && element.rect.h > 40.0
                    {
                        controls::draw_row(
                            renderer,
                            element,
                            self.value_for(element.id),
                            interaction,
                        );
                    } else {
                        let label = match self.value_for(element.id) {
                            ControlValue::Action(value) => value.into_owned(),
                            _ => element.label.clone(),
                        };
                        controls::draw_button_style(
                            renderer,
                            element.rect,
                            &label,
                            if element.id == ElementId::DeleteDisplayProfile
                                || element.id == ElementId::DiscardDisplayEdits
                            {
                                controls::ButtonStyle::Danger
                            } else if matches!(
                                element.id,
                                ElementId::NewDisplayProfile
                                    | ElementId::TestApplyDisplayProfile
                                    | ElementId::KeepDisplayChange
                                    | ElementId::OverlayPreview
                                    | ElementId::DisplayWizardNext
                            ) {
                                controls::ButtonStyle::Primary
                            } else {
                                controls::ButtonStyle::Secondary
                            },
                            interaction,
                        );
                    }
                }
                _ if matches!(element.kind, ElementKind::Card | ElementKind::Info) => {}
                _ if element.id.is_shell_chrome() => {}
                _ => controls::draw_row(renderer, element, self.value_for(element.id), interaction),
            }
        }
        if self.page == Page::Home && !self.runtime.degraded.is_empty() {
            self.draw_degraded_summary(renderer);
        }
    }

    fn draw_search_results(&self, renderer: &Renderer) {
        for element in &self.layout.elements {
            if !element.scrolls
                || !self.layout.content_clip.intersects(element.rect)
                || element.kind == ElementKind::Card
            {
                continue;
            }
            controls::draw_row(
                renderer,
                element,
                self.value_for(element.id),
                self.interaction(element.id, false),
            );
        }
    }

    fn draw_onboarding(&self, renderer: &Renderer) {
        for element in &self.layout.elements {
            if !element.scrolls || !self.layout.content_clip.intersects(element.rect) {
                continue;
            }
            controls::draw_row(
                renderer,
                element,
                self.value_for(element.id),
                self.interaction(element.id, self.is_disabled(element.id)),
            );
        }
    }

    fn draw_visual_regions(&self, renderer: &Renderer) {
        for region in &self.layout.regions {
            if !self.layout.content_clip.intersects(region.rect) {
                continue;
            }
            match region.kind {
                RegionKind::WorkspaceNotice => self.draw_workspace_notice(renderer, region.rect),
                RegionKind::PauseNotice => self.draw_pause_notice(renderer, region.rect),
                RegionKind::AudioCurrentApp => self.draw_current_app_audio(renderer, region.rect),
                RegionKind::OverlayPreview => self.draw_overlay_preview(renderer, region.rect),
                RegionKind::DisplaySafety => self.draw_display_safety(renderer, region.rect),
                RegionKind::DisplayWizardSteps => self.draw_wizard_steps(renderer, region.rect),
                RegionKind::DisplayWizardSummary => {
                    self.draw_display_wizard_summary(renderer, region.rect)
                }
            }
        }
    }

    fn draw_workspace_notice(&self, renderer: &Renderer, rect: UiRect) {
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 10.0, BrushRole::Warning, 1.0);
        renderer.text(
            "Workspace shortcuts are turned off",
            UiRect::new(rect.x + 16.0, rect.y + 12.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            BrushRole::Warning,
        );
        renderer.text_clipped(
            "Enable Workspace shortcuts above to use desktop and Special actions.",
            UiRect::new(rect.x + 16.0, rect.y + 40.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::SectionDescription,
            BrushRole::TextSecondary,
        );
    }

    fn draw_pause_notice(&self, renderer: &Renderer, rect: UiRect) {
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 10.0, BrushRole::Warning, 1.0);
        renderer.text(
            "Shortcuts are paused",
            UiRect::new(rect.x + 16.0, rect.y + 12.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            BrushRole::Warning,
        );
        renderer.text(
            "Your configured bindings stay saved; they will not fire until resumed.",
            UiRect::new(rect.x + 16.0, rect.y + 40.0, rect.w - 32.0, 20.0).d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }

    fn draw_current_app_audio(&self, renderer: &Renderer, rect: UiRect) {
        let aggregate = self.runtime.foreground.aggregate;
        let (state, role) = match aggregate {
            crate::audio::Aggregate::AllMuted => ("Muted", BrushRole::Warning),
            crate::audio::Aggregate::AllActive => ("Active", BrushRole::Success),
            crate::audio::Aggregate::Mixed => ("Mixed sessions", BrushRole::Accent),
            crate::audio::Aggregate::NoSession => ("No audio session", BrushRole::TextSecondary),
            crate::audio::Aggregate::NoExternalApp => {
                ("No controllable app", BrushRole::TextSecondary)
            }
            crate::audio::Aggregate::Error => ("Audio unavailable", BrushRole::Danger),
        };
        let (primary, detail) = match aggregate {
            crate::audio::Aggregate::NoExternalApp => (
                "No controllable app".to_string(),
                "Switch to another app to control its audio".to_string(),
            ),
            crate::audio::Aggregate::NoSession => (
                self.runtime
                    .foreground
                    .app_name
                    .as_deref()
                    .map_or_else(|| state.to_string(), |app| format!("{app} · {state}")),
                "The selected app has no active audio session".to_string(),
            ),
            _ => {
                let app = self
                    .runtime
                    .foreground
                    .app_name
                    .as_deref()
                    .unwrap_or("Current app");
                (
                    format!("{app} · {state}"),
                    "Audio sessions from another app".to_string(),
                )
            }
        };
        renderer.fill_rounded(rect.d2d(), 8.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 8.0, role, 1.0);
        controls::draw_icon(
            renderer,
            UiRect::new(rect.x + 16.0, rect.y + 18.0, 28.0, 28.0),
            Page::Audio,
            role,
        );
        renderer.text_clipped(
            &primary,
            UiRect::new(rect.x + 58.0, rect.y + 10.0, rect.w - 76.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            BrushRole::Text,
        );
        renderer.text_clipped(
            &detail,
            UiRect::new(rect.x + 58.0, rect.y + 34.0, rect.w - 76.0, 20.0).d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }

    fn draw_overlay_preview(&self, renderer: &Renderer, rect: UiRect) {
        let placement = overlay_placement_geometry(rect, self.overlay_preview_aspect);
        let preview = placement.preview;
        renderer.text(
            "Preview",
            UiRect::new(preview.x, preview.y + 10.0, preview.w, 24.0).d2d(),
            TextStyle::Section,
            BrushRole::Text,
        );
        let controls = placement.controls;
        renderer.text(
            "Position",
            UiRect::new(controls.x, controls.y + 8.0, controls.w, 24.0).d2d(),
            TextStyle::Section,
            BrushRole::Text,
        );
        let canvas = overlay_preview_canvas_rect(preview, self.overlay_preview_aspect);
        renderer.fill_rounded(canvas.translated_y(2.0).d2d(), 8.0, BrushRole::Shadow);
        renderer.fill_rounded(canvas.d2d(), 8.0, BrushRole::Card);
        renderer.stroke_rounded(canvas.d2d(), 8.0, BrushRole::BorderStrong, 1.0);
        let sample = overlay_preview_card_rect(
            canvas,
            self.draft.overlay.position,
            self.draft.overlay.scale,
        );
        let sample_surface = match self.draft.overlay.appearance {
            OverlayAppearance::Dark => BrushRole::CardPressed,
            OverlayAppearance::Light => BrushRole::BackgroundSubtle,
            OverlayAppearance::System => BrushRole::Card,
        };
        renderer.fill_rounded(sample.translated_y(1.0).d2d(), 8.0, BrushRole::Shadow);
        renderer.fill_rounded(sample.d2d(), 8.0, sample_surface);
        renderer.stroke_rounded(sample.d2d(), 8.0, BrushRole::Accent, 1.0);
    }

    fn draw_display_safety(&self, renderer: &Renderer, rect: UiRect) {
        let disabled = !self.draft.display_profiles.enabled && !self.display_rollback_active;
        let recovery = self.runtime.display_rollback_error.is_some();
        let role = if recovery {
            BrushRole::Danger
        } else if disabled {
            BrushRole::TextSecondary
        } else {
            BrushRole::Warning
        };
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 10.0, role, 1.25);
        let title = if disabled {
            "Display profiles are turned off"
        } else if recovery {
            "Display recovery needs attention"
        } else {
            "Keep this display setup?"
        };
        let detail = if disabled {
            "Turn on Display profiles above to save and switch arrangements."
        } else if recovery {
            "The previous setup was not restored. Use Revert to retry recovery."
        } else {
            "Reverting automatically when the 15-second timer ends."
        };
        renderer.text_clipped(
            title,
            UiRect::new(rect.x + 16.0, rect.y + 14.0, rect.w - 32.0, 24.0).d2d(),
            TextStyle::BodyStrong,
            role,
        );
        renderer.text_clipped(
            detail,
            UiRect::new(rect.x + 16.0, rect.y + 44.0, rect.w - 32.0, 24.0).d2d(),
            TextStyle::SectionDescription,
            BrushRole::TextSecondary,
        );
    }

    fn draw_wizard_steps(&self, renderer: &Renderer, rect: UiRect) {
        let current = self
            .display_editor
            .map_or(DisplayWizardStep::Displays, |editor| editor.step);
        let width = rect.w / DisplayWizardStep::ALL.len() as f32;
        for (index, step) in DisplayWizardStep::ALL.into_iter().enumerate() {
            let x = rect.x + index as f32 * width;
            let selected = step == current;
            renderer.fill_rounded(
                UiRect::new(x, rect.y + 8.0, width - 8.0, 38.0).d2d(),
                7.0,
                if selected {
                    BrushRole::Accent
                } else {
                    BrushRole::Card
                },
            );
            renderer.text(
                &format!("{}  {}", step.number(), step.title()),
                UiRect::new(x + 8.0, rect.y + 8.0, width - 24.0, 38.0).d2d(),
                TextStyle::Caption,
                if selected {
                    BrushRole::AccentText
                } else {
                    BrushRole::TextSecondary
                },
            );
        }
    }

    fn draw_display_wizard_summary(&self, renderer: &Renderer, rect: UiRect) {
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::BackgroundSubtle);
        renderer.stroke_rounded(rect.d2d(), 10.0, BrushRole::Border, 1.0);
        if self
            .display_editor
            .is_some_and(|editor| editor.step == DisplayWizardStep::Review)
        {
            self.draw_display_review_summary(renderer, rect);
            return;
        }
        let profile = self.draft.display_profiles.active();
        let (name, summary, detail) = if let Some(profile) = profile {
            if self.display_inventory_error.is_some() || !self.display_inventory_loaded {
                (
                    profile.name.as_str(),
                    "Readiness unknown".to_string(),
                    if self.display_inventory_error.is_some() {
                        "Windows display information is unavailable".to_string()
                    } else {
                        "Windows display information has not been checked".to_string()
                    },
                )
            } else {
                let unavailable = profile
                    .routes
                    .iter()
                    .filter(|route| {
                        !self
                            .display_outputs
                            .iter()
                            .any(|output| crate::display::same_output(&output.route, route))
                    })
                    .count();
                let summary = if unavailable > 0 {
                    "Needs attention".to_string()
                } else if profile.routes.len() <= 1 {
                    "Single display".to_string()
                } else {
                    profile.topology.label().to_string()
                };
                let detail = if unavailable > 0 {
                    format!(
                        "{} selected · {} screen(s) unavailable",
                        profile.routes.len(),
                        unavailable
                    )
                } else {
                    format!("{} screen(s) selected", profile.routes.len())
                };
                (profile.name.as_str(), summary, detail)
            }
        } else {
            (
                "New display profile",
                "No screens selected".into(),
                "Select at least one screen to continue".into(),
            )
        };
        let summary_role = if matches!(summary.as_str(), "Needs attention" | "Readiness unknown") {
            BrushRole::Warning
        } else {
            BrushRole::Accent
        };
        renderer.text_clipped(
            name,
            UiRect::new(rect.x + 16.0, rect.y + 14.0, rect.w - 32.0, 24.0).d2d(),
            TextStyle::Section,
            BrushRole::Text,
        );
        renderer.text(
            &summary,
            UiRect::new(rect.x + 16.0, rect.y + 48.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            summary_role,
        );
        renderer.text_clipped(
            &detail,
            UiRect::new(rect.x + 16.0, rect.y + 78.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }

    fn draw_display_review_summary(&self, renderer: &Renderer, rect: UiRect) {
        let Some(profile) = self.draft.display_profiles.active().cloned() else {
            return;
        };
        let lines = [
            ("Profile", profile.name.clone()),
            ("Screens", self.display_review_screen_names(&profile)),
            (
                "Arrangement",
                if profile.routes.len() <= 1 {
                    "Single display".into()
                } else {
                    profile.topology.label().into()
                },
            ),
            (
                "Shortcut",
                format_optional_hotkey(self.active_profile_hotkey()),
            ),
            ("Readiness", self.display_review_readiness(&profile)),
        ];
        for (index, (label, value)) in lines.into_iter().enumerate() {
            let y = rect.y + 10.0 + index as f32 * 31.0;
            renderer.text_clipped(
                label,
                UiRect::new(rect.x + 16.0, y, 92.0, 20.0).d2d(),
                TextStyle::Caption,
                BrushRole::TextSecondary,
            );
            renderer.text_clipped(
                &value,
                UiRect::new(rect.x + 116.0, y, rect.w - 132.0, 22.0).d2d(),
                TextStyle::Body,
                if label == "Readiness" && value.starts_with("Ready") {
                    BrushRole::Success
                } else if label == "Readiness" {
                    BrushRole::Warning
                } else {
                    BrushRole::Text
                },
            );
        }
    }

    fn display_review_screen_names(&self, profile: &crate::display::DisplayProfile) -> String {
        if profile.routes.is_empty() {
            return "No screens selected".into();
        }
        profile
            .routes
            .iter()
            .enumerate()
            .map(|(index, route)| self.display_review_screen_name(index, route))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn display_review_readiness(&self, profile: &crate::display::DisplayProfile) -> String {
        if self.display_inventory_error.is_some() {
            return "Unknown · Windows display information unavailable".into();
        }
        if !self.display_inventory_loaded {
            return "Unknown · Windows display information not checked".into();
        }
        let missing = profile
            .routes
            .iter()
            .filter(|route| {
                !self
                    .display_outputs
                    .iter()
                    .any(|output| crate::display::same_output(&output.route, route))
            })
            .count();
        if missing > 0 {
            format!("Needs attention · {missing} screen(s) unavailable")
        } else if profile.routes.is_empty() || !profile.confirmed {
            "Needs test before activation".into()
        } else {
            "Ready to activate".into()
        }
    }

    fn draw_degraded_summary(&self, renderer: &Renderer) {
        let Some((name, _)) = self.runtime.degraded.first() else {
            return;
        };
        let rect = UiRect::new(
            self.layout.content_column.x,
            self.layout.content_clip.bottom() - 74.0,
            self.layout.content_column.w,
            58.0,
        );
        renderer.fill_rounded(rect.d2d(), 10.0, BrushRole::Card);
        renderer.stroke_rounded(rect.d2d(), 10.0, BrushRole::Warning, 1.0);
        renderer.text(
            &format!("{} unavailable", human_subsystem_name(name)),
            UiRect::new(rect.x + 16.0, rect.y + 8.0, rect.w - 32.0, 22.0).d2d(),
            TextStyle::BodyStrong,
            BrushRole::Warning,
        );
        renderer.text(
            "Open Diagnostics for the full technical reason.",
            UiRect::new(rect.x + 16.0, rect.y + 34.0, rect.w - 32.0, 18.0).d2d(),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
    }

    fn profile_route_label(&self, index: usize, route: &crate::display::DisplayRoute) -> String {
        if self.display_inventory_error.is_some() {
            format!("Saved screen {} · Status unknown", index + 1)
        } else if !self.display_inventory_loaded {
            format!("Saved screen {} · Status not checked", index + 1)
        } else if self
            .display_outputs
            .iter()
            .any(|output| crate::display::same_output(&output.route, route))
        {
            self.output_label(route)
        } else {
            format!("Saved screen {} · Unavailable", index + 1)
        }
    }
    fn display_screen_name(&self, index: usize, route: &crate::display::DisplayRoute) -> String {
        if self.display_inventory_error.is_some() || !self.display_inventory_loaded {
            return format!("Saved screen {}", index + 1);
        }
        self.display_outputs
            .iter()
            .find(|output| crate::display::same_output(&output.route, route))
            .map(|output| {
                display_output_label(
                    &output.monitor_name,
                    &output.adapter_name,
                    &output.connector_name,
                    output.active,
                )
                .primary
            })
            .unwrap_or_else(|| format!("Saved screen {}", index + 1))
    }
    fn display_review_screen_name(
        &self,
        index: usize,
        route: &crate::display::DisplayRoute,
    ) -> String {
        let name = self.display_screen_name(index, route);
        if self.display_inventory_error.is_none()
            && self.display_inventory_loaded
            && !self
                .display_outputs
                .iter()
                .any(|output| crate::display::same_output(&output.route, route))
        {
            format!("{name} · Unavailable")
        } else {
            name
        }
    }

    fn selected_display_names(&self) -> Vec<String> {
        self.draft
            .display_profiles
            .active()
            .map(|profile| {
                profile
                    .routes
                    .iter()
                    .enumerate()
                    .take(2)
                    .map(|(index, route)| self.display_screen_name(index, route))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn profile_card_data(
        &self,
        index: usize,
    ) -> Option<(String, String, String, bool, bool, bool)> {
        let profile = self.draft.display_profiles.profiles.get(index)?;
        let missing = if self.display_inventory_error.is_some() || !self.display_inventory_loaded {
            0
        } else {
            profile
                .routes
                .iter()
                .filter(|route| {
                    !self
                        .display_outputs
                        .iter()
                        .any(|output| crate::display::same_output(&output.route, route))
                })
                .count()
        };
        let summary = match profile.routes.as_slice() {
            [] => "No displays selected".into(),
            [route] => self.profile_route_label(0, route),
            routes => {
                let names = routes
                    .iter()
                    .enumerate()
                    .take(2)
                    .map(|(index, route)| self.profile_route_label(index, route))
                    .collect::<Vec<_>>();
                if routes.len() > 2 {
                    format!("{} + {} more", names.join(" · "), routes.len() - 2)
                } else {
                    names.join(" · ")
                }
            }
        };
        let shortcut = self
            .draft
            .hotkeys
            .display_profiles
            .iter()
            .find(|binding| binding.profile_id.eq_ignore_ascii_case(&profile.id))
            .map_or_else(
                || "No shortcut".into(),
                |binding| format_hotkey(binding.hotkey),
            );
        let selected = self
            .draft
            .display_profiles
            .active_profile
            .as_deref()
            .is_some_and(|id| id.eq_ignore_ascii_case(&profile.id));
        Some((
            profile.name.clone(),
            summary,
            shortcut,
            profile.confirmed
                && missing == 0
                && !profile.routes.is_empty()
                && self.display_inventory_loaded
                && self.display_inventory_error.is_none(),
            self.display_inventory_error.is_some() || !self.display_inventory_loaded || missing > 0,
            selected,
        ))
    }

    fn current_input_name(&self) -> String {
        match &self.runtime.microphone {
            crate::audio::AudioState::Unavailable { .. } => "Microphone unavailable".into(),
            crate::audio::AudioState::Muted { .. } | crate::audio::AudioState::Active { .. } => {
                let raw = self
                    .runtime
                    .microphone_name
                    .clone()
                    .or_else(|| {
                        self.devices
                            .input_defaults
                            .for_role(self.draft.audio.input_role)
                            .map(|device| device.name.clone())
                    })
                    .or_else(|| match &self.draft.audio.input_device {
                        DeviceSelection::Endpoint(endpoint) => self
                            .devices
                            .inputs
                            .iter()
                            .find(|device| device.endpoint == *endpoint)
                            .map(|device| device.name.clone()),
                        DeviceSelection::Default => None,
                    })
                    .unwrap_or_else(|| "Windows default microphone".into());
                friendly_device_name(&raw, AudioDeviceKind::Microphone).primary
            }
        }
    }

    fn current_output_name(&self) -> String {
        match &self.runtime.output {
            crate::audio::OutputState::Current { device, .. } => {
                friendly_device(device, AudioDeviceKind::Speaker).primary
            }
            crate::audio::OutputState::Unavailable { .. } => "Speakers unavailable".into(),
        }
    }
    fn device_selection_view(&self, id: ElementId) -> DeviceSelectionPresentation {
        match id {
            ElementId::InputDevice => device_selection_presentation(
                &self.draft.audio.input_device,
                &self.devices.inputs,
                self.devices
                    .input_defaults
                    .for_role(self.draft.audio.input_role),
                AudioDeviceKind::Microphone,
            ),
            ElementId::OutputDevice => device_selection_presentation(
                &self.draft.audio.output_device,
                &self.devices.outputs,
                self.devices
                    .output_defaults
                    .for_role(self.draft.audio.output_role),
                AudioDeviceKind::Speaker,
            ),
            _ => unreachable!("device selection view requested for another element"),
        }
    }

    fn special_workspace_summary(&self) -> (String, String, String) {
        if !self.draft.virtual_desktops.enabled {
            return ("Off".into(), "Workspaces are off".into(), "Enable".into());
        }
        if matches!(
            &self.runtime.desktop.native,
            crate::desktop::BackendAvailability::Available
        ) {
            (
                "Available".into(),
                "A dedicated workspace for windows kept out of the way".into(),
                "Open".into(),
            )
        } else {
            (
                "Unavailable".into(),
                "Windows workspace service is unavailable".into(),
                "Open".into(),
            )
        }
    }

    fn display_profile_needs_attention(&self) -> bool {
        if !self.draft.display_profiles.enabled {
            return true;
        }
        let Some(profile) = self.draft.display_profiles.active() else {
            return true;
        };
        if self.display_inventory_error.is_some() || !self.display_inventory_loaded {
            return true;
        }
        !profile.confirmed
            || profile.routes.is_empty()
            || profile.routes.iter().any(|route| {
                !self
                    .display_outputs
                    .iter()
                    .any(|output| crate::display::same_output(&output.route, route))
            })
    }

    fn display_summary(&self) -> (String, String) {
        if !self.draft.display_profiles.enabled {
            return (
                "Display profiles off".into(),
                "Enable display profiles to save arrangements".into(),
            );
        }
        let Some(profile) = self.draft.display_profiles.active() else {
            return (
                "No profile selected".into(),
                "Set up a display profile".into(),
            );
        };
        let missing = if self.display_inventory_error.is_some() || !self.display_inventory_loaded {
            None
        } else {
            Some(
                profile
                    .routes
                    .iter()
                    .filter(|route| {
                        !self
                            .display_outputs
                            .iter()
                            .any(|output| crate::display::same_output(&output.route, route))
                    })
                    .count(),
            )
        };
        let detail = if self.display_inventory_error.is_some() {
            "Readiness unknown · Windows display information unavailable".into()
        } else if !self.display_inventory_loaded {
            "Readiness pending · Open Displays to check connected screens".into()
        } else if missing == Some(0) && !profile.confirmed {
            "Needs a test before activation".into()
        } else if missing.is_some_and(|count| count > 0) {
            format!(
                "Needs attention · {} saved screen(s) unavailable",
                missing.unwrap_or_default()
            )
        } else if profile.routes.len() > 1 {
            format!("Ready · {} screens configured", profile.routes.len())
        } else {
            format!("Ready · {}", self.output_label(&profile.routes[0]))
        };
        (profile.name.clone(), detail)
    }
    fn home_diagnostics_copy(&self) -> (String, String, String) {
        if !self.draft.display_profiles.enabled {
            return (
                "Display profiles are off".into(),
                "Turn on Display profiles".into(),
                "Open Displays to save and switch screen arrangements.".into(),
            );
        }
        if self.display_profile_needs_attention() {
            let (_, readiness) = self.display_summary();
            let detail = if self.display_inventory_error.is_some() {
                "Windows display information is currently unavailable.".into()
            } else if !self.display_inventory_loaded {
                "Open Displays to check connected screens.".into()
            } else {
                "Open Displays to test or repair this profile.".into()
            };
            return ("Display profile needs attention".into(), readiness, detail);
        }
        if self.runtime.degraded.is_empty() {
            (
                "System status".into(),
                "WinShort services ready".into(),
                "Technical details stay in Diagnostics.".into(),
            )
        } else {
            (
                "System status".into(),
                "Some services need attention".into(),
                "Open Diagnostics for details and recovery.".into(),
            )
        }
    }

    fn shortcut_health_copy(&self) -> (String, String, String) {
        let (active, conflicts) = self.shortcut_health();
        if !self.draft.general.start_hotkeys_enabled {
            (
                "Shortcuts paused".into(),
                format!("{active} configured"),
                "Open".into(),
            )
        } else if conflicts > 0 {
            (
                format!("{conflicts} need attention"),
                format!("{active} configured"),
                "Open".into(),
            )
        } else {
            (
                format!("{active} active"),
                "No conflicts".into(),
                "Open".into(),
            )
        }
    }

    fn shortcut_health(&self) -> (usize, usize) {
        let config = &self.draft;
        let bindings = [
            config.hotkeys.toggle_microphone,
            config.hotkeys.toggle_output,
            config.hotkeys.toggle_foreground_audio,
            config.hotkeys.cycle_input_device,
            config.hotkeys.cycle_output_device,
            config.hotkeys.foreground_volume_up,
            config.hotkeys.foreground_volume_down,
            config.virtual_desktops.previous_desktop,
            config.virtual_desktops.scratchpad_assign,
            config.virtual_desktops.scratchpad_toggle,
        ];
        let explicit = bindings.iter().filter(|binding| binding.is_some()).count()
            + config.hotkeys.display_profiles.len();
        let numbered = usize::from(
            config.virtual_desktops.enabled && config.virtual_desktops.win_number_switching,
        ) * 9;
        let move_follow = usize::from(
            config.virtual_desktops.enabled
                && config.virtual_desktops.move_follow_modifier.is_some(),
        ) * 9;
        let move_silent = usize::from(
            config.virtual_desktops.enabled
                && config.virtual_desktops.move_silent_modifier.is_some(),
        ) * 9;
        let active = explicit + numbered + move_follow + move_silent;
        let conflicts = crate::config::validate(config)
            .iter()
            .filter(|violation| violation.field.contains("hotkey"))
            .count();
        (active, conflicts)
    }

    fn draw_footer(&self, renderer: &Renderer) {
        let text = if let Some(first) = self.validation.first() {
            format!("Couldn't apply change — {}", friendly_violation(first))
        } else if self.display_rollback_active {
            if self.display_keep_available {
                "Display test is active — keep it or revert before the timer ends".into()
            } else {
                "Display recovery is active — use Revert to retry".into()
            }
        } else if self.display_editor.is_some() || self.display_draft_dirty {
            "Display draft — changes aren't applied until you test and keep them".into()
        } else if self.applied_until.is_some() {
            APPLIED_STATUS.into()
        } else if self.onboarding_step.is_some() {
            "You can change these choices later".into()
        } else {
            "Changes save automatically".into()
        };
        let role = if self.validation.is_empty() {
            if self.applied_until.is_some() {
                BrushRole::Success
            } else {
                BrushRole::TextSecondary
            }
        } else {
            BrushRole::Danger
        };
        renderer.text_clipped(
            &text,
            UiRect::new(
                24.0,
                self.layout.footer.y + 8.0,
                self.layout.width - 48.0,
                22.0,
            )
            .d2d(),
            TextStyle::Caption,
            role,
        );
    }

    fn value_for(&self, id: ElementId) -> ControlValue<'_> {
        match id {
            ElementId::Search => ControlValue::Text(Cow::Owned(self.search_query.clone())),
            ElementId::Nav(page) => ControlValue::Action(Cow::Borrowed(page.label())),
            ElementId::SearchResult(index) => {
                if search(&self.search_query).get(index as usize).is_some() {
                    ControlValue::Action(Cow::Borrowed("Open"))
                } else {
                    ControlValue::Action(Cow::Borrowed(""))
                }
            }
            ElementId::WindowClose => ControlValue::Action(Cow::Borrowed("Close")),
            ElementId::HomeSpeaker => ControlValue::Text(Cow::Owned(self.current_output_name())),
            ElementId::HomeCurrentDesktop => ControlValue::Text(Cow::Owned(
                self.runtime.desktop.current_desktop.map_or_else(
                    || "Desktop status unavailable".into(),
                    |index| format!("Desktop {}", index + 1),
                ),
            )),
            ElementId::HomeMicrophone => ControlValue::Text(Cow::Owned(self.current_input_name())),
            ElementId::HomePreviousDesktop => ControlValue::Action(Cow::Borrowed("Switch")),
            ElementId::HomeSpecial => {
                ControlValue::Action(Cow::Owned(self.special_workspace_summary().2))
            }
            ElementId::HomeDisplayProfile => {
                ControlValue::Text(Cow::Owned(self.display_summary().0))
            }
            ElementId::HomeShortcutHealth => {
                ControlValue::Text(Cow::Owned(self.shortcut_health_copy().0))
            }
            ElementId::HomeDiagnostics => ControlValue::Action(Cow::Borrowed("Open")),
            ElementId::DisplayProfileCard(index) => ControlValue::Text(Cow::Owned(
                self.draft
                    .display_profiles
                    .profiles
                    .get(index as usize)
                    .map(|profile| profile.name.clone())
                    .unwrap_or_else(|| "Unavailable".into()),
            )),
            ElementId::DisplayOutputCard(index) => ControlValue::Toggle(
                self.display_output_card_data(index as usize)
                    .is_some_and(|(_, _, selected, _)| selected),
            ),
            ElementId::DisplayTopologyChoice(index) => {
                let topology = if index == 0 {
                    crate::display::DisplayTopology::Extend
                } else {
                    crate::display::DisplayTopology::Clone
                };
                ControlValue::Toggle(
                    self.draft
                        .display_profiles
                        .active()
                        .is_some_and(|profile| profile.topology == topology),
                )
            }
            ElementId::InputCycleMode(_) | ElementId::OutputCycleMode(_) => {
                ControlValue::Toggle(self.choice_selected(id))
            }
            ElementId::InputCycleDevice(index) | ElementId::OutputCycleDevice(index) => {
                ControlValue::Toggle(self.cycle_device_selected(id, index as usize))
            }
            ElementId::OverlayPositionCell(index) => ControlValue::Toggle(
                self.draft.overlay.position == overlay_position(index as usize),
            ),
            ElementId::DisplayWizardBack => ControlValue::Action(Cow::Borrowed("Back")),
            ElementId::DisplayWizardNext => ControlValue::Action(Cow::Borrowed("Next")),
            ElementId::DisplayWizardCancel => ControlValue::Action(Cow::Borrowed("Cancel")),
            ElementId::EditDisplayProfile => ControlValue::Action(Cow::Borrowed("Edit")),
            ElementId::DisplayWizardSummary => ControlValue::Text(Cow::Borrowed("")),
            ElementId::OnboardingContinue => ControlValue::Action(Cow::Borrowed("Continue")),
            ElementId::OnboardingOpen => ControlValue::Action(Cow::Borrowed("Open WinShort")),
            ElementId::StartWithWindows => ControlValue::Toggle(self.startup_enabled),
            ElementId::StartHotkeysEnabled => {
                ControlValue::Toggle(!self.draft.general.start_hotkeys_enabled)
            }
            ElementId::HotkeyCard(_) => ControlValue::Action(Cow::Borrowed("")),
            ElementId::HotkeyEnabled(slot) => {
                ControlValue::Action(Cow::Borrowed(if self.hotkey_enabled(slot) {
                    "Disable"
                } else {
                    "Enable"
                }))
            }
            ElementId::HotkeyUnassign(_) => ControlValue::Action(Cow::Borrowed("Unassign")),
            ElementId::MicHotkey
            | ElementId::OutputHotkey
            | ElementId::ForegroundHotkey
            | ElementId::CycleInputHotkey
            | ElementId::CycleOutputHotkey
            | ElementId::ForegroundVolumeUpHotkey
            | ElementId::ForegroundVolumeDownHotkey
            | ElementId::PreviousDesktopHotkey
            | ElementId::AssignScratchpadHotkey
            | ElementId::ToggleScratchpadHotkey => {
                let slot = HotkeySlot::from_capture_id(id).expect("hotkey slot");
                self.hotkey_value(id, self.configured_hotkey(slot))
            }
            ElementId::InputDevice | ElementId::OutputDevice => {
                ControlValue::Text(Cow::Owned(self.device_selection_view(id).primary))
            }
            ElementId::InputAllowlist => ControlValue::Text(Cow::Owned(allowlist_label(
                self.draft.audio.cycle_input_allowlist.as_deref(),
            ))),
            ElementId::OutputAllowlist => ControlValue::Text(Cow::Owned(allowlist_label(
                self.draft.audio.cycle_output_allowlist.as_deref(),
            ))),
            ElementId::InputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.input_role.label()))
            }
            ElementId::OutputRole => {
                ControlValue::Text(Cow::Borrowed(self.draft.audio.output_role.label()))
            }
            ElementId::DisplayProfilesEnabled => {
                ControlValue::Toggle(self.draft.display_profiles.enabled)
            }
            ElementId::DisplayProfile => ControlValue::Text(Cow::Owned(
                self.draft
                    .display_profiles
                    .active()
                    .map(|profile| profile.name.clone())
                    .unwrap_or_else(|| "No profile selected".into()),
            )),
            ElementId::DisplayProfileHotkey => {
                self.hotkey_value(id, self.configured_hotkey(HotkeySlot::DisplayProfile))
            }
            ElementId::DisplayOutputs => {
                ControlValue::Text(Cow::Owned(self.display_outputs_label()))
            }
            ElementId::DisplayTopology => ControlValue::Text(Cow::Owned(
                self.draft
                    .display_profiles
                    .active()
                    .map(|profile| {
                        if profile.routes.len() <= 1 {
                            "Single display".to_string()
                        } else {
                            profile.topology.label().to_string()
                        }
                    })
                    .unwrap_or_else(|| "No profile selected".into()),
            )),
            ElementId::DisplayRoute => {
                ControlValue::Text(Cow::Owned(self.selected_display_route_label()))
            }
            ElementId::EditDisplayRoute => ControlValue::Action(Cow::Borrowed("Edit")),
            ElementId::NewDisplayProfile => ControlValue::Action(Cow::Borrowed("New from current")),
            ElementId::UpdateDisplayProfile => {
                ControlValue::Action(Cow::Borrowed("Replace from current"))
            }
            ElementId::RenameDisplayProfile => ControlValue::Action(Cow::Borrowed("Edit name")),
            ElementId::DuplicateDisplayProfile => ControlValue::Action(Cow::Borrowed("Duplicate")),
            ElementId::TestApplyDisplayProfile => ControlValue::Action(Cow::Borrowed("Test")),
            ElementId::ApplyDisplayProfile => ControlValue::Action(Cow::Borrowed("Activate")),
            ElementId::DeleteDisplayProfile => {
                ControlValue::Action(Cow::Borrowed(if self.delete_profile_confirm {
                    "Confirm delete"
                } else {
                    "Delete"
                }))
            }
            ElementId::KeepDisplayChange => ControlValue::Action(Cow::Borrowed("Keep")),
            ElementId::UndoDisplayChange => ControlValue::Action(Cow::Borrowed("Revert")),
            ElementId::DiscardDisplayEdits => ControlValue::Action(Cow::Borrowed("Discard")),
            ElementId::DesktopsEnabled => ControlValue::Toggle(self.draft.virtual_desktops.enabled),
            ElementId::WinNumberEnabled => {
                ControlValue::Toggle(self.draft.virtual_desktops.win_number_switching)
            }
            ElementId::DesktopNumberModifier => ControlValue::Text(Cow::Owned(
                format_desktop_modifier(self.draft.virtual_desktops.number_modifier),
            )),
            ElementId::MoveDesktopModifier => ControlValue::Text(Cow::Owned(
                self.draft
                    .virtual_desktops
                    .move_follow_modifier
                    .map_or_else(|| "Not assigned".into(), format_modifier_display),
            )),
            ElementId::SilentMoveDesktopModifier => ControlValue::Text(Cow::Owned(
                self.draft
                    .virtual_desktops
                    .move_silent_modifier
                    .map_or_else(|| "Not assigned".into(), format_modifier_display),
            )),
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
            ElementId::OverlayMonitor => ControlValue::Text(Cow::Owned(
                crate::ui::presentation::monitor_choice_label(&self.draft.overlay.monitor),
            )),
            ElementId::OverlayDuration => ControlValue::Slider {
                ratio: (self.draft.overlay.duration_ms.saturating_sub(500) as f32 / 9500.0)
                    .clamp(0.0, 1.0),
                label: Cow::Borrowed(overlay_duration_label(self.draft.overlay.duration_ms)),
            },
            ElementId::OverlayOpacity => ControlValue::Slider {
                ratio: ((self.draft.overlay.opacity - 0.3) / 0.7).clamp(0.0, 1.0),
                label: Cow::Borrowed(overlay_opacity_label(self.draft.overlay.opacity)),
            },
            ElementId::OverlayScale => ControlValue::Slider {
                ratio: ((self.draft.overlay.scale - 0.7) / 0.9).clamp(0.0, 1.0),
                label: Cow::Borrowed(overlay_scale_label(self.draft.overlay.scale)),
            },
            ElementId::OverlayPreview => ControlValue::Action(Cow::Borrowed("Show on screen")),
            ElementId::DebugLogging => {
                ControlValue::Toggle(crate::diagnostics::logging::debug_logging_enabled())
            }
            ElementId::DiagnosticsStatus => ControlValue::Action(Cow::Borrowed("Open")),
            ElementId::OpenConfigFolder => ControlValue::Action(Cow::Borrowed("Open folder")),
            ElementId::ResetSettings => {
                ControlValue::Action(Cow::Borrowed(if self.reset_confirm {
                    "Confirm reset"
                } else {
                    "Reset"
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
                ControlValue::Text(Cow::Owned(format!(
                    "{} …",
                    format_modifier_display(self.recording_modifiers)
                )))
            }
        } else {
            ControlValue::Text(Cow::Owned(format_optional_hotkey(hotkey)))
        }
    }

    fn hotkey_action(&self, slot: HotkeySlot) -> Option<String> {
        Some(match slot {
            HotkeySlot::Microphone => "toggle_microphone".into(),
            HotkeySlot::Output => "toggle_output".into(),
            HotkeySlot::Foreground => "toggle_foreground_audio".into(),
            HotkeySlot::CycleInput => "cycle_input_device".into(),
            HotkeySlot::CycleOutput => "cycle_output_device".into(),
            HotkeySlot::ForegroundVolumeUp => "foreground_volume_up".into(),
            HotkeySlot::ForegroundVolumeDown => "foreground_volume_down".into(),
            HotkeySlot::PreviousDesktop => "previous_desktop".into(),
            HotkeySlot::AssignSpecial => "scratchpad_assign".into(),
            HotkeySlot::ToggleSpecial => "scratchpad_toggle".into(),
            HotkeySlot::DisplayProfile => format!(
                "display_profile:{}",
                self.draft.display_profiles.active()?.id
            ),
        })
    }
    fn hotkey_subject(slot: HotkeySlot) -> &'static str {
        match slot {
            HotkeySlot::Microphone => "Mute microphone shortcut",
            HotkeySlot::Output => "Mute speakers shortcut",
            HotkeySlot::Foreground => "Mute current app shortcut",
            HotkeySlot::CycleInput => "Next microphone shortcut",
            HotkeySlot::CycleOutput => "Next speaker shortcut",
            HotkeySlot::ForegroundVolumeUp => "Current app volume up shortcut",
            HotkeySlot::ForegroundVolumeDown => "Current app volume down shortcut",
            HotkeySlot::PreviousDesktop => "Previous desktop shortcut",
            HotkeySlot::AssignSpecial => "Move window to Special shortcut",
            HotkeySlot::ToggleSpecial => "Open or close Special shortcut",
            HotkeySlot::DisplayProfile => "Selected display profile shortcut",
        }
    }

    fn active_hotkey(&self, slot: HotkeySlot) -> Option<Hotkey> {
        match slot {
            HotkeySlot::Microphone => self.draft.hotkeys.toggle_microphone,
            HotkeySlot::Output => self.draft.hotkeys.toggle_output,
            HotkeySlot::Foreground => self.draft.hotkeys.toggle_foreground_audio,
            HotkeySlot::CycleInput => self.draft.hotkeys.cycle_input_device,
            HotkeySlot::CycleOutput => self.draft.hotkeys.cycle_output_device,
            HotkeySlot::ForegroundVolumeUp => self.draft.hotkeys.foreground_volume_up,
            HotkeySlot::ForegroundVolumeDown => self.draft.hotkeys.foreground_volume_down,
            HotkeySlot::PreviousDesktop => self.draft.virtual_desktops.previous_desktop,
            HotkeySlot::AssignSpecial => self.draft.virtual_desktops.scratchpad_assign,
            HotkeySlot::ToggleSpecial => self.draft.virtual_desktops.scratchpad_toggle,
            HotkeySlot::DisplayProfile => self.active_profile_hotkey(),
        }
    }

    fn configured_hotkey(&self, slot: HotkeySlot) -> Option<Hotkey> {
        self.active_hotkey(slot).or_else(|| {
            self.hotkey_action(slot)
                .as_deref()
                .and_then(|action| self.draft.hotkeys.disabled_hotkey(action))
        })
    }

    fn hotkey_enabled(&self, slot: HotkeySlot) -> bool {
        self.active_hotkey(slot).is_some()
    }

    fn set_active_hotkey(&mut self, slot: HotkeySlot, hotkey: Option<Hotkey>) {
        match slot {
            HotkeySlot::Microphone => self.draft.hotkeys.toggle_microphone = hotkey,
            HotkeySlot::Output => self.draft.hotkeys.toggle_output = hotkey,
            HotkeySlot::Foreground => self.draft.hotkeys.toggle_foreground_audio = hotkey,
            HotkeySlot::CycleInput => self.draft.hotkeys.cycle_input_device = hotkey,
            HotkeySlot::CycleOutput => self.draft.hotkeys.cycle_output_device = hotkey,
            HotkeySlot::ForegroundVolumeUp => self.draft.hotkeys.foreground_volume_up = hotkey,
            HotkeySlot::ForegroundVolumeDown => self.draft.hotkeys.foreground_volume_down = hotkey,
            HotkeySlot::PreviousDesktop => self.draft.virtual_desktops.previous_desktop = hotkey,
            HotkeySlot::AssignSpecial => self.draft.virtual_desktops.scratchpad_assign = hotkey,
            HotkeySlot::ToggleSpecial => self.draft.virtual_desktops.scratchpad_toggle = hotkey,
            HotkeySlot::DisplayProfile => self.set_active_profile_hotkey(hotkey),
        }
    }

    fn set_recorded_hotkey(&mut self, slot: HotkeySlot, hotkey: Hotkey) {
        let Some(action) = self.hotkey_action(slot) else {
            return;
        };
        if self.active_hotkey(slot).is_none()
            && self.draft.hotkeys.disabled_hotkey(&action).is_some()
        {
            self.draft.hotkeys.set_disabled_hotkey(action, hotkey);
        } else {
            self.set_active_hotkey(slot, Some(hotkey));
            self.draft.hotkeys.clear_disabled_hotkey(&action);
        }
    }

    fn toggle_hotkey_enabled(&mut self, hwnd: HWND, slot: HotkeySlot) {
        let Some(action) = self.hotkey_action(slot) else {
            return;
        };
        let before = self.draft.clone();
        if let Some(hotkey) = self.active_hotkey(slot) {
            self.set_active_hotkey(slot, None);
            self.draft.hotkeys.clear_disabled_hotkey(&action);
            self.draft.hotkeys.set_disabled_hotkey(action, hotkey);
        } else if let Some(hotkey) = self.draft.hotkeys.take_disabled_hotkey(&action) {
            self.set_active_hotkey(slot, Some(hotkey));
            let violations = crate::config::validate(&self.draft);
            if !violations.is_empty() {
                self.replace_draft(before);
                self.validation = violations;
                return;
            }
        } else {
            return;
        }
        self.commit_local_change(hwnd, before);
    }

    fn unassign_hotkey(&mut self, hwnd: HWND, slot: HotkeySlot) {
        let Some(action) = self.hotkey_action(slot) else {
            return;
        };
        let before = self.draft.clone();
        self.set_active_hotkey(slot, None);
        self.draft.hotkeys.clear_disabled_hotkey(&action);
        if self.draft != before {
            self.commit_local_change(hwnd, before);
        }
    }

    fn choice_selected(&self, id: ElementId) -> bool {
        let (mode, index) = match id {
            ElementId::InputCycleMode(index) => (
                allowlist_mode(self.draft.audio.cycle_input_allowlist.as_deref()),
                index,
            ),
            ElementId::OutputCycleMode(index) => (
                allowlist_mode(self.draft.audio.cycle_output_allowlist.as_deref()),
                index,
            ),
            _ => return false,
        };
        mode == allowlist_mode_for_index(index)
    }

    fn cycle_device_selected(&self, id: ElementId, index: usize) -> bool {
        let (devices, configured) = match id {
            ElementId::InputCycleDevice(_) => (
                &self.devices.inputs,
                self.draft.audio.cycle_input_allowlist.as_deref(),
            ),
            ElementId::OutputCycleDevice(_) => (
                &self.devices.outputs,
                self.draft.audio.cycle_output_allowlist.as_deref(),
            ),
            _ => return false,
        };
        let Some(endpoint) = devices.get(index).map(|device| device.endpoint.as_str()) else {
            return false;
        };
        configured.is_some_and(|values| values.iter().any(|value| value == endpoint))
    }

    fn cycle_device_label(&self, id: ElementId, index: usize) -> String {
        match id {
            ElementId::InputCycleDevice(_) => crate::ui::presentation::device_choice_label_at(
                &self.devices.inputs,
                index,
                self.devices
                    .input_defaults
                    .for_role(self.draft.audio.input_role),
                AudioDeviceKind::Microphone,
            ),
            ElementId::OutputCycleDevice(_) => crate::ui::presentation::device_choice_label_at(
                &self.devices.outputs,
                index,
                self.devices
                    .output_defaults
                    .for_role(self.draft.audio.output_role),
                AudioDeviceKind::Speaker,
            ),
            _ => None,
        }
        .unwrap_or_else(|| "Device unavailable".into())
    }

    fn interaction(&self, id: ElementId, disabled: bool) -> Interaction {
        let toggle_value = match id {
            ElementId::StartWithWindows => self.startup_enabled,
            ElementId::StartHotkeysEnabled => !self.draft.general.start_hotkeys_enabled,
            ElementId::DesktopsEnabled => self.draft.virtual_desktops.enabled,
            ElementId::WinNumberEnabled => self.draft.virtual_desktops.win_number_switching,
            ElementId::DisplayProfilesEnabled => self.draft.display_profiles.enabled,
            ElementId::OverlayEnabled => self.draft.overlay.enabled,
            ElementId::OverlayExternalChanges => self.draft.overlay.show_external_audio_changes,
            ElementId::DebugLogging => crate::diagnostics::logging::debug_logging_enabled(),
            _ => false,
        };
        Interaction {
            hovered: self.hovered == Some(id),
            pressed: self.pressed == Some(id),
            focused: self.visual_focus(id),
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
            ElementId::HotkeyEnabled(slot) | ElementId::HotkeyUnassign(slot) => {
                self.configured_hotkey(slot).is_none()
                    || (matches!(
                        slot,
                        HotkeySlot::PreviousDesktop
                            | HotkeySlot::AssignSpecial
                            | HotkeySlot::ToggleSpecial
                    ) && !self.draft.virtual_desktops.enabled)
                    || (slot == HotkeySlot::DisplayProfile
                        && self.draft.display_profiles.active().is_none())
            }
            ElementId::HotkeyCard(_) => false,
            ElementId::WinNumberEnabled => !self.draft.virtual_desktops.enabled,
            ElementId::DesktopNumberModifier => {
                !self.draft.virtual_desktops.enabled
                    || !self.draft.virtual_desktops.win_number_switching
            }
            ElementId::MoveDesktopModifier
            | ElementId::SilentMoveDesktopModifier
            | ElementId::PreviousDesktopHotkey
            | ElementId::AssignScratchpadHotkey
            | ElementId::ToggleScratchpadHotkey => !self.draft.virtual_desktops.enabled,
            ElementId::DisplayProfile => {
                !self.draft.display_profiles.enabled
                    || self.draft.display_profiles.active().is_none()
                    || self.display_draft_dirty
            }
            ElementId::DisplayProfileHotkey
            | ElementId::DisplayOutputs
            | ElementId::DisplayRoute
            | ElementId::EditDisplayRoute => {
                !self.draft.display_profiles.enabled
                    || self.draft.display_profiles.active().is_none()
                    || self.display_rollback_active
            }
            ElementId::DisplayTopology => {
                !self.draft.display_profiles.enabled
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.len() <= 1)
                    || self.display_rollback_active
            }
            ElementId::NewDisplayProfile => {
                !self.draft.display_profiles.enabled
                    || self.display_rollback_active
                    || self.display_editor.is_some()
            }
            ElementId::EditDisplayProfile => {
                !self.draft.display_profiles.enabled
                    || self.draft.display_profiles.active().is_none()
                    || self.display_rollback_active
            }
            ElementId::UpdateDisplayProfile
            | ElementId::DuplicateDisplayProfile
            | ElementId::DeleteDisplayProfile => {
                !self.draft.display_profiles.enabled
                    || self.draft.display_profiles.active().is_none()
                    || self.display_rollback_active
                    || self.display_editor.is_some()
            }
            ElementId::RenameDisplayProfile => {
                !self.draft.display_profiles.enabled
                    || self.draft.display_profiles.active().is_none()
                    || self.display_rollback_active
            }
            ElementId::TestApplyDisplayProfile => {
                !self.draft.display_profiles.enabled
                    || self.draft.display_profiles.active().is_none()
                    || self.display_rollback_active
                    || self.display_editor.is_none()
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.is_empty())
            }
            ElementId::ApplyDisplayProfile => {
                !self.draft.display_profiles.enabled
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| !profile.confirmed)
                    || self.display_rollback_active
            }
            ElementId::DisplayProfileCard(index) => {
                index as usize >= self.draft.display_profiles.profiles.len()
                    || !self.draft.display_profiles.enabled
                    || self.display_rollback_active
                    || self.display_editor.is_some()
            }
            ElementId::DisplayOutputCard(index) => {
                self.display_editor.is_none()
                    || self.display_rollback_active
                    || index as usize >= self.display_route_candidates().len()
            }
            ElementId::DisplayTopologyChoice(_) => {
                self.display_editor.is_none()
                    || self.display_rollback_active
                    || self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.len() <= 1)
            }
            ElementId::DisplayWizardNext => {
                self.display_editor.is_none_or(|editor| match editor.step {
                    DisplayWizardStep::Displays => self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.routes.is_empty()),
                    DisplayWizardStep::Arrangement => false,
                    DisplayWizardStep::NameAndShortcut => self
                        .draft
                        .display_profiles
                        .active()
                        .is_none_or(|profile| profile.name.trim().is_empty()),
                    DisplayWizardStep::Review => true,
                })
            }
            ElementId::DisplayWizardBack | ElementId::DisplayWizardCancel => {
                self.display_editor.is_none()
            }
            ElementId::DisplayProfilesEnabled => self.display_draft_dirty,
            ElementId::KeepDisplayChange => !self.display_keep_available,
            ElementId::UndoDisplayChange => !self.display_rollback_active,
            ElementId::DiscardDisplayEdits => !self.display_draft_dirty,
            ElementId::InputRole => !Self::endpoint_role_enabled(&self.draft.audio.input_device),
            ElementId::OutputRole => !Self::endpoint_role_enabled(&self.draft.audio.output_device),
            ElementId::OverlayAppearance => false,
            ElementId::OverlayExternalChanges
            | ElementId::OverlayPosition
            | ElementId::OverlayMonitor
            | ElementId::OverlayPositionCell(_)
            | ElementId::OverlayDuration
            | ElementId::OverlayOpacity
            | ElementId::OverlayScale
            | ElementId::OverlayPreview => !self.draft.overlay.enabled,
            ElementId::Search
            | ElementId::Nav(_)
            | ElementId::SearchResult(_)
            | ElementId::HomeCurrentDesktop
            | ElementId::HomePreviousDesktop
            | ElementId::HomeSpecial
            | ElementId::HomeDisplayProfile
            | ElementId::HomeShortcutHealth
            | ElementId::HomeDiagnostics
            | ElementId::OnboardingContinue
            | ElementId::OnboardingOpen
            | ElementId::DisplayWizardSummary
            | ElementId::InputAllowlist
            | ElementId::OutputAllowlist
            | ElementId::InputCycleMode(_)
            | ElementId::OutputCycleMode(_)
            | ElementId::InputCycleDevice(_)
            | ElementId::OutputCycleDevice(_)
            | ElementId::DebugLogging
            | ElementId::DiagnosticsStatus
            | ElementId::OpenConfigFolder
            | ElementId::ResetSettings => false,
            ElementId::Save => !self.dirty(),
            ElementId::HomeSpeaker => {
                matches!(
                    &self.runtime.output,
                    crate::audio::OutputState::Unavailable { .. }
                )
            }
            ElementId::HomeMicrophone => {
                matches!(
                    &self.runtime.microphone,
                    crate::audio::AudioState::Unavailable { .. }
                )
            }
            _ => false,
        }
    }

    fn set_hover(&mut self, hwnd: HWND, next: Option<ElementId>) {
        if next == self.hovered {
            return;
        }
        if !SystemVisualPreferences::query().animations_enabled {
            self.hovered = next;
            self.motion.clear_channel(MotionChannel::Hover);
            invalidate(hwnd);
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
        let track = controls::slider_track_rect(element.rect);
        let ratio = (x - track.x) / track.w;
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
        let before = self.draft.clone();
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
        if self.draft != before {
            self.commit_local_change(hwnd, before);
        }
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
        if id != ElementId::DeleteDisplayProfile {
            self.delete_profile_confirm = false;
        }
        match id {
            ElementId::WindowClose => unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(hwnd),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            },
            ElementId::Nav(page) => {
                self.set_page(page);
                self.focused = Some(ElementId::Nav(page));
                if page == Page::Displays {
                    self.refresh_display_outputs();
                }
            }
            ElementId::Search => self.focused = Some(ElementId::Search),
            ElementId::SearchResult(index) => {
                if let Some(result) = search(&self.search_query).get(index as usize) {
                    let page = result.item.page;
                    let target = result.item.target;
                    self.set_page(page);
                    self.focused = Some(target);
                    if page == Page::Displays {
                        self.refresh_display_outputs();
                    }
                }
            }
            ElementId::HomeSpeaker => {
                self.set_page(Page::Audio);
                self.focused = Some(ElementId::OutputDevice);
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::OutputDevice,
                ));
            }
            ElementId::HomeMicrophone => {
                self.set_page(Page::Audio);
                self.focused = Some(ElementId::InputDevice);
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::InputDevice,
                ));
            }
            ElementId::HomeCurrentDesktop => self.set_page(Page::Workspaces),
            ElementId::HomePreviousDesktop => {
                post_main(crate::event::AppEvent::SwitchPreviousDesktopFromUi);
            }
            ElementId::HomeSpecial => {
                if !self.draft.virtual_desktops.enabled {
                    self.activate(hwnd, ElementId::DesktopsEnabled);
                } else if matches!(
                    &self.runtime.desktop.native,
                    crate::desktop::BackendAvailability::Available
                ) {
                    post_main(crate::event::AppEvent::ToggleSpecialWorkspaceFromUi);
                } else {
                    self.set_page(Page::Workspaces);
                }
            }
            ElementId::HomeDisplayProfile => {
                self.set_page(Page::Displays);
                self.refresh_display_outputs();
            }
            ElementId::HomeShortcutHealth => self.set_page(Page::Shortcuts),
            ElementId::HomeDiagnostics => post_main(crate::event::AppEvent::ShowDiagnostics),
            ElementId::DisplayProfileCard(index) => {
                if let Some(profile) = self.draft.display_profiles.profiles.get(index as usize) {
                    let id = profile.id.clone();
                    let ready = self.display_inventory_loaded
                        && self.display_inventory_error.is_none()
                        && profile.confirmed
                        && !profile.routes.is_empty()
                        && profile.routes.iter().all(|route| {
                            self.display_outputs
                                .iter()
                                .any(|output| crate::display::same_output(&output.route, route))
                        });
                    let already_selected = self
                        .draft
                        .display_profiles
                        .active_profile
                        .as_deref()
                        .is_some_and(|active| active.eq_ignore_ascii_case(&id));
                    if already_selected && ready {
                        post_main(crate::event::AppEvent::ApplyDisplayProfile {
                            profile: profile.clone(),
                        });
                    } else if already_selected {
                        self.display_editor = Some(DisplayEditorState {
                            step: DisplayWizardStep::Review,
                        });
                        self.display_draft_dirty = false;
                    } else {
                        let before = self.draft.clone();
                        self.draft.display_profiles.active_profile = Some(id);
                        self.selected_display_route = 0;
                        self.commit_local_change(hwnd, before);
                    }
                }
            }
            ElementId::DisplayOutputCard(index) => self.toggle_display_output(index),
            ElementId::DisplayTopologyChoice(index) => {
                self.set_display_topology(index);
            }
            ElementId::InputCycleMode(index) | ElementId::OutputCycleMode(index) => {
                self.set_cycle_mode(hwnd, id, index);
            }
            ElementId::InputCycleDevice(index) | ElementId::OutputCycleDevice(index) => {
                self.toggle_cycle_device(hwnd, id, index as usize);
            }
            ElementId::OverlayPositionCell(index) => {
                self.set_overlay_position(hwnd, index as usize);
            }
            ElementId::DisplayWizardSummary => {}
            ElementId::DisplayWizardBack => self.move_display_editor(hwnd, false),
            ElementId::DisplayWizardNext => self.move_display_editor(hwnd, true),
            ElementId::DisplayWizardCancel => self.cancel_display_editor(),
            ElementId::OnboardingContinue => {
                self.onboarding_step = Some(2);
                self.reset_scroll();
            }
            ElementId::OnboardingOpen => {
                if crate::ui::first_run::mark_completed(&crate::config::data_dir()).is_err() {
                    crate::warn_!("could not persist onboarding completion marker");
                }
                self.onboarding_step = None;
                self.set_page(Page::Home);
            }
            ElementId::StartWithWindows => {
                let enable = !self.startup_enabled;
                if let Err(error) = crate::platform::startup::set_enabled(enable) {
                    self.validation = vec![Violation {
                        field: "Startup".into(),
                        message: error.to_string(),
                    }];
                } else {
                    self.startup_enabled = enable;
                    self.applied_until = Some(Instant::now() + Duration::from_secs(2));
                    start_timer(hwnd);
                }
            }
            ElementId::StartHotkeysEnabled => {
                let before = self.draft.clone();
                self.draft.general.start_hotkeys_enabled =
                    !self.draft.general.start_hotkeys_enabled;
                if self.commit_local_change(hwnd, before) {
                    self.animate_toggle(hwnd, id, !self.draft.general.start_hotkeys_enabled);
                }
            }
            ElementId::HotkeyCard(_) => {}
            ElementId::HotkeyEnabled(slot) => self.toggle_hotkey_enabled(hwnd, slot),
            ElementId::HotkeyUnassign(slot) => self.unassign_hotkey(hwnd, slot),
            ElementId::MicHotkey
            | ElementId::OutputHotkey
            | ElementId::ForegroundHotkey
            | ElementId::CycleInputHotkey
            | ElementId::CycleOutputHotkey
            | ElementId::ForegroundVolumeUpHotkey
            | ElementId::ForegroundVolumeDownHotkey
            | ElementId::PreviousDesktopHotkey
            | ElementId::AssignScratchpadHotkey
            | ElementId::ToggleScratchpadHotkey
            | ElementId::DisplayProfileHotkey => {
                self.recording = Some(id);
                self.recording_modifiers = ModifierMask::NONE;
                self.validation.clear();
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
            ElementId::InputAllowlist => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::InputAllowlist,
                ));
            }
            ElementId::OutputAllowlist => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::OutputAllowlist,
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
            ElementId::DisplayProfilesEnabled => {
                let before = self.draft.clone();
                self.draft.display_profiles.enabled = !self.draft.display_profiles.enabled;
                if self.commit_local_change(hwnd, before) {
                    self.animate_toggle(hwnd, id, self.draft.display_profiles.enabled);
                }
            }
            ElementId::DisplayProfile => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::DisplayProfile,
                ));
            }
            ElementId::DisplayOutputs => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::DisplayOutputs,
                ));
            }
            ElementId::DisplayTopology => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::DisplayTopology,
                ));
            }
            ElementId::DisplayRoute => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::DisplayRoute,
                ));
            }
            ElementId::EditDisplayRoute => {
                if let (Some(profile), Some(initial)) = (
                    self.draft.display_profiles.active(),
                    self.selected_display_route_edit_value(),
                ) {
                    post_main(crate::event::AppEvent::OpenDisplayRouteEditPrompt {
                        profile_id: profile.id.clone(),
                        route_index: self.selected_display_route,
                        initial,
                    });
                }
            }
            ElementId::NewDisplayProfile | ElementId::UpdateDisplayProfile => {
                self.start_display_editor(hwnd, id == ElementId::UpdateDisplayProfile);
            }
            ElementId::EditDisplayProfile => {
                self.start_existing_display_editor();
            }
            ElementId::RenameDisplayProfile => {
                if let Some(profile) = self.draft.display_profiles.active() {
                    post_main(crate::event::AppEvent::OpenDisplayRenamePrompt {
                        profile_id: profile.id.clone(),
                        current_name: profile.name.clone(),
                    });
                }
            }
            ElementId::DuplicateDisplayProfile => {
                let before = self.draft.clone();
                self.duplicate_active_display_profile();
                if self.draft != before {
                    self.commit_local_change(hwnd, before);
                }
            }
            ElementId::TestApplyDisplayProfile => {
                if let Some(profile) = self.draft.display_profiles.active().cloned() {
                    post_main(crate::event::AppEvent::TestApplyDisplayProfile { profile });
                }
            }
            ElementId::ApplyDisplayProfile => {
                if let Some(profile) = self.draft.display_profiles.active().cloned() {
                    post_main(crate::event::AppEvent::ApplyDisplayProfile { profile });
                }
            }
            ElementId::DeleteDisplayProfile => {
                if Self::consume_reset_confirmation(&mut self.delete_profile_confirm) {
                    let before = self.draft.clone();
                    self.delete_active_display_profile();
                    if self.draft != before {
                        self.commit_local_change(hwnd, before);
                    }
                }
            }
            ElementId::KeepDisplayChange => post_main(crate::event::AppEvent::KeepDisplayProfile),
            ElementId::UndoDisplayChange => post_main(crate::event::AppEvent::RevertDisplayProfile),
            ElementId::DiscardDisplayEdits => {
                self.cancel_display_editor();
                self.validation.clear();
            }
            ElementId::DesktopsEnabled => {
                let before = self.draft.clone();
                self.draft.virtual_desktops.enabled = !self.draft.virtual_desktops.enabled;
                if self.commit_local_change(hwnd, before) {
                    self.animate_toggle(hwnd, id, self.draft.virtual_desktops.enabled);
                }
            }
            ElementId::WinNumberEnabled => {
                let before = self.draft.clone();
                self.draft.virtual_desktops.win_number_switching =
                    !self.draft.virtual_desktops.win_number_switching;
                if self.commit_local_change(hwnd, before) {
                    self.animate_toggle(hwnd, id, self.draft.virtual_desktops.win_number_switching);
                }
            }
            ElementId::DesktopNumberModifier => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::DesktopNumberModifier,
                ));
            }
            ElementId::MoveDesktopModifier => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::MoveDesktopModifier,
                ));
            }
            ElementId::SilentMoveDesktopModifier => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::SilentMoveDesktopModifier,
                ));
            }
            ElementId::OverlayEnabled => {
                let before = self.draft.clone();
                self.draft.overlay.enabled = !self.draft.overlay.enabled;
                if self.commit_local_change(hwnd, before) {
                    self.animate_toggle(hwnd, id, self.draft.overlay.enabled);
                }
            }
            ElementId::OverlayAppearance => {
                post_main(crate::event::AppEvent::OpenSettingsPicker(
                    PickerKind::OverlayAppearance,
                ));
            }
            ElementId::OverlayExternalChanges => {
                let before = self.draft.clone();
                self.draft.overlay.show_external_audio_changes =
                    !self.draft.overlay.show_external_audio_changes;
                if self.commit_local_change(hwnd, before) {
                    self.animate_toggle(hwnd, id, self.draft.overlay.show_external_audio_changes);
                }
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
                self.applied_until = Some(Instant::now() + Duration::from_secs(2));
                start_timer(hwnd);
                self.animate_toggle(hwnd, id, enabled);
            }
            ElementId::OverlayPreview => post_main(crate::event::AppEvent::PreviewOverlay {
                config: self.draft.overlay.clone(),
            }),
            ElementId::DiagnosticsStatus => post_main(crate::event::AppEvent::ShowDiagnostics),
            ElementId::OpenConfigFolder => open_config_folder(),
            ElementId::ResetSettings => {
                if Self::consume_reset_confirmation(&mut self.reset_confirm) {
                    let before = self.draft.clone();
                    self.replace_draft(Config::default());
                    self.commit_local_change(hwnd, before);
                }
            }
            ElementId::Cancel => {
                self.replace_draft((*crate::app::config()).clone());
                self.validation.clear();
                self.recording = None;
                self.stop_capture();
            }
            ElementId::Save => {}
            ElementId::OverlayDuration | ElementId::OverlayOpacity | ElementId::OverlayScale => {}
        }
        self.rebuild_layout(hwnd);
        invalidate(hwnd);
        self.publish_automation_snapshot(hwnd);
    }
    fn start_display_editor(&mut self, _hwnd: HWND, update_selected: bool) {
        if self.display_editor.is_some() {
            return;
        }
        self.refresh_display_outputs();
        let before = self.draft.clone();
        self.capture_display_profile(_hwnd, update_selected);
        if self.draft != before {
            self.display_editor = Some(DisplayEditorState {
                step: DisplayWizardStep::Displays,
            });
            self.display_draft_dirty = true;
            self.selected_display_route = 0;
            self.validation.clear();
        }
    }

    fn start_existing_display_editor(&mut self) {
        if self
            .draft
            .display_profiles
            .active()
            .is_some_and(|profile| !profile.routes.is_empty())
        {
            self.display_editor = Some(DisplayEditorState {
                step: DisplayWizardStep::Displays,
            });
            self.display_draft_dirty = false;
            self.selected_display_route = 0;
            self.validation.clear();
        }
    }

    fn move_display_editor(&mut self, _hwnd: HWND, forward: bool) {
        let Some(editor) = self.display_editor.as_mut() else {
            return;
        };
        if forward && editor.step == DisplayWizardStep::Displays {
            let has_route = self
                .draft
                .display_profiles
                .active()
                .is_some_and(|profile| !profile.routes.is_empty());
            if !has_route {
                self.validation = vec![Violation {
                    field: "display_profiles.routes".into(),
                    message: "select at least one screen before continuing".into(),
                }];
                return;
            }
        }
        let next = if forward {
            editor.step.next()
        } else {
            editor.step.previous()
        };
        if let Some(step) = next {
            editor.step = step;
            self.reset_scroll();
            self.validation.clear();
        }
    }

    fn cancel_display_editor(&mut self) {
        if self.display_editor.is_some() {
            self.replace_draft((*crate::app::config()).clone());
            self.validation.clear();
        }
    }

    fn toggle_display_output(&mut self, index: u8) {
        let Some((_, _, route, _)) = self
            .display_route_candidates()
            .into_iter()
            .nth(index as usize)
        else {
            return;
        };
        let Some(profile) = self.draft.display_profiles.active() else {
            return;
        };
        let profile_id = profile.id.clone();
        if let Some(profile) = self
            .draft
            .display_profiles
            .profiles
            .iter_mut()
            .find(|profile| profile.id.eq_ignore_ascii_case(&profile_id))
        {
            if let Some(existing) = profile
                .routes
                .iter()
                .position(|configured| crate::display::same_output(configured, &route))
            {
                profile.routes.remove(existing);
            } else {
                profile.routes.push(route);
            }
            profile.confirmed = false;
            if profile.routes.len() <= 1 {
                profile.topology = crate::display::DisplayTopology::Custom;
            } else if !matches!(
                profile.topology,
                crate::display::DisplayTopology::Extend | crate::display::DisplayTopology::Clone
            ) {
                profile.topology = crate::display::DisplayTopology::Extend;
            }
            self.selected_display_route = self
                .selected_display_route
                .min(profile.routes.len().saturating_sub(1));
            self.display_draft_dirty = true;
            self.validation.clear();
        }
    }

    fn set_display_topology(&mut self, index: u8) {
        let topology = if index == 0 {
            crate::display::DisplayTopology::Extend
        } else {
            crate::display::DisplayTopology::Clone
        };
        if let Some(profile) = self
            .draft
            .display_profiles
            .active_profile
            .as_deref()
            .and_then(|id| {
                self.draft
                    .display_profiles
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id.eq_ignore_ascii_case(id))
            })
        {
            if profile.routes.len() > 1 {
                profile.topology = topology;
                profile.confirmed = false;
                self.display_draft_dirty = true;
                self.validation.clear();
            }
        }
    }

    fn set_cycle_mode(&mut self, hwnd: HWND, id: ElementId, index: u8) {
        let mode = allowlist_mode_for_index(index);
        let (allowlist, devices) = match (id, mode) {
            (ElementId::InputCycleMode(_), AllowlistMode::All) => (None, &self.devices.inputs),
            (ElementId::InputCycleMode(_), AllowlistMode::Selected) => (
                Some(
                    self.draft
                        .audio
                        .cycle_input_allowlist
                        .clone()
                        .unwrap_or_else(|| {
                            self.devices
                                .inputs
                                .iter()
                                .map(|device| device.endpoint.clone())
                                .collect()
                        }),
                ),
                &self.devices.inputs,
            ),
            (ElementId::InputCycleMode(_), AllowlistMode::Disabled) => {
                (Some(Vec::new()), &self.devices.inputs)
            }
            (ElementId::OutputCycleMode(_), AllowlistMode::All) => (None, &self.devices.outputs),
            (ElementId::OutputCycleMode(_), AllowlistMode::Selected) => (
                Some(
                    self.draft
                        .audio
                        .cycle_output_allowlist
                        .clone()
                        .unwrap_or_else(|| {
                            self.devices
                                .outputs
                                .iter()
                                .map(|device| device.endpoint.clone())
                                .collect()
                        }),
                ),
                &self.devices.outputs,
            ),
            (ElementId::OutputCycleMode(_), AllowlistMode::Disabled) => {
                (Some(Vec::new()), &self.devices.outputs)
            }
            _ => return,
        };
        if matches!(mode, AllowlistMode::Selected) && devices.is_empty() {
            self.validation = vec![Violation {
                field: "Audio".into(),
                message: "No devices are available for a selected cycling list".into(),
            }];
            return;
        }
        let before = self.draft.clone();
        match id {
            ElementId::InputCycleMode(_) => self.draft.audio.cycle_input_allowlist = allowlist,
            ElementId::OutputCycleMode(_) => self.draft.audio.cycle_output_allowlist = allowlist,
            _ => return,
        }
        self.commit_local_change(hwnd, before);
    }

    fn toggle_cycle_device(&mut self, hwnd: HWND, id: ElementId, index: usize) {
        let before = self.draft.clone();
        let (devices, allowlist) = match id {
            ElementId::InputCycleDevice(_) => (
                &self.devices.inputs,
                &mut self.draft.audio.cycle_input_allowlist,
            ),
            ElementId::OutputCycleDevice(_) => (
                &self.devices.outputs,
                &mut self.draft.audio.cycle_output_allowlist,
            ),
            _ => return,
        };
        let Some(endpoint) = devices.get(index).map(|device| device.endpoint.clone()) else {
            return;
        };
        let values = allowlist.get_or_insert_with(Vec::new);
        if let Some(position) = values.iter().position(|value| value == &endpoint) {
            values.remove(position);
        } else {
            values.push(endpoint);
        }
        self.commit_local_change(hwnd, before);
    }

    fn set_overlay_position(&mut self, hwnd: HWND, index: usize) {
        let position = overlay_position(index);
        let before = self.draft.clone();
        self.draft.overlay.position = position;
        self.commit_local_change(hwnd, before);
    }

    fn capture_display_profile(&mut self, _hwnd: HWND, update_selected: bool) {
        let (id, name) = if update_selected {
            let Some(profile) = self.draft.display_profiles.active() else {
                self.validation = vec![Violation {
                    field: "display_profiles.active_profile".into(),
                    message: "select a profile before Update from Current".into(),
                }];
                return;
            };
            (profile.id.clone(), profile.name.clone())
        } else {
            next_display_profile_identity(&self.draft.display_profiles.profiles)
        };
        match crate::display::capture_current_profile(&id, &name) {
            Ok(mut profile) => {
                profile.confirmed = false;
                if self.draft.display_profiles.upsert(profile) {
                    self.selected_display_route = 0;
                    self.validation.clear();
                } else {
                    self.validation = vec![Violation {
                        field: "display_profiles.profiles".into(),
                        message: format!(
                            "at most {} display profiles are supported",
                            crate::display::MAX_PROFILES
                        ),
                    }];
                }
            }
            Err(error) => {
                let reason = error.to_string();
                crate::error_!("display profile capture failed: {reason}");
                self.validation = vec![Violation {
                    field: "display_profiles".into(),
                    message: reason,
                }];
            }
        }
    }

    fn duplicate_active_display_profile(&mut self) {
        let Some(source) = self.draft.display_profiles.active().cloned() else {
            return;
        };
        let (id, _) = next_display_profile_identity(&self.draft.display_profiles.profiles);
        let mut name = format!("{} Copy", source.name.trim());
        let base = name.clone();
        let mut suffix = 2usize;
        while self
            .draft
            .display_profiles
            .profiles
            .iter()
            .any(|profile| profile.name.eq_ignore_ascii_case(&name))
        {
            name = format!("{base} {suffix}");
            suffix += 1;
        }
        let mut duplicate = source;
        duplicate.id = id;
        duplicate.name = name;
        duplicate.confirmed = false;
        if self.draft.display_profiles.upsert(duplicate) {
            self.selected_display_route = 0;
            self.validation.clear();
        } else {
            self.validation = vec![Violation {
                field: "display_profiles.profiles".into(),
                message: format!(
                    "at most {} display profiles are supported",
                    crate::display::MAX_PROFILES
                ),
            }];
        }
    }

    fn rename_display_profile(&mut self, profile_id: &str, name: &str) {
        let normalized = name.trim();
        if normalized.is_empty() {
            self.validation = vec![Violation {
                field: "display_profiles.profiles.name".into(),
                message: "profile name must not be empty".into(),
            }];
            return;
        }
        if self.draft.display_profiles.profiles.iter().any(|profile| {
            !profile.id.eq_ignore_ascii_case(profile_id)
                && profile.name.eq_ignore_ascii_case(normalized)
        }) {
            self.validation = vec![Violation {
                field: "display_profiles.profiles.name".into(),
                message: format!("profile name `{normalized}` is already in use"),
            }];
            return;
        }
        let Some(profile) = self
            .draft
            .display_profiles
            .profiles
            .iter_mut()
            .find(|profile| profile.id.eq_ignore_ascii_case(profile_id))
        else {
            self.validation = vec![Violation {
                field: "display_profiles.active_profile".into(),
                message: "selected profile no longer exists".into(),
            }];
            return;
        };
        profile.name = normalized.to_string();
        if self.display_editor.is_some() {
            self.display_draft_dirty = true;
        }
        self.validation.clear();
    }
    fn edit_display_route(&mut self, profile_id: &str, route_index: usize, value: &str) {
        let (x, y, width, height, refresh, refresh_denominator, rotation) =
            match parse_display_route_values(value) {
                Ok(values) => values,
                Err(error) => {
                    self.validation = vec![Violation {
                        field: "display_profiles.routes".into(),
                        message: error.into(),
                    }];
                    return;
                }
            };

        let Some(profile) = self
            .draft
            .display_profiles
            .profiles
            .iter_mut()
            .find(|profile| profile.id.eq_ignore_ascii_case(profile_id))
        else {
            self.validation = vec![Violation {
                field: "display_profiles.active_profile".into(),
                message: "selected profile no longer exists".into(),
            }];
            return;
        };
        let Some(route) = profile.routes.get_mut(route_index) else {
            self.validation = vec![Violation {
                field: "display_profiles.routes".into(),
                message: "selected display route no longer exists".into(),
            }];
            return;
        };
        route.source_position_x = x;
        route.source_position_y = y;
        route.source_width = width;
        route.source_height = height;
        route.active_width = width;
        route.active_height = height;
        route.total_width = route.total_width.max(width);
        route.total_height = route.total_height.max(height);
        route.refresh_numerator = refresh;
        route.refresh_denominator = refresh_denominator;
        route.rotation = rotation;
        profile.confirmed = false;
        self.selected_display_route = route_index;
        self.display_draft_dirty = true;
        if self.display_editor.is_none() {
            self.display_editor = Some(DisplayEditorState {
                step: DisplayWizardStep::Review,
            });
        }
        self.validation.clear();
    }

    fn delete_active_display_profile(&mut self) {
        if let Some(active) = self.draft.display_profiles.active_profile.clone() {
            self.draft.display_profiles.remove(&active);
            self.draft
                .hotkeys
                .display_profiles
                .retain(|binding| !binding.profile_id.eq_ignore_ascii_case(&active));
            self.draft
                .hotkeys
                .clear_disabled_hotkey(&format!("display_profile:{active}"));
            self.selected_display_route = 0;
        }
        self.validation.clear();
    }

    fn animate_toggle(&mut self, hwnd: HWND, id: ElementId, value: bool) {
        if !SystemVisualPreferences::query().animations_enabled {
            self.motion.clear_channel(MotionChannel::ToggleState);
            return;
        }
        self.motion.animate_to(
            id,
            MotionChannel::ToggleState,
            if value { 1.0 } else { 0.0 },
            160,
        );
        start_timer(hwnd);
    }

    fn stop_capture(&mut self) {
        crate::keyboard::hook::end_capture();
        self.capture_armed = false;
    }

    fn record_key(&mut self, hwnd: HWND, vk: u16, down: bool) -> bool {
        let Some(id) = self.recording else {
            return false;
        };
        let before = self.draft.clone();
        if vk == 0x1B && down {
            self.recording = None;
            self.recording_modifiers = ModifierMask::NONE;
            self.stop_capture();
            invalidate(hwnd);
            return true;
        }
        if id == ElementId::DisplayProfileHotkey && vk == 0x2E && down {
            let action = self.hotkey_action(HotkeySlot::DisplayProfile);
            self.set_active_profile_hotkey(None);
            if let Some(action) = action {
                self.draft.hotkeys.clear_disabled_hotkey(&action);
            }
            self.recording_modifiers = ModifierMask::NONE;
            self.stop_capture();
            self.validation = crate::config::validate(&self.draft);
            if self.display_editor.is_some() && self.draft != before {
                self.display_draft_dirty = true;
            }
            if self.validation.is_empty() && self.draft != before && !self.display_draft_dirty {
                self.commit_local_change(hwnd, before.clone());
            }
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
        if let Some(slot) = HotkeySlot::from_capture_id(id) {
            self.set_recorded_hotkey(slot, hotkey);
        }
        if self.display_editor.is_some() && self.draft != before {
            self.display_draft_dirty = true;
        }
        self.recording = None;
        self.recording_modifiers = ModifierMask::NONE;
        self.stop_capture();
        self.validation = crate::config::validate(&self.draft);
        if self.validation.is_empty() && self.draft != before && !self.display_draft_dirty {
            self.commit_local_change(hwnd, before);
        }
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
                    if let Some(slot) = HotkeySlot::from_capture_id(id) {
                        if slot == HotkeySlot::DisplayProfile
                            && key == VirtualKey(0x2E)
                            && chord.modifiers.is_empty()
                        {
                            self.set_active_hotkey(slot, None);
                            if let Some(action) = self.hotkey_action(slot) {
                                self.draft.hotkeys.clear_disabled_hotkey(&action);
                            }
                        } else {
                            self.set_recorded_hotkey(slot, hotkey);
                        }
                    }
                }
                if self.display_editor.is_some() {
                    self.display_draft_dirty = true;
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
        let order = self.layout.focus_order();
        if order.is_empty() {
            self.focused = None;
            return;
        }
        self.focus_visible = true;
        let next = Self::next_focus_index(&order, self.focused, reverse, |id| self.is_disabled(id));
        self.scroll_focus_into_view(next);
        self.rebuild_layout(hwnd);
        self.focused = Some(next);
        self.sync_search_caret(hwnd);
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
        let current = self.scroll;
        let target = if element.rect.y < top {
            (current - (top - element.rect.y)).clamp(0.0, self.layout.max_scroll)
        } else if element.rect.bottom() > bottom {
            (current + (element.rect.bottom() - bottom)).clamp(0.0, self.layout.max_scroll)
        } else {
            current
        };
        if (target - current).abs() >= 0.001 {
            self.scroll = target;
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
            (PickerKind::InputAllowlist, PickerValue::Allowlist(value)) => {
                self.draft.audio.cycle_input_allowlist = value;
            }
            (PickerKind::OutputAllowlist, PickerValue::Allowlist(value)) => {
                self.draft.audio.cycle_output_allowlist = value;
            }
            (PickerKind::DisplayProfile, PickerValue::DisplayProfile(value)) => {
                self.draft.display_profiles.active_profile = value;
                self.selected_display_route = 0;
            }
            (PickerKind::DisplayOutputs, PickerValue::DisplayOutputs(routes)) => {
                if routes.is_empty() {
                    self.validation = vec![Violation {
                        field: "display_profiles.outputs".into(),
                        message: "Select at least one output for this profile".into(),
                    }];
                    return;
                }
                if let Some(profile) = self
                    .draft
                    .display_profiles
                    .active_profile
                    .as_deref()
                    .and_then(|id| {
                        self.draft
                            .display_profiles
                            .profiles
                            .iter_mut()
                            .find(|profile| profile.id.eq_ignore_ascii_case(id))
                    })
                {
                    let mut merged = Vec::with_capacity(routes.len());
                    for selected in routes {
                        if let Some(existing) = profile
                            .routes
                            .iter()
                            .find(|route| crate::display::same_output(route, &selected))
                        {
                            merged.push(existing.clone());
                        } else {
                            merged.push(selected);
                        }
                    }
                    profile.routes = merged;
                    profile.confirmed = false;
                    if profile.routes.len() <= 1 {
                        profile.topology = crate::display::DisplayTopology::Custom;
                    } else if !matches!(
                        profile.topology,
                        crate::display::DisplayTopology::Extend
                            | crate::display::DisplayTopology::Clone
                    ) {
                        profile.topology = crate::display::DisplayTopology::Extend;
                    }
                    self.selected_display_route = self
                        .selected_display_route
                        .min(profile.routes.len().saturating_sub(1));
                    self.display_draft_dirty = true;
                }
            }
            (PickerKind::DisplayTopology, PickerValue::DisplayTopology(topology)) => {
                if let Some(profile) = self
                    .draft
                    .display_profiles
                    .active_profile
                    .as_deref()
                    .and_then(|id| {
                        self.draft
                            .display_profiles
                            .profiles
                            .iter_mut()
                            .find(|profile| profile.id.eq_ignore_ascii_case(id))
                    })
                {
                    profile.topology = topology;
                    profile.confirmed = false;
                    self.display_draft_dirty = true;
                }
            }
            (PickerKind::DisplayRoute, PickerValue::DisplayRoute(index)) => {
                self.selected_display_route = index;
            }
            (PickerKind::InputRole, PickerValue::Role(value)) => {
                self.draft.audio.input_role = value;
            }
            (PickerKind::OutputRole, PickerValue::Role(value)) => {
                self.draft.audio.output_role = value;
            }
            (PickerKind::DesktopNumberModifier, PickerValue::Modifier(value)) => {
                self.draft.virtual_desktops.number_modifier = value;
            }
            (PickerKind::MoveDesktopModifier, PickerValue::Modifier(value)) => {
                self.draft.virtual_desktops.move_follow_modifier =
                    (!value.is_empty()).then_some(value);
            }
            (PickerKind::SilentMoveDesktopModifier, PickerValue::Modifier(value)) => {
                self.draft.virtual_desktops.move_silent_modifier =
                    (!value.is_empty()).then_some(value);
            }
            (PickerKind::OverlayAppearance, PickerValue::Appearance(value)) => {
                self.draft.overlay.appearance = value;
            }
            (PickerKind::OverlayPosition, PickerValue::Position(value)) => {
                self.draft.overlay.position = value;
            }
            (PickerKind::OverlayMonitor, PickerValue::Monitor(value)) => {
                self.draft.overlay.monitor = value;
                self.refresh_overlay_preview_aspect();
            }
            _ => {}
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
        let values: Vec<(ElementId, String, bool, f32)> = self
            .layout
            .elements
            .iter()
            .filter(|element| element.kind != ElementKind::Card)
            .map(|element| {
                let id = element.id;
                let control = self.value_for(id);
                let (value, ratio) = match control {
                    ControlValue::Toggle(value) => {
                        (if value { "On".into() } else { "Off".into() }, 0.0)
                    }
                    ControlValue::Text(value) => (value.into_owned(), 0.0),
                    ControlValue::Slider { ratio, label } => (label.into_owned(), ratio),
                    ControlValue::Action(value) => (value.into_owned(), 0.0),
                };
                (id, value, !self.is_disabled(id), ratio)
            })
            .collect();
        if let Some(automation) = &self.automation {
            let mut snapshot =
                snapshot_from_settings(hwnd, &self.layout, &values, self.focused, self.dpi);
            for node in &mut snapshot.nodes {
                if let ElementId::DisplayProfileCard(index) = node.id {
                    if let Some(profile) = self.draft.display_profiles.profiles.get(index as usize)
                    {
                        node.name = profile.name.clone();
                        let ready = self
                            .profile_card_data(index as usize)
                            .is_some_and(|(_, _, _, ready, _, _)| ready);
                        node.help_text = if ready {
                            "Select this ready display profile to activate it".into()
                        } else if self.display_inventory_error.is_some() {
                            "Windows display information is unavailable; readiness is unknown"
                                .into()
                        } else if profile.confirmed {
                            "Review this profile; a saved screen needs attention".into()
                        } else {
                            "Select this profile and test it before activation".into()
                        };
                    }
                }
                match node.id {
                    ElementId::HomeSpecial => {
                        let (status, detail, action) = self.special_workspace_summary();
                        node.name = format!("Special Desktop: {status}");
                        node.value = action;
                        node.help_text = detail;
                    }
                    ElementId::HomeDiagnostics => {
                        let (title, value, detail) = self.home_diagnostics_copy();
                        node.name = title;
                        node.value = value;
                        node.help_text = detail;
                    }
                    ElementId::InputCycleMode(index) => {
                        let mode =
                            allowlist_mode(self.draft.audio.cycle_input_allowlist.as_deref());
                        node.name = crate::ui::presentation::allowlist_mode_label(
                            allowlist_mode_for_index(index),
                            AudioDeviceKind::Microphone,
                        )
                        .into();
                        node.help_text = if mode == allowlist_mode_for_index(index) {
                            "Selected cycling mode".into()
                        } else {
                            "Choose this cycling mode".into()
                        };
                    }
                    ElementId::OutputCycleMode(index) => {
                        let mode =
                            allowlist_mode(self.draft.audio.cycle_output_allowlist.as_deref());
                        node.name = crate::ui::presentation::allowlist_mode_label(
                            allowlist_mode_for_index(index),
                            AudioDeviceKind::Speaker,
                        )
                        .into();
                        node.help_text = if mode == allowlist_mode_for_index(index) {
                            "Selected cycling mode".into()
                        } else {
                            "Choose this cycling mode".into()
                        };
                    }
                    ElementId::InputDevice | ElementId::OutputDevice => {
                        let presentation = self.device_selection_view(node.id);
                        node.name = format!("{}: {}", node.name, presentation.primary);
                        node.value = presentation.accessible_value();
                        node.help_text = node.value.clone();
                    }
                    ElementId::InputCycleDevice(index) => {
                        if let Some(device) = self.devices.inputs.get(index as usize) {
                            let label = friendly_device(device, AudioDeviceKind::Microphone);
                            node.name = label.primary;
                            node.help_text = label
                                .detail
                                .unwrap_or_else(|| "Use this microphone when cycling".into());
                        }
                    }
                    ElementId::OutputCycleDevice(index) => {
                        if let Some(device) = self.devices.outputs.get(index as usize) {
                            let label = friendly_device(device, AudioDeviceKind::Speaker);
                            node.name = label.primary;
                            node.help_text = label
                                .detail
                                .unwrap_or_else(|| "Use this speaker when cycling".into());
                        }
                    }
                    ElementId::DisplayWizardSummary => {
                        if let Some(profile) = self.draft.display_profiles.active().cloned() {
                            let arrangement = if profile.routes.len() <= 1 {
                                "Single display".into()
                            } else {
                                profile.topology.label().to_string()
                            };
                            let screens = self.display_review_screen_names(&profile);
                            let shortcut = format_optional_hotkey(self.active_profile_hotkey());
                            let readiness = self.display_review_readiness(&profile);
                            node.name = format!("Display profile review: {}", profile.name);
                            node.help_text = format!(
                                "Profile: {}. Screens: {screens}. Arrangement: {arrangement}. Shortcut: {shortcut}. Readiness: {readiness}.",
                                profile.name
                            );
                        }
                    }
                    ElementId::RenameDisplayProfile => {
                        if let Some(profile) = self.draft.display_profiles.active() {
                            node.name = format!("Profile name: {}", profile.name);
                            node.help_text = "Change the current profile name".into();
                        }
                    }
                    ElementId::DisplayOutputCard(index) => {
                        if let Some((primary, detail, _, _)) =
                            self.display_output_card_data(index as usize)
                        {
                            node.name = primary;
                            node.help_text = detail;
                        }
                    }
                    ElementId::DisplayTopologyChoice(index) => {
                        node.name = if index == 0 { "Extend" } else { "Duplicate" }.into();
                    }
                    ElementId::OverlayPositionCell(index) => {
                        node.name = overlay_position_label(index as usize).into();
                        node.help_text = "Choose this overlay position".into();
                    }
                    ElementId::HotkeyEnabled(slot) => {
                        let state = if self.hotkey_enabled(slot) {
                            "Disable"
                        } else {
                            "Enable"
                        };
                        node.name = format!("{} {}", state, Self::hotkey_subject(slot));
                        node.help_text = if self.configured_hotkey(slot).is_some() {
                            format!(
                                "{} this shortcut without changing its assigned chord",
                                state
                            )
                        } else {
                            "Assign a shortcut before enabling it".into()
                        };
                    }
                    ElementId::HotkeyUnassign(slot) => {
                        node.name = format!("Unassign {}", Self::hotkey_subject(slot));
                        node.help_text = if self.configured_hotkey(slot).is_some() {
                            "Remove this shortcut completely".into()
                        } else {
                            "No shortcut is assigned".into()
                        };
                    }
                    _ => {}
                }
                if let ElementId::Nav(page) = node.id {
                    node.name = if page == self.page {
                        format!("{} (selected)", page.label())
                    } else {
                        page.label().into()
                    };
                    node.help_text = page.description().into();
                }
            }
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
        self.clear_search_focus_without_settings_window();
        self.sync_search_caret(hwnd);
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
        self.clear_search_focus_without_settings_window();
        self.sync_search_caret(hwnd);
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
        self.clear_search_focus_without_settings_window();
        self.sync_search_caret(hwnd);
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
        self.clear_search_focus_without_settings_window();
        self.sync_search_caret(hwnd);
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
        self.clear_search_focus_without_settings_window();
        self.sync_search_caret(hwnd);
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
                    self.focus_visible = true;
                    self.focused = Some(id);
                    self.activate(hwnd, id);
                }
                SettingsAutomationAction::SetSlider { id, value } => {
                    self.focus_visible = true;
                    let before = self.draft.clone();
                    if !self.is_disabled(id)
                        && self.set_slider_from_value(id, value)
                        && self.commit_local_change(hwnd, before)
                    {
                        self.focused = Some(id);
                        invalidate(hwnd);
                    }
                }
                SettingsAutomationAction::SetSearch(value) => {
                    self.focus_visible = true;
                    self.search_query = value;
                    self.reset_scroll();
                    self.focused = Some(ElementId::Search);
                    self.rebuild_layout(hwnd);
                    invalidate(hwnd);
                    focus_requested = true;
                }
                SettingsAutomationAction::SetWindowFocus => {
                    focus_requested = true;
                }
                SettingsAutomationAction::SetFocus(id) => {
                    if self.layout.element(id).is_some() && !self.is_disabled(id) {
                        self.focus_visible = true;
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

pub struct ControlCenterWindow {
    pub hwnd: HWND,
    picker: Option<PickerPopup>,
    picker_owner: Option<ElementId>,
    rename_prompt: Option<TextPrompt>,
    last_rect: Option<SavedSettingsRect>,
}

impl ControlCenterWindow {
    pub fn create(devices: crate::audio::devices::DeviceLists) -> Result<Self> {
        let _atom = *REGISTERED.get_or_init(|| {
            win::register_class(CLASS_NAME, Some(settings_wndproc))
                .expect("register settings class")
        });

        let primary = crate::platform::monitor::primary();
        let primary_dpi = primary.as_ref().map_or(96, |m| m.dpi);
        let (default_width, default_height) = fixed_window_size(primary_dpi);
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
                CONTROL_CENTER_STYLE,
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
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.refresh_overlay_preview_aspect();
            ui.rebuild_layout(hwnd);
            ui.publish_automation_snapshot(hwnd);
        }
        Ok(Self {
            hwnd,
            picker: None,
            picker_owner: None,
            last_rect: saved,
            rename_prompt: None,
        })
    }
    pub fn set_display_rollback_state(&mut self, active: bool, keep_available: bool) {
        let Some(cell) = (unsafe { win::state_cell::<SettingsUi>(self.hwnd) }) else {
            return;
        };
        let changed = {
            let mut ui = cell.borrow_mut();
            let changed =
                ui.display_rollback_active != active || ui.display_keep_available != keep_available;
            if changed {
                ui.set_display_rollback_state(active, keep_available);
                ui.rebuild_layout(self.hwnd);
            }
            changed
        };
        if changed {
            invalidate(self.hwnd);
            if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
                cell.borrow_mut().publish_automation_snapshot(self.hwnd);
            }
        }
    }
    pub fn set_runtime_snapshot(&mut self, snapshot: ControlCenterRuntimeSnapshot) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.set_runtime_snapshot(snapshot);
            ui.rebuild_layout(self.hwnd);
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }
    pub fn open_display_profile_rename(
        &mut self,
        profile_id: String,
        current_name: String,
    ) -> Result<()> {
        self.close_rename_prompt();
        self.rename_prompt = Some(TextPrompt::create(
            self.hwnd,
            PromptAction::RenameProfile { profile_id },
            "Rename display profile",
            "Rename",
            &current_name,
        )?);
        Ok(())
    }
    pub fn open_display_route_edit(
        &mut self,
        profile_id: String,
        route_index: usize,
        initial: String,
    ) -> Result<()> {
        self.close_rename_prompt();
        self.rename_prompt = Some(TextPrompt::create(
            self.hwnd,
            PromptAction::EditRoute {
                profile_id,
                route_index,
            },
            "Edit advanced display output",
            "Apply",
            &initial,
        )?);
        Ok(())
    }

    pub fn edit_display_route(&mut self, profile_id: &str, route_index: usize, value: &str) {
        self.close_rename_prompt();
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.edit_display_route(profile_id, route_index, value);
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }

    pub fn rename_display_profile(&mut self, profile_id: &str, name: &str) {
        self.close_rename_prompt();
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            let before = ui.draft.clone();
            ui.rename_display_profile(profile_id, name);
            if ui.draft != before && !ui.display_draft_dirty {
                ui.commit_local_change(self.hwnd, before);
            }
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }
    pub fn cancel_display_profile_rename(&mut self) {
        self.close_rename_prompt();
    }
    pub fn update_display_profile_after_keep(
        &mut self,
        confirmed_profile: &crate::display::DisplayProfile,
    ) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            if let Some(existing) = ui
                .draft
                .display_profiles
                .profiles
                .iter_mut()
                .find(|profile| profile.id.eq_ignore_ascii_case(&confirmed_profile.id))
            {
                *existing = confirmed_profile.clone();
            }
            ui.display_draft_dirty = false;
            ui.display_editor = None;
            ui.rebuild_layout(self.hwnd);
        }
        invalidate(self.hwnd);
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            cell.borrow_mut().publish_automation_snapshot(self.hwnd);
        }
    }

    fn close_rename_prompt(&mut self) {
        if let Some(mut prompt) = self.rename_prompt.take() {
            prompt.close();
        }
    }

    pub fn show(&mut self) -> Result<()> {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.closing = false;
            ui.startup_enabled = crate::platform::startup::is_enabled();
            if !ui.dirty() {
                ui.replace_draft((*crate::app::config()).clone());
            }
            ui.refresh_display_outputs();
            ui.refresh_overlay_preview_aspect();
            ui.rebuild_layout(self.hwnd);
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
            ui.rebuild_layout(self.hwnd);
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
        if matches!(kind, PickerKind::DisplayOutputs | PickerKind::DisplayRoute) {
            cell.borrow_mut().refresh_display_outputs();
        }
        let (draft, display_outputs, control_rect, dpi, selected_display_route) = {
            let ui = cell.borrow();
            let element = picker_element(kind)
                .and_then(|id| ui.layout.element(id))
                .ok_or_else(|| Error::internal("settings picker row missing"))?;
            (
                ui.draft.clone(),
                ui.display_outputs.clone(),
                controls::value_control_rect_for(element.rect, element.id, element.kind),
                ui.dpi,
                ui.selected_display_route,
            )
        };
        let anchor = client_rect_from_dip(control_rect, dpi);
        let (choices, current) = picker_choices(
            kind,
            &draft,
            &devices,
            &monitors,
            &display_outputs,
            selected_display_route,
        );
        let selected_indices = picker_selection_indices(kind, &draft, &choices);
        if choices.is_empty() {
            return Err(Error::config("no choices available"));
        }
        let work = client_work_rect(self.hwnd)?;
        let scale = dpi.max(96) as f32 / 96.0;
        let width = (picker_width_dip(control_rect.w, &choices) * scale).round() as i32;
        let height = picker_height_px(choices.len(), scale);
        let geometry = crate::ui::picker::place_popup(anchor, work, width, height);
        let owner =
            picker_element(kind).ok_or_else(|| Error::internal("settings picker row missing"))?;
        let picker = match PickerPopup::create(
            self.hwnd,
            kind,
            choices,
            current,
            &selected_indices,
            geometry,
        ) {
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
            let mut ui = cell.borrow_mut();
            if kind == PickerKind::DisplayProfile && ui.display_draft_dirty {
                ui.validation = vec![Violation {
                    field: "Displays".into(),
                    message:
                        "Test or discard the current display edits before selecting another profile"
                            .into(),
                }];
            } else {
                let before = ui.draft.clone();
                ui.apply_picker(kind, value);
                let risky_display_edit = matches!(
                    kind,
                    PickerKind::DisplayOutputs | PickerKind::DisplayTopology
                );
                if ui.draft != before && !risky_display_edit {
                    ui.commit_local_change(self.hwnd, before);
                }
            }
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
        self.cancel_picker();
    }

    pub fn cancel_picker(&mut self) {
        self.cancel_picker_impl(true);
    }

    pub(crate) fn cancel_picker_without_focus(&mut self) {
        self.cancel_picker_impl(false);
    }

    pub fn discard_uncommitted_draft(&mut self) {
        if let Some(cell) = unsafe { win::state_cell::<SettingsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.discard_uncommitted_draft();
            ui.publish_automation_snapshot(self.hwnd);
        }
        invalidate(self.hwnd);
    }

    pub(crate) fn close_for_hide(&mut self) {
        self.close_rename_prompt();
        self.cancel_picker_without_focus();
        self.discard_uncommitted_draft();
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
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
fn fixed_window_size(dpi: u32) -> (i32, i32) {
    let scale = dpi.max(96) as f32 / 96.0;
    (
        (DESIGN_WIDTH * scale).round() as i32,
        (DESIGN_HEIGHT * scale).round() as i32,
    )
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
        let (width, height) = fixed_window_size(target_dpi);
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

fn picker_element(kind: PickerKind) -> Option<ElementId> {
    Some(match kind {
        PickerKind::InputDevice => ElementId::InputDevice,
        PickerKind::OutputDevice => ElementId::OutputDevice,
        PickerKind::InputAllowlist => ElementId::InputAllowlist,
        PickerKind::OutputAllowlist => ElementId::OutputAllowlist,
        PickerKind::DisplayProfile => ElementId::DisplayProfile,
        PickerKind::DisplayOutputs => ElementId::DisplayOutputs,
        PickerKind::DisplayTopology => ElementId::DisplayTopology,
        PickerKind::DisplayRoute => ElementId::DisplayRoute,
        PickerKind::InputRole => ElementId::InputRole,
        PickerKind::OutputRole => ElementId::OutputRole,
        PickerKind::DesktopNumberModifier => ElementId::DesktopNumberModifier,
        PickerKind::MoveDesktopModifier => ElementId::MoveDesktopModifier,
        PickerKind::SilentMoveDesktopModifier => ElementId::SilentMoveDesktopModifier,
        PickerKind::OverlayAppearance => ElementId::OverlayAppearance,
        PickerKind::OverlayPosition => ElementId::OverlayPosition,
        PickerKind::OverlayMonitor => ElementId::OverlayMonitor,
    })
}

fn client_rect_from_dip(rect: UiRect, dpi: u32) -> PopupRect {
    let scale = dpi.max(96) as f32 / 96.0;
    PopupRect::new(
        (rect.x * scale).round() as i32,
        (rect.y * scale).round() as i32,
        (rect.right() * scale).round() as i32,
        (rect.bottom() * scale).round() as i32,
    )
}

fn client_work_rect(hwnd: HWND) -> Result<PopupRect> {
    let mut client = RECT::default();
    unsafe {
        if windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client).is_err() {
            return Err(Error::config("settings picker client area is unavailable"));
        }
    }
    let work = PopupRect::new(client.left, client.top, client.right, client.bottom);
    if work.width() <= 0 || work.height() <= 0 {
        return Err(Error::config("settings picker client area is empty"));
    }
    Ok(work)
}
const PICKER_MIN_WIDTH_DIP: f32 = 320.0;
const PICKER_MAX_WIDTH_DIP: f32 = 400.0;

fn picker_height_px(choice_count: usize, scale: f32) -> i32 {
    let items =
        (choice_count.min(10) as f32 * crate::ui::picker::ITEM_HEIGHT_DIP * scale).round() as i32;
    let inset = (UiTokens::PICKER_INSET * scale).round().max(1.0) as i32;
    items + inset * 2 + 2
}

fn picker_width_dip(control_width: f32, choices: &[PickerChoice]) -> f32 {
    // The native list uses a single-line GDI item renderer. Reserve a
    // conservative text envelope for the longest concise label, then keep
    // the popup compact enough to remain a bounded child surface.
    let longest = choices
        .iter()
        .map(|choice| choice.label.encode_utf16().count() as f32)
        .fold(0.0, f32::max);
    let content_width = 64.0 + longest * 7.2;
    control_width
        .max(PICKER_MIN_WIDTH_DIP)
        .max(content_width)
        .min(PICKER_MAX_WIDTH_DIP)
}

fn picker_choices(
    kind: PickerKind,
    draft: &Config,
    devices: &crate::audio::devices::DeviceLists,
    _monitors: &[crate::platform::monitor::MonitorGeometry],
    display_outputs: &[crate::display::DisplayOutput],
    selected_display_route: usize,
) -> (Vec<PickerChoice>, usize) {
    let mut choices = Vec::new();
    match kind {
        PickerKind::InputDevice => {
            choices.extend(device_choices(
                &devices.inputs,
                devices.input_defaults.for_role(draft.audio.input_role),
                AudioDeviceKind::Microphone,
            ));
        }
        PickerKind::OutputDevice => {
            choices.extend(device_choices(
                &devices.outputs,
                devices.output_defaults.for_role(draft.audio.output_role),
                AudioDeviceKind::Speaker,
            ));
        }
        PickerKind::InputAllowlist => {
            choices.extend(allowlist_choices(
                &devices.inputs,
                draft.audio.cycle_input_allowlist.as_deref(),
                AudioDeviceKind::Microphone,
            ));
        }
        PickerKind::OutputAllowlist => {
            choices.extend(allowlist_choices(
                &devices.outputs,
                draft.audio.cycle_output_allowlist.as_deref(),
                AudioDeviceKind::Speaker,
            ));
        }
        PickerKind::DisplayProfile => {
            choices.extend(display_profile_choices(&draft.display_profiles));
        }
        PickerKind::DisplayOutputs => {
            choices.extend(display_outputs.iter().map(|output| {
                let label = display_output_label(
                    &output.monitor_name,
                    &output.adapter_name,
                    &output.connector_name,
                    output.active,
                );
                PickerChoice {
                    label: label.compact(),
                    value: PickerValue::DisplayOutput(output.route.clone()),
                }
            }));
        }
        PickerKind::DisplayTopology => {
            for topology in [
                crate::display::DisplayTopology::Extend,
                crate::display::DisplayTopology::Clone,
            ] {
                choices.push(PickerChoice {
                    label: topology.label().into(),
                    value: PickerValue::DisplayTopology(topology),
                });
            }
            if let Some(topology) = draft
                .display_profiles
                .active()
                .map(|profile| profile.topology)
                .filter(|topology| {
                    !matches!(
                        topology,
                        crate::display::DisplayTopology::Extend
                            | crate::display::DisplayTopology::Clone
                    )
                })
            {
                choices.push(PickerChoice {
                    label: "Current arrangement (advanced)".into(),
                    value: PickerValue::DisplayTopology(topology),
                });
            }
        }
        PickerKind::DisplayRoute => {
            if let Some(profile) = draft.display_profiles.active() {
                choices.extend(profile.routes.iter().enumerate().map(|(index, route)| {
                    let label = display_outputs
                        .iter()
                        .find(|output| crate::display::same_output(&output.route, route))
                        .map(|output| {
                            display_output_label(
                                &output.monitor_name,
                                &output.adapter_name,
                                &output.connector_name,
                                output.active,
                            )
                            .compact()
                        })
                        .unwrap_or_else(|| {
                            format!("Configured display {} — unavailable", index + 1)
                        });
                    PickerChoice {
                        label,
                        value: PickerValue::DisplayRoute(index),
                    }
                }));
            }
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
        PickerKind::DesktopNumberModifier => {
            choices.extend(modifier_choices(false));
        }
        PickerKind::MoveDesktopModifier => {
            choices.extend(modifier_choices(true));
        }
        PickerKind::SilentMoveDesktopModifier => {
            choices.extend(modifier_choices(true));
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
                label: "Primary".into(),
                value: PickerValue::Monitor(MonitorChoice::Primary),
            });
            choices.push(PickerChoice {
                label: "Cursor position".into(),
                value: PickerValue::Monitor(MonitorChoice::Cursor),
            });
        }
    }
    let current_index = match kind {
        PickerKind::InputDevice => current_device_index(
            &draft.audio.input_device,
            devices.input_defaults.for_role(draft.audio.input_role),
            &choices,
        ),
        PickerKind::OutputDevice => current_device_index(
            &draft.audio.output_device,
            devices.output_defaults.for_role(draft.audio.output_role),
            &choices,
        ),
        PickerKind::InputAllowlist | PickerKind::OutputAllowlist => {
            let configured = if kind == PickerKind::InputAllowlist {
                draft.audio.cycle_input_allowlist.as_deref()
            } else {
                draft.audio.cycle_output_allowlist.as_deref()
            };
            match allowlist_mode(configured) {
                AllowlistMode::All => 0,
                AllowlistMode::Selected => 1,
                AllowlistMode::Disabled => 2,
            }
        }
        PickerKind::DisplayOutputs => 0,
        _ => {
            let current = current_picker_value(kind, draft, selected_display_route);
            choices
                .iter()
                .position(|choice| choice.value == current)
                .unwrap_or(0)
        }
    };
    (choices, current_index)
}
fn picker_selection_indices(
    kind: PickerKind,
    draft: &Config,
    choices: &[PickerChoice],
) -> Vec<usize> {
    if kind == PickerKind::DisplayOutputs {
        let Some(profile) = draft.display_profiles.active() else {
            return Vec::new();
        };
        return choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| match &choice.value {
                PickerValue::DisplayOutput(route)
                    if profile
                        .routes
                        .iter()
                        .any(|configured| crate::display::same_output(configured, route)) =>
                {
                    Some(index)
                }
                _ => None,
            })
            .collect();
    }
    let configured = match kind {
        PickerKind::InputAllowlist => draft.audio.cycle_input_allowlist.as_deref(),
        PickerKind::OutputAllowlist => draft.audio.cycle_output_allowlist.as_deref(),
        _ => return Vec::new(),
    };
    let mode = allowlist_mode(configured);
    let mode_index = match mode {
        AllowlistMode::All => 0,
        AllowlistMode::Selected => 1,
        AllowlistMode::Disabled => 2,
    };
    let mut selected = vec![mode_index];
    if mode == AllowlistMode::Selected {
        selected.extend(choices.iter().enumerate().filter_map(
            |(index, choice)| match &choice.value {
                PickerValue::Allowlist(Some(values))
                    if values.len() == 1
                        && configured.is_some_and(|ids| ids.iter().any(|id| id == &values[0])) =>
                {
                    Some(index)
                }
                _ => None,
            },
        ));
    }
    selected
}

fn allowlist_choices(
    devices: &[crate::audio::DeviceId],
    configured: Option<&[String]>,
    kind: AudioDeviceKind,
) -> Vec<PickerChoice> {
    let mut choices = vec![
        PickerChoice {
            label: crate::ui::presentation::allowlist_mode_label(AllowlistMode::All, kind).into(),
            value: PickerValue::AllowlistMode(AllowlistMode::All),
        },
        PickerChoice {
            label: crate::ui::presentation::allowlist_mode_label(AllowlistMode::Selected, kind)
                .into(),
            value: PickerValue::AllowlistMode(AllowlistMode::Selected),
        },
        PickerChoice {
            label: crate::ui::presentation::allowlist_mode_label(AllowlistMode::Disabled, kind)
                .into(),
            value: PickerValue::AllowlistMode(AllowlistMode::Disabled),
        },
    ];
    choices.extend(devices.iter().enumerate().map(|(index, device)| {
        PickerChoice {
            label: crate::ui::presentation::device_choice_label_at(devices, index, None, kind)
                .unwrap_or_else(|| "Device unavailable".into()),
            value: PickerValue::Allowlist(Some(vec![device.endpoint.clone()])),
        }
    }));
    if let Some(configured) = configured {
        for endpoint in configured {
            if !devices.iter().any(|device| device.endpoint == *endpoint) {
                choices.push(PickerChoice {
                    label: "Saved device unavailable — reconnect it to use it".into(),
                    value: PickerValue::Allowlist(Some(vec![endpoint.clone()])),
                });
            }
        }
    }
    choices
}
fn display_profile_choices(profiles: &crate::display::DisplayProfilesCfg) -> Vec<PickerChoice> {
    let mut choices = vec![PickerChoice {
        label: "No profile selected".into(),
        value: PickerValue::DisplayProfile(None),
    }];
    choices.extend(profiles.profiles.iter().map(|profile| {
        let status = if profile.confirmed {
            "Ready"
        } else {
            "Needs a test"
        };
        PickerChoice {
            label: format!("{} — {} ({status})", profile.name, profile.topology.label()),
            value: PickerValue::DisplayProfile(Some(profile.id.clone())),
        }
    }));
    choices
}

fn modifier_choices(allow_unassigned: bool) -> Vec<PickerChoice> {
    let mut choices = Vec::new();
    if allow_unassigned {
        choices.push(PickerChoice {
            label: "Unassigned".into(),
            value: PickerValue::Modifier(ModifierMask::NONE),
        });
    }
    for bits in 1u8..=0b1111 {
        let modifier = ModifierMask::from_bits(bits);
        choices.push(PickerChoice {
            label: format_modifier_display(modifier),
            value: PickerValue::Modifier(modifier),
        });
    }
    choices
}

fn device_choices(
    devices: &[crate::audio::DeviceId],
    default: Option<&crate::audio::DeviceId>,
    kind: AudioDeviceKind,
) -> Vec<PickerChoice> {
    devices
        .iter()
        .enumerate()
        .map(|(index, device)| PickerChoice {
            label: crate::ui::presentation::device_choice_label_at(devices, index, default, kind)
                .unwrap_or_else(|| "Device unavailable".into()),
            value: PickerValue::Device(DeviceSelection::Endpoint(device.endpoint.clone())),
        })
        .collect()
}

fn current_device_index(
    selection: &DeviceSelection,
    default: Option<&crate::audio::DeviceId>,
    choices: &[PickerChoice],
) -> usize {
    let endpoint = match selection {
        DeviceSelection::Default => default.map(|device| device.endpoint.as_str()),
        DeviceSelection::Endpoint(endpoint) => Some(endpoint.as_str()),
    };
    endpoint
        .and_then(|endpoint| {
            choices.iter().position(|choice| {
                matches!(
                    &choice.value,
                    PickerValue::Device(DeviceSelection::Endpoint(id)) if id == endpoint
                )
            })
        })
        .unwrap_or(0)
}

fn current_picker_value(
    kind: PickerKind,
    draft: &Config,
    selected_display_route: usize,
) -> PickerValue {
    match kind {
        PickerKind::InputDevice => PickerValue::Device(draft.audio.input_device.clone()),
        PickerKind::OutputDevice => PickerValue::Device(draft.audio.output_device.clone()),
        PickerKind::InputAllowlist => {
            PickerValue::AllowlistMode(allowlist_mode(draft.audio.cycle_input_allowlist.as_deref()))
        }
        PickerKind::OutputAllowlist => PickerValue::AllowlistMode(allowlist_mode(
            draft.audio.cycle_output_allowlist.as_deref(),
        )),
        PickerKind::DisplayProfile => {
            PickerValue::DisplayProfile(draft.display_profiles.active_profile.clone())
        }
        PickerKind::DisplayOutputs => PickerValue::DisplayOutputs(Vec::new()),
        PickerKind::DisplayTopology => PickerValue::DisplayTopology(
            draft
                .display_profiles
                .active()
                .map(|profile| profile.topology)
                .unwrap_or_default(),
        ),
        PickerKind::DisplayRoute => PickerValue::DisplayRoute(selected_display_route),
        PickerKind::InputRole => PickerValue::Role(draft.audio.input_role),
        PickerKind::OutputRole => PickerValue::Role(draft.audio.output_role),
        PickerKind::DesktopNumberModifier => {
            PickerValue::Modifier(draft.virtual_desktops.number_modifier)
        }
        PickerKind::MoveDesktopModifier => PickerValue::Modifier(
            draft
                .virtual_desktops
                .move_follow_modifier
                .unwrap_or(ModifierMask::NONE),
        ),
        PickerKind::SilentMoveDesktopModifier => PickerValue::Modifier(
            draft
                .virtual_desktops
                .move_silent_modifier
                .unwrap_or(ModifierMask::NONE),
        ),
        PickerKind::OverlayAppearance => PickerValue::Appearance(draft.overlay.appearance),
        PickerKind::OverlayPosition => PickerValue::Position(draft.overlay.position),
        PickerKind::OverlayMonitor => PickerValue::Monitor(draft.overlay.monitor.clone()),
    }
}

fn chrome_hit_test_dip(chrome: crate::ui::layout::TopChromeGeometry, x: f32, y: f32) -> u32 {
    if chrome.search.contains(x, y) || chrome.close.contains(x, y) {
        HTCLIENT
    } else if chrome.caption.contains(x, y) {
        HTCAPTION
    } else {
        HTCLIENT
    }
}
fn blocked_fixed_window_command(command: usize) -> bool {
    matches!(command & 0xFFF0, 0xF000 | 0xF020 | 0xF030 | 0xF120)
}

fn chrome_hit_test(ui: &SettingsUi, hwnd: HWND, lparam: LPARAM) -> u32 {
    let mut point = POINT {
        x: (lparam.0 as u32 & 0xFFFF) as u16 as i16 as i32,
        y: ((lparam.0 as u32 >> 16) & 0xFFFF) as u16 as i16 as i32,
    };
    if !unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
        return HTCLIENT;
    }
    let scale = 96.0 / unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96) as f32;
    let x = point.x as f32 * scale;
    let y = point.y as f32 * scale;
    let chrome = crate::ui::layout::top_chrome_geometry(ui.layout.width, ui.layout.nav_width);
    chrome_hit_test_dip(chrome, x, y)
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
            WM_NCHITTEST => {
                let ui = cell.borrow();
                LRESULT(chrome_hit_test(&ui, hwnd, lparam) as isize)
            }
            WM_SYSCOMMAND if blocked_fixed_window_command(wparam.0) => LRESULT(0),
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
                let picker_to_close = {
                    let mut ui = cell.borrow_mut();
                    ui.on_window_focus(hwnd, false, next);
                    if ui.picker_hwnd.is_some_and(|picker| picker != next)
                        && ui.picker_list_hwnd != Some(next)
                    {
                        ui.picker_hwnd
                    } else {
                        None
                    }
                };
                if let Some(popup_hwnd) = picker_to_close {
                    crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
                        popup_hwnd: popup_hwnd.0 as isize,
                        restore_focus: false,
                    });
                }
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
                    crate::event::post_main(crate::event::AppEvent::ControlCenterWindowClosed);
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
            WM_WINDOWPOSCHANGING => {
                let position = &mut *(lparam.0 as *mut WINDOWPOS);
                let (width, height) = fixed_window_size(cell.borrow().dpi);
                position.cx = width;
                position.cy = height;
                LRESULT(0)
            }
            WM_SIZE => {
                let picker_hwnd = cell.borrow().picker_hwnd;
                if let Some(popup_hwnd) = picker_hwnd {
                    crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
                        popup_hwnd: popup_hwnd.0 as isize,
                        restore_focus: false,
                    });
                }
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
                let picker_hwnd = cell.borrow().picker_hwnd;
                if let Some(popup_hwnd) = picker_hwnd {
                    crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
                        popup_hwnd: popup_hwnd.0 as isize,
                        restore_focus: false,
                    });
                }
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
                let (fixed_width, fixed_height) = fixed_window_size(new_dpi);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    fixed_width,
                    fixed_height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let mut ui = cell.borrow_mut();
                ui.rebuild_layout(hwnd);
                ui.publish_automation_snapshot(hwnd);
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                let mut ui = cell.borrow_mut();
                ui.refresh_overlay_preview_aspect();
                ui.rebuild_layout(hwnd);
                ui.publish_automation_snapshot(hwnd);
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                let theme = settings_theme();
                let theme_applied = {
                    let mut ui = cell.borrow_mut();
                    if let Some(renderer) = ui.renderer.as_mut() {
                        match renderer.set_theme(theme) {
                            Ok(()) => true,
                            Err(error) => {
                                crate::error_!("settings theme change failed: {error}");
                                false
                            }
                        }
                    } else {
                        true
                    }
                };
                if theme_applied {
                    apply_chrome(hwnd, theme);
                }
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let (x, y, needs_track) = {
                    let ui = cell.borrow();
                    let (x, y) = mouse_point(lparam, ui.dpi);
                    (x, y, !ui.mouse_tracking)
                };
                if needs_track {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut track);
                    cell.borrow_mut().mouse_tracking = true;
                }
                let mut ui = cell.borrow_mut();
                if let Some(offset) = ui.scroll_drag_offset {
                    ui.scroll = controls::scroll_from_scrollbar_pointer(
                        ui.layout.content_clip,
                        y,
                        offset,
                        ui.layout.max_scroll,
                    );
                    ui.rebuild_layout(hwnd);
                    ui.publish_automation_snapshot(hwnd);
                    invalidate(hwnd);
                    return LRESULT(0);
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
                ui.set_hover(hwnd, None);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (picker_hwnd, close_hit) = {
                    let mut ui = cell.borrow_mut();
                    let picker_hwnd = ui.picker_hwnd;
                    let close_hit = picker_hwnd.is_some_and(|_| {
                        let (x, y) = mouse_point(lparam, ui.dpi);
                        ui.rebuild_layout(hwnd);
                        ui.layout.hit_test(x, y) == Some(ElementId::WindowClose)
                    });
                    (picker_hwnd, close_hit)
                };
                if let Some(popup_hwnd) = picker_hwnd {
                    crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
                        popup_hwnd: popup_hwnd.0 as isize,
                        restore_focus: true,
                    });
                    if !close_hit {
                        return LRESULT(0);
                    }
                }
                let (focus_requested, capture_requested) = {
                    let mut ui = cell.borrow_mut();
                    let (x, y) = mouse_point(lparam, ui.dpi);
                    ui.rebuild_layout(hwnd);
                    let mut focus_requested = false;
                    let mut capture_requested = false;
                    let scrollbar_hit = ui.layout.max_scroll > 0.0
                        && controls::scrollbar_hit_rect(ui.layout.content_clip).contains(x, y);
                    if scrollbar_hit {
                        ui.set_pointer_focus(None);
                        ui.sync_search_caret(hwnd);
                        if let Some(thumb) = controls::scrollbar_thumb_rect(
                            ui.layout.content_clip,
                            ui.scroll,
                            ui.layout.max_scroll,
                        ) {
                            let offset = if thumb.contains(x, y) {
                                y - thumb.y
                            } else {
                                thumb.h * 0.5
                            };
                            ui.scroll_drag_offset = Some(offset);
                            ui.scroll = controls::scroll_from_scrollbar_pointer(
                                ui.layout.content_clip,
                                y,
                                offset,
                                ui.layout.max_scroll,
                            );
                            ui.rebuild_layout(hwnd);
                            capture_requested = true;
                            invalidate(hwnd);
                        }
                    } else {
                        let hit = ui.layout.hit_test(x, y);
                        ui.set_pointer_focus(hit);
                        ui.sync_search_caret(hwnd);
                        if let Some(id) = hit {
                            if !ui.is_disabled(id) {
                                focus_requested = true;
                                capture_requested = true;
                                ui.pressed = Some(id);
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
                    }
                    invalidate(hwnd);
                    ui.publish_automation_snapshot(hwnd);
                    (focus_requested, capture_requested)
                };
                if capture_requested {
                    let _ = SetCapture(hwnd);
                }
                if focus_requested {
                    let _ = SetFocus(Some(hwnd));
                    let actual = GetFocus();
                    cell.borrow_mut().sync_focus_after_set_focus(hwnd, actual);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if cell.borrow_mut().scroll_drag_offset.take().is_some() {
                    let _ = ReleaseCapture();
                    invalidate(hwnd);
                    return LRESULT(0);
                }
                let (slider, activate_id) = {
                    let mut ui = cell.borrow_mut();
                    let (x, y) = mouse_point(lparam, ui.dpi);
                    let pressed = ui.pressed.take();
                    let slider = pressed.is_some_and(|id| {
                        matches!(
                            id,
                            ElementId::OverlayDuration
                                | ElementId::OverlayOpacity
                                | ElementId::OverlayScale
                        )
                    });
                    let activate_id =
                        pressed.filter(|id| !slider && ui.layout.hit_test(x, y) == Some(*id));
                    (slider, activate_id)
                };
                let _ = ReleaseCapture();
                if slider {
                    if let Some(cell) = win::state_cell::<SettingsUi>(hwnd) {
                        let mut ui = cell.borrow_mut();
                        let before = (*crate::app::config()).clone();
                        ui.commit_local_change(hwnd, before);
                        ui.publish_automation_snapshot(hwnd);
                    }
                } else if let Some(id) = activate_id {
                    cell.borrow_mut().activate(hwnd, id);
                }
                invalidate(hwnd);
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
                        ui.set_scroll(scroll, hwnd);
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
                    ui.focus_visible = true;
                    if ui.handle_search_key(hwnd, vk) {
                        return LRESULT(0);
                    }
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
                        let target =
                            page_scroll_target(ui.scroll, page, ui.layout.max_scroll, vk == 0x22);
                        ui.set_scroll(target, hwnd);
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
                            crate::event::post_main(
                                crate::event::AppEvent::ControlCenterWindowClosed,
                            );
                        }
                        LRESULT(0)
                    }
                    _ => win::def_proc(hwnd, msg, wparam, lparam),
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                let consumed = {
                    let mut ui = cell.borrow_mut();
                    ui.record_key(hwnd, wparam.0 as u16, false)
                };
                if consumed {
                    return LRESULT(0);
                }
                win::def_proc(hwnd, msg, wparam, lparam)
            }
            WM_TIMER if wparam.0 == UI_TIMER => {
                let mut ui = cell.borrow_mut();
                if ui.recording.is_some() {
                    while let Some(chord) = crate::keyboard::hook::take_captured_chord() {
                        let before = ui.draft.clone();
                        ui.finish_recording(chord);
                        if ui.recording.is_none()
                            && ui.validation.is_empty()
                            && ui.draft != before
                            && !ui.display_draft_dirty
                        {
                            ui.commit_local_change(hwnd, before);
                        }
                    }
                    if ui.recording.is_none() {
                        crate::keyboard::hook::end_capture();
                        ui.capture_armed = false;
                    }
                }
                let active = ui.motion.tick();
                let caret_active = ui.tick_search_caret(Instant::now());
                let applied = ui.applied_until.is_some_and(|until| Instant::now() < until);
                invalidate(hwnd);
                if !active && !caret_active && !applied && ui.recording.is_none() {
                    let _ = KillTimer(Some(hwnd), UI_TIMER);
                }
                LRESULT(0)
            }
            WM_CHAR => {
                let mut ui = cell.borrow_mut();
                if ui.handle_search_char(hwnd, wparam.0 as u16) {
                    return LRESULT(0);
                }
                LRESULT(0)
            }
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
    (scroll - delta / 120.0 * WHEEL_SCROLL_DIP).clamp(0.0, max_scroll)
}
fn page_scroll_target(scroll: f32, page: f32, max_scroll: f32, forward: bool) -> f32 {
    if forward {
        (scroll + page).min(max_scroll)
    } else {
        (scroll - page).max(0.0)
    }
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

#[cfg(test)]
fn close_picker_before_settings_hide<Cancel, Discard, Hide>(
    cancel_picker: Cancel,
    discard_draft: Discard,
    hide_settings: Hide,
) where
    Cancel: FnOnce(),
    Discard: FnOnce(),
    Hide: FnOnce(),
{
    cancel_picker();
    discard_draft();
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
type DisplayRouteEditValues = (i32, i32, u32, u32, u32, u32, i32);

fn parse_display_route_values(
    value: &str,
) -> std::result::Result<DisplayRouteEditValues, &'static str> {
    let mut parts = value.split(',').map(str::trim);
    let x = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse()
        .map_err(|_| "route x must be an integer")?;
    let y = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse()
        .map_err(|_| "route y must be an integer")?;
    let width = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse::<u32>()
        .map_err(|_| "route width must be a positive integer")?;
    let height = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse::<u32>()
        .map_err(|_| "route height must be a positive integer")?;
    let refresh = parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?;
    let (refresh, refresh_denominator) = parse_display_refresh(refresh)?;
    let rotation = match parts
        .next()
        .ok_or("route edit needs x,y,width,height,refresh,rotation")?
        .parse::<i32>()
        .map_err(|_| "route rotation must be 0, 90, 180, or 270 degrees")?
    {
        0 => 1,
        90 => 2,
        180 => 3,
        270 => 4,
        _ => return Err("route rotation must be 0, 90, 180, or 270 degrees"),
    };
    if parts.next().is_some() {
        return Err("route edit has too many comma-separated values");
    }
    if width == 0 || height == 0 {
        return Err("route width and height must be positive");
    }
    Ok((x, y, width, height, refresh, refresh_denominator, rotation))
}

fn parse_display_refresh(value: &str) -> std::result::Result<(u32, u32), &'static str> {
    let mut values = value.split('/');
    let numerator = values
        .next()
        .ok_or("route refresh must be a positive integer or numerator/denominator")?
        .parse::<u32>()
        .map_err(|_| "route refresh must be a positive integer or numerator/denominator")?;
    let denominator = values.next().map_or(Ok(1), |value| {
        value
            .parse::<u32>()
            .map_err(|_| "route refresh denominator must be positive")
    })?;
    if values.next().is_some() {
        return Err("route refresh must be a positive integer or numerator/denominator");
    }
    if numerator == 0 || denominator == 0 {
        return Err("route refresh must be positive");
    }
    Ok((numerator, denominator))
}

fn rotation_degrees(value: i32) -> i32 {
    match value {
        2 => 90,
        3 => 180,
        4 => 270,
        _ => 0,
    }
}

fn next_display_profile_identity(profiles: &[crate::display::DisplayProfile]) -> (String, String) {
    for number in 1usize.. {
        let id = format!("profile-{number}");
        let name = format!("Profile {number}");
        if profiles.iter().all(|profile| {
            !profile.id.eq_ignore_ascii_case(&id) && !profile.name.eq_ignore_ascii_case(&name)
        }) {
            return (id, name);
        }
    }
    unreachable!("profile identity space exhausted")
}

fn allowlist_label(allowlist: Option<&[String]>) -> String {
    match allowlist {
        None => "All available devices".into(),
        Some([]) => "Don't cycle".into(),
        Some(ids) => format!("{} selected", ids.len()),
    }
}

fn allowlist_mode_for_index(index: u8) -> AllowlistMode {
    match index {
        0 => AllowlistMode::All,
        1 => AllowlistMode::Selected,
        _ => AllowlistMode::Disabled,
    }
}

fn overlay_position(index: usize) -> OverlayPosition {
    OverlayPosition::ALL[index.min(OverlayPosition::ALL.len() - 1)]
}
fn work_area_aspect(work: Option<RECT>) -> (u32, u32) {
    work.map_or((16, 9), |work| {
        let width = work.right.saturating_sub(work.left).max(1) as u32;
        let height = work.bottom.saturating_sub(work.top).max(1) as u32;
        (width, height)
    })
}

fn overlay_preview_aspect(choice: &MonitorChoice) -> (u32, u32) {
    let work = match choice {
        MonitorChoice::Primary => crate::platform::monitor::primary().map(|monitor| monitor.work),
        MonitorChoice::Cursor => crate::platform::monitor::cursor().map(|monitor| monitor.work),
        MonitorChoice::Device(name) => crate::platform::monitor::all()
            .into_iter()
            .find(|monitor| monitor.device_name.eq_ignore_ascii_case(name))
            .map(|monitor| monitor.work),
    };
    work_area_aspect(work)
}

fn overlay_preview_card_rect(canvas: UiRect, position: OverlayPosition, scale: f32) -> UiRect {
    let scale = scale.clamp(0.7, 1.6);
    let margin_x = (canvas.w * 0.04).clamp(8.0, 48.0);
    let margin_y = (canvas.h * 0.04).clamp(8.0, 32.0);
    let max_width = (canvas.w - margin_x * 2.0).max(1.0);
    let max_height = (canvas.h - margin_y * 2.0).max(1.0);
    let width = (canvas.w * 0.36 * scale).min(max_width).max(1.0);
    let height = (canvas.h * 0.34 * scale).min(max_height).max(1.0);
    let x = match position {
        OverlayPosition::TopLeft | OverlayPosition::CenterLeft | OverlayPosition::BottomLeft => {
            canvas.x + margin_x
        }
        OverlayPosition::TopCenter | OverlayPosition::Center | OverlayPosition::BottomCenter => {
            canvas.x + (canvas.w - width) * 0.5
        }
        OverlayPosition::TopRight | OverlayPosition::CenterRight | OverlayPosition::BottomRight => {
            canvas.right() - margin_x - width
        }
    };
    let y = match position {
        OverlayPosition::TopLeft | OverlayPosition::TopCenter | OverlayPosition::TopRight => {
            canvas.y + margin_y
        }
        OverlayPosition::CenterLeft | OverlayPosition::Center | OverlayPosition::CenterRight => {
            canvas.y + (canvas.h - height) * 0.5
        }
        OverlayPosition::BottomLeft
        | OverlayPosition::BottomCenter
        | OverlayPosition::BottomRight => canvas.bottom() - margin_y - height,
    };
    UiRect::new(x, y, width, height)
}

fn overlay_position_label(index: usize) -> &'static str {
    overlay_position(index).label()
}

fn human_subsystem_name(name: &str) -> &str {
    match name {
        "audio" => "Audio",
        "desktop" => "Workspace",
        "overlay" => "Overlay",
        "keyboard" => "Shortcuts",
        "tray" => "Tray",
        "foreground" => "Current app audio",
        _ => "WinShort service",
    }
}

fn friendly_violation(violation: &Violation) -> String {
    let message = violation.message.as_str();
    if violation.field == "Startup" {
        "WinShort couldn't update Windows startup. Try again.".into()
    } else if message.contains("configuration invalid") {
        "WinShort couldn't save this change. Check the setting and try again.".into()
    } else if message.contains("hotkey") || message.contains("modifier") {
        "That shortcut conflicts with another action. Choose a different key combination.".into()
    } else if message.contains("display") || message.contains("route") {
        "The selected display setup is not available. Check Displays and try again.".into()
    } else {
        message.to_string()
    }
}
fn overlay_duration_label(milliseconds: u32) -> &'static str {
    match milliseconds {
        0..=2_000 => "Short",
        2_001..=5_000 => "Normal",
        _ => "Long",
    }
}

fn overlay_opacity_label(opacity: f32) -> &'static str {
    if opacity < 0.6 {
        "Low"
    } else if opacity < 0.9 {
        "Normal"
    } else {
        "High"
    }
}

fn overlay_scale_label(scale: f32) -> &'static str {
    if scale < 1.0 {
        "Small"
    } else if scale < 1.4 {
        "Normal"
    } else {
        "Large"
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
    fn unavailable_explicit_device_is_not_exposed_as_a_system_target() {
        let mut config = Config::default();
        config.audio.input_device = DeviceSelection::Endpoint("missing-endpoint".into());
        let devices = crate::audio::devices::DeviceLists {
            inputs: vec![crate::audio::DeviceId {
                endpoint: "current-endpoint".into(),
                name: "Current microphone".into(),
            }],
            outputs: Vec::new(),
            input_defaults: Default::default(),
            output_defaults: Default::default(),
            warnings: Vec::new(),
        };

        let (choices, current) =
            picker_choices(PickerKind::InputDevice, &config, &devices, &[], &[], 0);

        assert_eq!(choices.len(), 1);
        assert_eq!(current, 0);
        assert_eq!(choices[0].label, "Current microphone");
        assert_eq!(
            choices[0].value,
            PickerValue::Device(DeviceSelection::Endpoint("current-endpoint".into()))
        );
        assert_eq!(
            config.audio.input_device,
            DeviceSelection::Endpoint("missing-endpoint".into())
        );
    }

    #[test]
    fn overlay_monitor_picker_exposes_only_primary_and_cursor_position() {
        let mut config = Config::default();
        let (choices, current) = picker_choices(
            PickerKind::OverlayMonitor,
            &config,
            &Default::default(),
            &[],
            &[],
            0,
        );

        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Primary", "Cursor position"]
        );
        assert_eq!(current, 1);
        assert_eq!(
            choices[1].value,
            PickerValue::Monitor(MonitorChoice::Cursor)
        );

        config.overlay.monitor = MonitorChoice::Primary;
        let (_, current) = picker_choices(
            PickerKind::OverlayMonitor,
            &config,
            &Default::default(),
            &[],
            &[],
            0,
        );
        assert_eq!(current, 0);
    }

    #[test]
    fn device_picker_exposes_only_real_endpoints_and_marks_system_default() {
        let current = crate::audio::DeviceId {
            endpoint: "current-endpoint".into(),
            name: "Current microphone".into(),
        };
        let devices = crate::audio::devices::DeviceLists {
            inputs: vec![current.clone()],
            outputs: Vec::new(),
            input_defaults: crate::audio::devices::DefaultDevices {
                console: Some(current),
                ..Default::default()
            },
            output_defaults: Default::default(),
            warnings: Vec::new(),
        };

        let config = Config::default();
        let (choices, selected) =
            picker_choices(PickerKind::InputDevice, &config, &devices, &[], &[], 0);

        assert_eq!(choices.len(), 1);
        assert_eq!(selected, 0);
        assert_eq!(choices[0].label, "Current microphone");
        assert_eq!(
            choices[0].value,
            PickerValue::Device(DeviceSelection::Endpoint("current-endpoint".into()))
        );
        assert_ne!(
            choices[0].value,
            PickerValue::Device(DeviceSelection::Default)
        );
    }

    #[test]
    fn allowlist_picker_exposes_clear_controls_and_offline_selections() {
        let mut config = Config::default();
        config.audio.cycle_input_allowlist =
            Some(vec!["second-endpoint".into(), "missing-endpoint".into()]);
        let devices = crate::audio::devices::DeviceLists {
            inputs: vec![
                crate::audio::DeviceId {
                    endpoint: "first-endpoint".into(),
                    name: "First microphone".into(),
                },
                crate::audio::DeviceId {
                    endpoint: "second-endpoint".into(),
                    name: "Second microphone".into(),
                },
            ],
            outputs: Vec::new(),
            input_defaults: Default::default(),
            output_defaults: Default::default(),
            warnings: Vec::new(),
        };
        let (choices, current) =
            picker_choices(PickerKind::InputAllowlist, &config, &devices, &[], &[], 0);
        assert_eq!(current, 1);
        assert_eq!(choices[0].label, "All available microphones");
        assert_eq!(choices[1].label, "Selected microphones");
        assert_eq!(choices[2].label, "Don't cycle microphones");
        assert_eq!(
            picker_selection_indices(PickerKind::InputAllowlist, &config, &choices),
            vec![1, 4, 5]
        );
        assert!(choices[5].label.starts_with("Saved device unavailable"));

        config.audio.cycle_input_allowlist = None;
        assert_eq!(
            picker_selection_indices(PickerKind::InputAllowlist, &config, &choices),
            vec![0]
        );
        config.audio.cycle_input_allowlist = Some(Vec::new());
        assert_eq!(
            picker_selection_indices(PickerKind::InputAllowlist, &config, &choices),
            vec![2]
        );
    }
    #[test]
    fn advanced_display_topology_stays_selected_in_picker() {
        let mut config = Config::default();
        let mut profile = sample_profile("custom", "Custom layout", true);
        profile.topology = crate::display::DisplayTopology::Custom;
        config.display_profiles.profiles = vec![profile];
        config.display_profiles.active_profile = Some("custom".into());

        let (choices, selected) = picker_choices(
            PickerKind::DisplayTopology,
            &config,
            &Default::default(),
            &[],
            &[],
            0,
        );
        assert_eq!(choices[selected].label, "Current arrangement (advanced)");
        assert_eq!(
            choices[selected].value,
            PickerValue::DisplayTopology(crate::display::DisplayTopology::Custom)
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
    fn pointer_focus_stays_semantic_without_painting_a_focus_ring() {
        let id = ElementId::OverlayEnabled;
        let mut ui = empty_settings_ui();
        ui.focused = Some(id);
        ui.focus_visible = false;
        assert!(!ui.interaction(id, false).focused);

        ui.focus_visible = true;
        assert!(ui.interaction(id, false).focused);
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
    fn fixed_window_size_is_dpi_scaled_and_not_user_sized() {
        assert_eq!(fixed_window_size(96), (960, 660));
        assert_eq!(fixed_window_size(144), (1440, 990));
        assert_eq!(fixed_window_size(192), (1920, 1320));
        assert_eq!(fixed_window_size(0), (960, 660));

        let work = RECT {
            left: 0,
            top: 0,
            right: 2400,
            bottom: 1600,
        };
        let (width, height) = fixed_window_size(144);
        let rect = clamp_window_rect(
            RECT {
                left: 100,
                top: 100,
                right: 100 + width,
                bottom: 100 + height,
            },
            work,
        );
        assert_eq!(rect.right - rect.left, width);
        assert_eq!(rect.bottom - rect.top, height);
    }

    #[test]
    fn display_outputs_value_hides_raw_displayconfig_identity() {
        let mut ui = empty_settings_ui();
        let route = crate::display::DisplayRoute {
            target_path: r"\\?\DISPLAY#MONITOR-A".into(),
            target_adapter: 1,
            target_id: 2,
            output_technology: 5,
            ..Default::default()
        };
        ui.display_outputs = vec![crate::display::DisplayOutput {
            route: route.clone(),
            monitor_name: "Desk Monitor".into(),
            adapter_name: "AMD Radeon Graphics".into(),
            connector_name: "HDMI".into(),
            active: true,
        }];
        ui.display_inventory_loaded = true;
        ui.draft.display_profiles.profiles = vec![crate::display::DisplayProfile {
            id: "ai".into(),
            name: "AI".into(),
            topology: crate::display::DisplayTopology::Custom,
            confirmed: false,
            routes: vec![route],
        }];
        ui.draft.display_profiles.active_profile = Some("ai".into());
        match ui.value_for(ElementId::DisplayOutputs) {
            ControlValue::Text(value) => {
                assert!(value.contains("Desk Monitor"));
                assert!(value.contains("AMD Radeon Graphics"));
                assert!(!value.contains("DISPLAY#"));
            }
            _ => panic!("unexpected control value variant"),
        }
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
            input_defaults: crate::audio::devices::DefaultDevices {
                console: Some(crate::audio::DeviceId {
                    endpoint: "input".into(),
                    name: "Cached microphone".into(),
                }),
                ..Default::default()
            },
            output_defaults: Default::default(),
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
    fn current_app_layout_flag_tracks_external_runtime_target() {
        let mut ui = empty_settings_ui();
        assert!(!ui.layout_context().current_app_audio_available);

        ui.runtime.foreground = crate::audio::AppAudioState {
            app_name: Some("Player".into()),
            aggregate: crate::audio::Aggregate::AllActive,
            sessions: 1,
            error: None,
        };
        assert!(ui.layout_context().current_app_audio_available);
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
    fn picker_anchor_is_converted_to_control_center_client_pixels() {
        let row = UiRect::new(24.0, 300.0, 560.0, 58.0);
        let control = controls::value_control_rect(row, crate::ui::layout::ElementKind::Value);
        let anchor = client_rect_from_dip(control, 144);
        assert_eq!(anchor.width(), 309);
        assert_eq!(anchor.height(), 51);
        let work = PopupRect::new(0, 0, 900, 600);
        let popup = crate::ui::picker::place_popup(anchor, work, 400, 180);
        assert_eq!(popup.right, anchor.right);
        assert_eq!(popup.bottom, anchor.top);
        assert!(popup.right <= work.right);
        assert!(popup.bottom <= work.bottom);
    }

    #[test]
    fn picker_width_is_compact_and_bounded() {
        let short = vec![PickerChoice {
            label: "Top Left".into(),
            value: PickerValue::Position(OverlayPosition::TopLeft),
        }];
        let long = vec![PickerChoice {
            label: "A deliberately long endpoint name for the default device".into(),
            value: PickerValue::Position(OverlayPosition::TopLeft),
        }];
        assert_eq!(picker_width_dip(190.0, &short), 320.0);
        assert_eq!(picker_width_dip(190.0, &long), 400.0);
        assert_eq!(picker_width_dip(900.0, &long), 400.0);
    }
    #[test]
    fn picker_height_includes_shared_host_inset() {
        assert_eq!(picker_height_px(0, 1.0), 10);
        assert_eq!(picker_height_px(3, 1.0), 106);
        assert_eq!(picker_height_px(12, 1.0), 330);
        assert_eq!(picker_height_px(3, 1.5), 158);
    }

    #[test]
    fn compact_overlay_preview_keeps_sample_inside_canvas() {
        let canvas = UiRect::new(0.0, 0.0, 444.0, 108.0);
        for position in OverlayPosition::ALL {
            for scale in [0.7, 1.0, 1.6] {
                let sample = overlay_preview_card_rect(canvas, position, scale);
                assert!(sample.x >= canvas.x);
                assert!(sample.y >= canvas.y);
                assert!(sample.right() <= canvas.right());
                assert!(sample.bottom() <= canvas.bottom());
            }
        }
    }

    fn empty_settings_ui() -> SettingsUi {
        let mut ui = SettingsUi::new(
            96,
            crate::audio::devices::DeviceLists {
                inputs: Vec::new(),
                outputs: Vec::new(),
                input_defaults: Default::default(),
                output_defaults: Default::default(),
                warnings: Vec::new(),
            },
        );
        ui.onboarding_step = None;
        ui.layout = SettingsLayout::build(DESIGN_WIDTH, DESIGN_HEIGHT, 0.0);
        ui
    }

    fn search_settings_ui() -> SettingsUi {
        let mut ui = empty_settings_ui();
        ui.layout =
            SettingsLayout::build_shell(DESIGN_WIDTH, DESIGN_HEIGHT, 0.0, Page::Home, "", 0, None);
        ui.focus_owner = AutomationFocusOwner::Settings;
        ui
    }

    #[test]
    fn search_pointer_focus_clears_on_blank_client_click() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = search_settings_ui();
        ui.set_pointer_focus(Some(ElementId::Search));
        ui.sync_search_caret(hwnd);
        assert!(ui.search_has_focus());

        let blank = ui
            .layout
            .hit_test(ui.layout.top_bar.x + 8.0, ui.layout.top_bar.y + 2.0);
        assert_eq!(blank, None);
        ui.set_pointer_focus(blank);
        ui.sync_search_caret(hwnd);

        assert_eq!(ui.focused, None);
        assert!(!ui.search_has_focus());
        assert!(!ui.search_caret_visible);
    }

    #[test]
    fn search_pointer_focus_transfers_to_another_control() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = search_settings_ui();
        ui.set_pointer_focus(Some(ElementId::Search));
        ui.sync_search_caret(hwnd);

        let audio = ui
            .layout
            .element(ElementId::Nav(Page::Audio))
            .expect("audio navigation")
            .rect;
        let target = ui
            .layout
            .hit_test(audio.x + audio.w * 0.5, audio.y + audio.h * 0.5);
        assert_eq!(target, Some(ElementId::Nav(Page::Audio)));
        ui.set_pointer_focus(target);
        ui.sync_search_caret(hwnd);

        assert_eq!(ui.focused, target);
        assert!(!ui.search_has_focus());
        assert!(!ui.search_caret_visible);
    }

    #[test]
    fn search_external_focus_loss_clears_editing_and_character_handling() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let outside = HWND(17usize as *mut _);
        let mut ui = search_settings_ui();
        ui.set_pointer_focus(Some(ElementId::Search));
        ui.sync_search_caret(hwnd);
        assert!(ui.handle_search_char(hwnd, 'a' as u16));
        assert_eq!(ui.search_query, "a");

        ui.on_window_focus(hwnd, false, outside);

        assert_eq!(ui.focused, None);
        assert!(!ui.search_has_focus());
        assert!(!ui.search_caret_visible);
        assert!(!ui.handle_search_char(hwnd, 'b' as u16));
        assert!(!ui.handle_search_key(hwnd, 0x08));
        assert_eq!(ui.search_query, "a");
    }

    #[test]
    fn stale_search_element_does_not_consume_input_without_settings_focus() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = search_settings_ui();
        ui.focused = Some(ElementId::Search);
        ui.focus_owner = AutomationFocusOwner::Outside;
        ui.search_query = "keep".into();
        ui.search_caret_visible = true;

        assert!(!ui.handle_search_char(hwnd, 'x' as u16));
        assert!(!ui.handle_search_key(hwnd, 0x08));
        ui.sync_search_caret(hwnd);
        assert_eq!(ui.search_query, "keep");
        assert!(!ui.search_caret_visible);
        assert!(!ui.search_has_focus());
    }

    #[test]
    fn search_caret_visibility_uses_the_editing_focus_predicate() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = search_settings_ui();
        ui.focused = Some(ElementId::Search);
        ui.focus_owner = AutomationFocusOwner::Outside;
        ui.search_caret_visible = true;
        ui.sync_search_caret(hwnd);
        assert!(!ui.search_caret_visible);

        ui.focus_owner = AutomationFocusOwner::Settings;
        ui.sync_search_caret(hwnd);
        assert!(ui.search_has_focus());
        assert!(ui.search_caret_visible);
        assert!(ui.search_caret_deadline.is_some());

        ui.focused = Some(ElementId::Nav(Page::Audio));
        ui.sync_search_caret(hwnd);
        assert!(!ui.search_caret_visible);
        assert!(ui.search_caret_deadline.is_none());
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
        ui.page = Page::Audio;
        ui.rebuild_layout(hwnd);
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
    fn managed_shortcut_buttons_expose_dynamic_uia_names() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = empty_settings_ui();
        ui.page = Page::Shortcuts;
        ui.rebuild_layout(hwnd);
        ui.install_automation(hwnd);

        let snapshot = ui.automation.as_ref().expect("automation").snapshot();
        let state = snapshot
            .nodes
            .iter()
            .find(|node| node.id == ElementId::HotkeyEnabled(HotkeySlot::Microphone))
            .expect("shortcut state node");
        assert_eq!(state.name, "Disable Mute microphone shortcut");
        assert!(state.enabled);
        let unassign = snapshot
            .nodes
            .iter()
            .find(|node| node.id == ElementId::HotkeyUnassign(HotkeySlot::Microphone))
            .expect("shortcut unassign node");
        assert_eq!(unassign.name, "Unassign Mute microphone shortcut");
        assert!(unassign.enabled);
    }

    #[test]
    fn picker_focus_state_suppresses_logical_child_focus() {
        let hwnd = HWND(2usize as *mut _);
        let mut ui = empty_settings_ui();
        ui.install_automation(hwnd);
        ui.focused = Some(ElementId::InputDevice);
        let picker_hwnd = HWND(3usize as *mut _);
        let picker_list_hwnd = HWND(4usize as *mut _);
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
    fn wheel_scroll_policy_accumulates_native_sized_deltas() {
        assert_eq!(scroll_after_wheel(128.0, 120.0, 512.0), 48.0);
        assert_eq!(scroll_after_wheel(48.0, 120.0, 512.0), 0.0);
        assert_eq!(scroll_after_wheel(128.0, 240.0, 512.0), 0.0);
        assert_eq!(scroll_after_wheel(0.0, 120.0, 512.0), 0.0);
        assert_eq!(scroll_after_wheel(512.0, -120.0, 512.0), 512.0);
    }

    #[test]
    fn wheel_and_page_scroll_are_immediate_and_accumulate_without_tween() {
        let mut scroll = 128.0;
        scroll = scroll_after_wheel(scroll, 120.0, 512.0);
        assert_eq!(scroll, 48.0);
        scroll = scroll_after_wheel(scroll, 240.0, 512.0);
        assert_eq!(scroll, 0.0);
        assert_eq!(page_scroll_target(128.0, 300.0, 512.0, true), 428.0);
        assert_eq!(page_scroll_target(128.0, 300.0, 512.0, false), 0.0);
        assert!(!Motion::default().has_active());
    }

    #[test]
    fn fixed_titlebar_exposes_only_close_and_no_resize_actions() {
        let layout = SettingsLayout::build_shell(960.0, 660.0, 0.0, Page::Home, "", 0, None);
        let close = layout
            .element(ElementId::WindowClose)
            .expect("close button");
        assert_eq!(close.kind, ElementKind::ButtonSecondary);
        assert!(node_has_invoke(close.kind));
        assert!(layout.focus_order().contains(&close.id));
        assert_eq!(
            layout
                .elements
                .iter()
                .filter(|element| {
                    !element.scrolls && element.kind == ElementKind::ButtonSecondary
                })
                .map(|element| element.id)
                .collect::<Vec<_>>(),
            vec![ElementId::WindowClose]
        );

        let forbidden = windows::Win32::UI::WindowsAndMessaging::WS_THICKFRAME.0
            | windows::Win32::UI::WindowsAndMessaging::WS_MINIMIZEBOX.0
            | windows::Win32::UI::WindowsAndMessaging::WS_MAXIMIZEBOX.0;
        assert_eq!(CONTROL_CENTER_STYLE.0 & forbidden, 0);
        assert_eq!(
            CONTROL_CENTER_STYLE.0 & windows::Win32::UI::WindowsAndMessaging::WS_POPUP.0,
            windows::Win32::UI::WindowsAndMessaging::WS_POPUP.0
        );
        assert!(blocked_fixed_window_command(0xF000));
        assert!(blocked_fixed_window_command(0xF020));
        assert!(blocked_fixed_window_command(0xF030));
        assert!(blocked_fixed_window_command(0xF120));
        assert!(!blocked_fixed_window_command(0xF060));

        let chrome = crate::ui::layout::top_chrome_geometry(layout.width, layout.nav_width);
        assert_eq!(layout.search_rect.y, chrome.close.y);
        assert_eq!(layout.search_rect.h, chrome.close.h);
        assert_eq!(
            chrome_hit_test_dip(
                chrome,
                chrome.search.x + chrome.search.w * 0.5,
                chrome.search.y + chrome.search.h * 0.5,
            ),
            HTCLIENT
        );
        assert_eq!(
            chrome_hit_test_dip(
                chrome,
                chrome.caption.x + chrome.caption.w * 0.5,
                chrome.caption.y + chrome.caption.h * 0.5,
            ),
            HTCAPTION
        );
        assert_eq!(
            chrome_hit_test_dip(chrome, chrome.row.x + 1.0, chrome.row.y + 1.0),
            HTCLIENT
        );
        assert_eq!(
            chrome_hit_test_dip(chrome, 1.0, layout.height - 1.0),
            HTCLIENT
        );
    }
    #[test]
    fn preview_work_area_aspect_uses_real_bounds_and_16_9_fallback() {
        assert_eq!(work_area_aspect(None), (16, 9));
        assert_eq!(
            work_area_aspect(Some(RECT {
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1200,
            })),
            (1920, 1200)
        );
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
            SettingsWheelAction::Scroll(scroll) if (scroll - 48.0).abs() < f32::EPSILON
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
            || steps.borrow_mut().push("discard"),
            || steps.borrow_mut().push("settings"),
        );
        assert_eq!(&*steps.borrow(), &["picker", "discard", "settings"]);
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

    fn sample_profile(id: &str, name: &str, confirmed: bool) -> crate::display::DisplayProfile {
        crate::display::DisplayProfile {
            id: id.into(),
            name: name.into(),
            topology: crate::display::DisplayTopology::Extend,
            confirmed,
            routes: vec![crate::display::DisplayRoute {
                target_path: format!("target-{id}"),
                source_width: 1920,
                source_height: 1080,
                refresh_numerator: 60,
                refresh_denominator: 1,
                ..Default::default()
            }],
        }
    }
    #[test]
    fn display_topology_changes_stay_local_until_explicit_keep() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("display", "Display", true)];
        ui.draft.display_profiles.active_profile = Some("display".into());

        ui.apply_picker(
            PickerKind::DisplayTopology,
            PickerValue::DisplayTopology(crate::display::DisplayTopology::Clone),
        );

        assert!(ui.display_draft_dirty);
        assert_eq!(
            ui.draft.display_profiles.active().unwrap().topology,
            crate::display::DisplayTopology::Clone
        );
        ui.replace_draft(Config::default());
        assert!(!ui.display_draft_dirty);
    }

    #[test]
    fn display_profile_rename_preserves_id_confirmation_and_hotkey_reference() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("a4c", "Gaming", true)];
        ui.draft.display_profiles.active_profile = Some("a4c".into());
        let hotkey = Hotkey::parse("Ctrl+Alt+F1").unwrap();
        ui.draft
            .hotkeys
            .display_profiles
            .push(crate::config::model::DisplayProfileHotkey {
                profile_id: "a4c".into(),
                hotkey,
            });

        ui.rename_display_profile("a4c", "  Gaming 240Hz  ");

        let profile = ui.draft.display_profiles.active().unwrap();
        assert_eq!(profile.id, "a4c");
        assert_eq!(profile.name, "Gaming 240Hz");
        assert!(profile.confirmed);
        assert_eq!(ui.draft.hotkeys.display_profiles[0].profile_id, "a4c");
    }

    #[test]
    fn display_profile_duplicate_gets_new_id_without_hotkey_or_confirmation() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("gaming", "Gaming", true)];
        ui.draft.display_profiles.active_profile = Some("gaming".into());
        ui.draft
            .hotkeys
            .display_profiles
            .push(crate::config::model::DisplayProfileHotkey {
                profile_id: "gaming".into(),
                hotkey: Hotkey::parse("Ctrl+Alt+F2").unwrap(),
            });

        ui.duplicate_active_display_profile();

        let duplicate = ui.draft.display_profiles.active().unwrap();
        assert_ne!(duplicate.id, "gaming");
        assert_eq!(duplicate.name, "Gaming Copy");
        assert!(!duplicate.confirmed);
        assert_eq!(ui.draft.hotkeys.display_profiles.len(), 1);
        assert_eq!(ui.draft.hotkeys.display_profiles[0].profile_id, "gaming");
    }

    #[test]
    fn display_profile_delete_removes_hotkey_and_selects_remaining_profile() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![
            sample_profile("first", "First", true),
            sample_profile("second", "Second", true),
        ];
        ui.draft.display_profiles.active_profile = Some("first".into());
        ui.draft
            .hotkeys
            .display_profiles
            .push(crate::config::model::DisplayProfileHotkey {
                profile_id: "first".into(),
                hotkey: Hotkey::parse("Ctrl+Alt+F3").unwrap(),
            });
        ui.draft.hotkeys.set_disabled_hotkey(
            "display_profile:first".into(),
            Hotkey::parse("Ctrl+Alt+F4").unwrap(),
        );

        ui.delete_active_display_profile();

        assert_eq!(
            ui.draft.display_profiles.active_profile.as_deref(),
            Some("second")
        );
        assert!(ui.draft.display_profiles.active().is_some());
        assert!(ui.draft.hotkeys.display_profiles.is_empty());
        assert!(ui
            .draft
            .hotkeys
            .disabled_hotkey("display_profile:first")
            .is_none());
    }

    #[test]
    fn display_route_editor_changes_supported_values_and_untrusts_profile() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("work", "Work", true)];
        ui.draft.display_profiles.active_profile = Some("work".into());

        ui.edit_display_route("work", 0, "-1920,0,2560,1440,144,90");

        let route = &ui.draft.display_profiles.active().unwrap().routes[0];
        assert_eq!(route.source_position_x, -1920);
        assert_eq!(route.source_width, 2560);
        assert_eq!(route.source_height, 1440);
        assert_eq!(route.refresh_numerator, 144);
        assert_eq!(route.rotation, 2);
        assert!(!ui.draft.display_profiles.active().unwrap().confirmed);
    }

    #[test]
    fn display_route_editor_rejects_unsupported_values() {
        assert!(parse_display_route_values("0,0,1920,1080,60,45").is_err());
        assert!(parse_display_route_values("0,0,0,1080,60,0").is_err());
        assert!(parse_display_route_values("0,0,1920,1080,60,0,extra").is_err());
    }
    #[test]
    fn display_route_editor_preserves_refresh_rate_rationals() {
        assert_eq!(
            parse_display_route_values("0,0,1920,1080,60000/1001,0"),
            Ok((0, 0, 1920, 1080, 60000, 1001, 1))
        );
        assert!(parse_display_route_values("0,0,1920,1080,60/0,0").is_err());
        assert!(parse_display_route_values("0,0,1920,1080,60/1001/2,0").is_err());
    }

    #[test]
    fn profile_hotkey_capture_and_clear_uses_stable_profile_id() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("ai-id", "AI", true)];
        ui.draft.display_profiles.active_profile = Some("ai-id".into());
        let hotkey = Hotkey::parse("Ctrl+Alt+F4").unwrap();

        ui.recording = Some(ElementId::DisplayProfileHotkey);
        ui.finish_recording(crate::keyboard::hook::CapturedChord {
            modifiers: hotkey.modifiers,
            key: Some(hotkey.key),
        });
        assert_eq!(ui.draft.hotkeys.display_profiles.len(), 1);
        assert_eq!(ui.draft.hotkeys.display_profiles[0].profile_id, "ai-id");
        assert_eq!(ui.draft.hotkeys.display_profiles[0].hotkey, hotkey);

        ui.recording = Some(ElementId::DisplayProfileHotkey);
        ui.finish_recording(crate::keyboard::hook::CapturedChord {
            modifiers: ModifierMask::NONE,
            key: Some(VirtualKey(0x2E)),
        });
        assert!(ui.draft.hotkeys.display_profiles.is_empty());
    }

    #[test]
    fn disabled_shortcut_re_recording_keeps_it_disabled() {
        let mut ui = empty_settings_ui();
        let replacement = Hotkey::parse("Ctrl+Alt+F20").unwrap();
        ui.draft
            .hotkeys
            .set_disabled_hotkey("cycle_output_device".into(), replacement);

        ui.set_recorded_hotkey(
            HotkeySlot::CycleOutput,
            Hotkey::parse("Ctrl+Alt+F21").unwrap(),
        );

        assert!(ui.draft.hotkeys.cycle_output_device.is_none());
        assert_eq!(
            ui.draft.hotkeys.disabled_hotkey("cycle_output_device"),
            Some(Hotkey::parse("Ctrl+Alt+F21").unwrap())
        );
        assert_eq!(
            ui.configured_hotkey(HotkeySlot::CycleOutput),
            Some(Hotkey::parse("Ctrl+Alt+F21").unwrap())
        );
        assert!(!ui.hotkey_enabled(HotkeySlot::CycleOutput));
        assert!(!ui.is_disabled(ElementId::HotkeyEnabled(HotkeySlot::CycleOutput)));
    }

    #[test]
    fn unassigning_a_shortcut_removes_active_and_disabled_copies() {
        let mut ui = empty_settings_ui();
        let hotkey = Hotkey::parse("Ctrl+Alt+F20").unwrap();
        ui.draft.hotkeys.cycle_output_device = Some(hotkey);
        ui.draft
            .hotkeys
            .set_disabled_hotkey("cycle_output_device".into(), hotkey);
        let action = ui.hotkey_action(HotkeySlot::CycleOutput).unwrap();

        ui.set_active_hotkey(HotkeySlot::CycleOutput, None);
        ui.draft.hotkeys.clear_disabled_hotkey(&action);

        assert!(ui.draft.hotkeys.cycle_output_device.is_none());
        assert!(ui.draft.hotkeys.disabled_hotkey(&action).is_none());
        assert!(ui.is_disabled(ElementId::HotkeyUnassign(HotkeySlot::CycleOutput)));
    }

    #[test]
    fn normal_special_workspace_summary_uses_available_without_ready_badge() {
        let mut ui = empty_settings_ui();
        ui.runtime.desktop.native = crate::desktop::BackendAvailability::Available;

        let (status, detail, action) = ui.special_workspace_summary();

        assert_eq!(status, "Available");
        assert!(!detail.is_empty());
        assert_eq!(action, "Open");
    }

    #[test]
    fn dirty_display_edits_block_other_changes_without_mutating_draft() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = empty_settings_ui();
        ui.display_draft_dirty = true;
        let before = ui.draft.clone();
        ui.draft.overlay.enabled = !ui.draft.overlay.enabled;

        assert!(!ui.commit_local_change(hwnd, before.clone()));
        assert_eq!(ui.draft, before);
        assert!(ui
            .validation
            .iter()
            .any(|violation| violation.field == "Displays"));
    }

    #[test]
    fn reenable_validates_disabled_chord_against_active_bindings() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let mut ui = empty_settings_ui();
        let conflict = ui.draft.hotkeys.toggle_microphone.unwrap();
        ui.draft
            .hotkeys
            .set_disabled_hotkey("cycle_output_device".into(), conflict);

        ui.toggle_hotkey_enabled(hwnd, HotkeySlot::CycleOutput);

        assert!(ui.draft.hotkeys.cycle_output_device.is_none());
        assert_eq!(
            ui.draft.hotkeys.disabled_hotkey("cycle_output_device"),
            Some(conflict)
        );
        assert!(ui
            .validation
            .iter()
            .any(|violation| violation.message.contains("conflicts")));
    }

    #[test]
    fn home_navigation_copy_uses_coherent_open_actions() {
        let ui = empty_settings_ui();
        assert_eq!(ui.special_workspace_summary().2, "Open");
        assert_eq!(ui.shortcut_health_copy().2, "Open");
        match ui.value_for(ElementId::HomeDiagnostics) {
            ControlValue::Action(value) => assert_eq!(value, "Open"),
            _ => panic!("expected Home diagnostics action"),
        }
        match ui.value_for(ElementId::DiagnosticsStatus) {
            ControlValue::Action(value) => assert_eq!(value, "Open"),
            _ => panic!("expected diagnostics action"),
        }
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
    fn display_inventory_failure_is_reported_as_unknown_not_unavailable() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("unknown", "Baseline", true)];
        ui.draft.display_profiles.active_profile = Some("unknown".into());
        ui.display_inventory_error = Some("inventory unavailable".into());
        ui.display_inventory_loaded = true;
        let (_, detail) = ui.display_summary();
        assert!(detail.contains("Readiness unknown"));
        assert!(!detail.contains("route(s) unavailable"));
    }

    #[test]
    fn home_display_summary_does_not_assume_inventory_before_first_refresh() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("pending", "Baseline", true)];
        ui.draft.display_profiles.active_profile = Some("pending".into());
        let (_, detail) = ui.display_summary();
        assert!(detail.contains("Readiness pending"));
        assert!(!detail.contains("unavailable"));
    }

    #[test]
    fn home_shortcut_health_explains_pause_instead_of_claiming_activity() {
        let mut ui = empty_settings_ui();
        ui.draft.general.start_hotkeys_enabled = false;
        let (value, detail, _) = ui.shortcut_health_copy();
        assert_eq!(value, "Shortcuts paused");
        assert!(detail.contains("configured"));
        assert!(!value.contains("active"));
    }

    #[test]
    fn special_workspace_off_state_exposes_enable_recovery_action() {
        let mut ui = empty_settings_ui();
        ui.draft.virtual_desktops.enabled = false;
        assert_eq!(
            ui.special_workspace_summary(),
            (
                "Off".to_string(),
                "Workspaces are off".to_string(),
                "Enable".to_string(),
            )
        );
    }

    #[test]
    fn special_workspace_distinguishes_disabled_from_backend_unavailable() {
        let mut ui = empty_settings_ui();
        ui.draft.virtual_desktops.enabled = true;
        let (status, detail, action) = ui.special_workspace_summary();
        assert_eq!(status, "Unavailable");
        assert!(detail.contains("service"));
        assert_eq!(action, "Open");
        ui.draft.virtual_desktops.enabled = false;
        assert_eq!(ui.special_workspace_summary().0, "Off");
    }

    #[test]
    fn home_status_names_display_profile_warning_truthfully() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("status", "Baseline", true)];
        ui.draft.display_profiles.active_profile = Some("status".into());
        ui.display_inventory_error = Some("display query failed".into());
        ui.display_inventory_loaded = true;
        let (title, value, detail) = ui.home_diagnostics_copy();
        assert_eq!(title, "Display profile needs attention");
        assert!(value.contains("Readiness unknown"));
        assert!(detail.contains("display information"));
    }

    #[test]
    fn display_test_remains_enabled_when_backend_revalidates_inventory() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("test", "Baseline", true)];
        ui.draft.display_profiles.active_profile = Some("test".into());
        ui.display_editor = Some(DisplayEditorState {
            step: DisplayWizardStep::Review,
        });
        ui.display_inventory_error = Some("display query failed".into());
        ui.display_inventory_loaded = true;
        assert!(!ui.is_disabled(ElementId::TestApplyDisplayProfile));
        assert!(ui.layout_context().display_inventory_unknown);
        assert!(ui
            .display_review_readiness(ui.draft.display_profiles.active().unwrap())
            .starts_with("Unknown"));
    }

    #[test]
    fn display_review_summary_contains_draft_identity_and_readiness() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("review", "Baseline", true)];
        ui.draft.display_profiles.active_profile = Some("review".into());
        ui.display_inventory_error = Some("display query failed".into());
        ui.display_inventory_loaded = true;
        let profile = ui.draft.display_profiles.active().unwrap().clone();
        assert_eq!(ui.display_review_screen_names(&profile), "Saved screen 1");
        assert!(ui
            .display_review_readiness(&profile)
            .contains("Windows display information unavailable"));
    }

    #[test]
    fn display_readiness_marks_unavailable_screens_as_attention() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("missing", "Baseline", true)];
        ui.display_inventory_loaded = true;
        ui.draft.display_profiles.active_profile = Some("missing".into());
        assert!(ui.display_profile_needs_attention());
        let (_, detail) = ui.display_summary();
        assert!(detail.contains("Needs attention"));
        assert!(detail.contains("unavailable"));
    }

    #[test]
    fn display_editor_back_and_next_move_one_step_without_applying() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("wizard", "Wizard", true)];
        ui.draft.display_profiles.active_profile = Some("wizard".into());
        ui.display_editor = Some(DisplayEditorState {
            step: DisplayWizardStep::Displays,
        });
        ui.move_display_editor(HWND::default(), true);
        assert_eq!(
            ui.display_editor.expect("editor").step,
            DisplayWizardStep::Arrangement
        );
        assert_eq!(
            ui.draft
                .display_profiles
                .active()
                .expect("profile")
                .topology,
            crate::display::DisplayTopology::Extend
        );
        ui.move_display_editor(HWND::default(), false);
        assert_eq!(
            ui.display_editor.expect("editor").step,
            DisplayWizardStep::Displays
        );
    }

    #[test]
    fn display_editor_requires_a_route_before_advancing() {
        let mut ui = empty_settings_ui();
        ui.display_editor = Some(DisplayEditorState {
            step: DisplayWizardStep::Displays,
        });
        ui.move_display_editor(HWND::default(), true);
        assert_eq!(
            ui.display_editor.expect("editor").step,
            DisplayWizardStep::Displays
        );
        assert!(ui
            .validation
            .iter()
            .any(|violation| violation.message.contains("at least one")));
    }

    #[test]
    fn delete_profile_requires_a_second_confirmation() {
        let mut ui = empty_settings_ui();
        ui.draft.display_profiles.profiles = vec![sample_profile("delete", "Delete me", true)];
        ui.draft.display_profiles.active_profile = Some("delete".into());
        ui.delete_profile_confirm = false;
        assert!(!ui.is_disabled(ElementId::DeleteDisplayProfile));
        assert!(!SettingsUi::consume_reset_confirmation(
            &mut ui.delete_profile_confirm
        ));
        assert!(ui.delete_profile_confirm);
        assert!(SettingsUi::consume_reset_confirmation(
            &mut ui.delete_profile_confirm
        ));
        assert!(!ui.delete_profile_confirm);
    }
    #[test]
    fn pause_toggle_reports_paused_state_not_enabled_state() {
        let mut ui = empty_settings_ui();
        ui.draft.general.start_hotkeys_enabled = false;
        assert!(matches!(
            ui.value_for(ElementId::StartHotkeysEnabled),
            ControlValue::Toggle(true)
        ));
        ui.draft.general.start_hotkeys_enabled = true;
        assert!(matches!(
            ui.value_for(ElementId::StartHotkeysEnabled),
            ControlValue::Toggle(false)
        ));
    }
    #[test]
    fn applied_status_is_generic() {
        let status = APPLIED_STATUS.to_ascii_lowercase();
        assert!(status.contains("applied"));
        assert!(!status.contains("hotkey"));
    }
}
