//! Native Diagnostics & Support window.
//!
//! The page is deliberately separate from Settings. It renders only copied
//! [`DiagnosticsSnapshot`] data; actions return to the main App through the
//! existing typed event queue.

use std::sync::OnceLock;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SetWindowPos, ShowWindow, CREATESTRUCTW, SWP_NOACTIVATE, SWP_NOZORDER,
    SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_DPICHANGED, WM_ERASEBKGND,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WS_OVERLAPPEDWINDOW,
};

use crate::diagnostics::snapshot::{DiagnosticsSnapshot, Health, SelfTestReport};
use crate::error::{Error, Result};
use crate::platform::window as win;
use crate::ui::controls::{self, Interaction};
use crate::ui::layout::Rect;
use crate::ui::renderer::{rect, BrushRole, Renderer, TextStyle};
use crate::ui::theme::{Theme, ThemeMode};

pub const CLASS_NAME: &str = "WinShort.Diagnostics";
const DESIGN_WIDTH: f32 = 860.0;
const DESIGN_HEIGHT: f32 = 760.0;
const WM_MOUSELEAVE: u32 = 0x02A3;
const FOOTER_HEIGHT: f32 = 104.0;
const HEADER_HEIGHT: f32 = 84.0;
const ROW_HEIGHT: f32 = 28.0;
static REGISTERED: OnceLock<u16> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Copy,
    OpenLogs,
    Bundle,
    SelfTest,
    Close,
}

impl Action {
    const ALL: [Self; 5] = [
        Self::Copy,
        Self::OpenLogs,
        Self::Bundle,
        Self::SelfTest,
        Self::Close,
    ];

    fn label(self, busy: bool) -> &'static str {
        match self {
            Self::Copy => "Copy Diagnostics",
            Self::OpenLogs => "Open Logs",
            Self::Bundle if busy => "Creating…",
            Self::Bundle => "Create Support Bundle",
            Self::SelfTest => "Run Self-Test",
            Self::Close => "Close",
        }
    }
}

#[derive(Debug, Clone)]
struct Line {
    section: bool,
    key: String,
    value: String,
    health: Option<Health>,
}

#[derive(Debug, Clone)]
struct Layout {
    width: f32,
    content: Rect,
    footer: Rect,
    lines: Vec<Line>,
    max_scroll: f32,
    scroll: f32,
    buttons: Vec<(Action, Rect)>,
}

pub struct DiagnosticsWindow {
    pub hwnd: HWND,
}

struct DiagnosticsUi {
    dpi: u32,
    renderer: Option<Renderer>,
    snapshot: DiagnosticsSnapshot,
    self_test: Option<SelfTestReport>,
    action_status: Option<String>,
    bundle_running: bool,
    scroll: f32,
    hovered: Option<Action>,
    pressed: Option<Action>,
    focused: Option<Action>,
    mouse_tracking: bool,
}

impl DiagnosticsUi {
    fn new(dpi: u32, snapshot: DiagnosticsSnapshot) -> Self {
        Self {
            dpi,
            renderer: None,
            snapshot,
            self_test: None,
            action_status: None,
            bundle_running: false,
            scroll: 0.0,
            hovered: None,
            pressed: None,
            focused: None,
            mouse_tracking: false,
        }
    }

    fn set_snapshot(&mut self, snapshot: DiagnosticsSnapshot, self_test: Option<SelfTestReport>) {
        self.snapshot = snapshot;
        self.self_test = self_test;
        self.scroll = 0.0;
    }

    fn lines(&self) -> Vec<Line> {
        let mut lines = Vec::new();
        let section = |lines: &mut Vec<Line>, name: &str| {
            lines.push(Line {
                section: true,
                key: name.into(),
                value: String::new(),
                health: None,
            });
        };
        let row = |lines: &mut Vec<Line>, key: &str, value: String, health: Option<Health>| {
            lines.push(Line {
                section: false,
                key: key.into(),
                value,
                health,
            });
        };
        let s = &self.snapshot;

        section(&mut lines, "Application");
        row(&mut lines, "Version", s.application.version.clone(), None);
        row(&mut lines, "Profile", s.application.profile.clone(), None);
        row(
            &mut lines,
            "Architecture",
            s.application.architecture.clone(),
            None,
        );
        row(
            &mut lines,
            "Windows",
            match (s.windows.build, s.windows.update_revision) {
                (Some(build), Some(ubr)) => format!("Build {build}.{ubr}"),
                (Some(build), None) => format!("Build {build}.?"),
                _ => s
                    .windows
                    .error
                    .clone()
                    .unwrap_or_else(|| "Unavailable".into()),
            },
            s.windows.error.as_ref().map(|_| Health::Unavailable),
        );
        row(
            &mut lines,
            "Windows architecture",
            s.windows.architecture.clone(),
            None,
        );

        section(&mut lines, "Keyboard");
        row(
            &mut lines,
            "Hook",
            if s.keyboard.hook_active {
                "Active"
            } else {
                "Not active"
            }
            .into(),
            Some(s.keyboard.health),
        );
        row(
            &mut lines,
            "Hotkeys",
            if s.keyboard.suspended {
                "Suspended"
            } else {
                "Enabled"
            }
            .into(),
            Some(if s.keyboard.suspended {
                Health::Warning
            } else {
                s.keyboard.health
            }),
        );
        row(
            &mut lines,
            "Recorder",
            if s.keyboard.capture_active {
                "Capturing"
            } else {
                "Idle"
            }
            .into(),
            None,
        );
        for (name, binding) in &s.keyboard.bindings {
            row(&mut lines, name, binding.clone(), None);
        }
        row(
            &mut lines,
            "Win+1..9",
            if s.keyboard.reserved_win_numbers {
                "Reserved by desktop switching".into()
            } else {
                "Not reserved".into()
            },
            None,
        );
        row(
            &mut lines,
            "Validation",
            if s.keyboard.conflicts.is_empty() {
                "No conflicts".into()
            } else {
                compact(&s.keyboard.conflicts.join(" | "))
            },
            Some(if s.keyboard.conflicts.is_empty() {
                Health::Healthy
            } else {
                Health::Error
            }),
        );

        section(&mut lines, "Audio");
        row(
            &mut lines,
            "Input",
            endpoint_value(&s.audio.input),
            Some(s.audio.input.health),
        );
        row(
            &mut lines,
            "Output",
            endpoint_value(&s.audio.output),
            Some(s.audio.output.health),
        );
        row(
            &mut lines,
            "Microphone",
            s.audio.microphone_state.clone(),
            None,
        );
        row(
            &mut lines,
            "Output state",
            s.audio.output_state.clone(),
            None,
        );
        row(
            &mut lines,
            "Foreground audio",
            format!(
                "{} · {} sessions{}",
                s.audio.foreground.aggregate,
                s.audio.foreground.sessions,
                s.audio
                    .foreground
                    .app_name
                    .as_deref()
                    .map_or_else(String::new, |name| format!(" · {name}"))
            ),
            Some(s.audio.foreground.health),
        );
        if let Some(error) = &s.audio.foreground.error {
            row(
                &mut lines,
                "Foreground result",
                compact(error),
                Some(Health::Error),
            );
        }

        section(&mut lines, "Virtual desktops");
        row(
            &mut lines,
            "Native backend",
            s.desktop.native.clone(),
            Some(s.desktop.health),
        );
        row(&mut lines, "Fallback", s.desktop.fallback.clone(), None);
        row(&mut lines, "Active backend", s.desktop.active.clone(), None);
        row(
            &mut lines,
            "Last served",
            s.desktop.last_served.clone(),
            None,
        );
        row(
            &mut lines,
            "Desktop count",
            s.desktop
                .desktop_count
                .map_or_else(|| "Unavailable".into(), |count| count.to_string()),
            None,
        );
        row(
            &mut lines,
            "Detected build",
            match (s.desktop.build, s.desktop.update_revision) {
                (Some(build), Some(ubr)) => format!("{build}.{ubr}"),
                (Some(build), None) => format!("{build}.?"),
                _ => "Unavailable".into(),
            },
            None,
        );
        if let Some(error) = &s.desktop.error {
            row(
                &mut lines,
                "Native detail",
                compact(error),
                Some(Health::Warning),
            );
        }

        section(&mut lines, "Config");
        row(
            &mut lines,
            "Status",
            s.config.health.label().into(),
            Some(s.config.health),
        );
        row(
            &mut lines,
            "Path",
            compact(&s.config.path.to_string_lossy()),
            None,
        );
        row(
            &mut lines,
            "Schema",
            s.config.schema_version.to_string(),
            None,
        );
        row(
            &mut lines,
            "Future-schema latch",
            if s.config.read_only {
                "Read-only"
            } else {
                "Not active"
            }
            .into(),
            Some(if s.config.read_only {
                Health::Error
            } else {
                Health::Healthy
            }),
        );
        row(
            &mut lines,
            "Configured hotkeys",
            s.config.hotkey_count.to_string(),
            None,
        );
        row(
            &mut lines,
            "Load warnings",
            s.config.warnings.len().to_string(),
            None,
        );
        row(
            &mut lines,
            "Repaired fields",
            s.config.repaired_fields.len().to_string(),
            None,
        );
        row(
            &mut lines,
            "Migrations",
            s.config.migrations.len().to_string(),
            None,
        );
        row(
            &mut lines,
            "Validation",
            if s.config.validation.is_empty() {
                "Valid".into()
            } else {
                compact(&s.config.validation.join(" | "))
            },
            Some(if s.config.validation.is_empty() {
                Health::Healthy
            } else {
                Health::Error
            }),
        );

        section(&mut lines, "Overlay");
        row(
            &mut lines,
            "Status",
            s.overlay.health.label().into(),
            Some(s.overlay.health),
        );
        row(
            &mut lines,
            "Enabled",
            if s.overlay.enabled { "Yes" } else { "No" }.into(),
            None,
        );
        row(&mut lines, "Position", s.overlay.position.clone(), None);
        row(
            &mut lines,
            "Monitor selector",
            s.overlay.monitor_selector.clone(),
            None,
        );
        row(
            &mut lines,
            "Last target monitor",
            s.overlay
                .target_monitor
                .clone()
                .unwrap_or_else(|| "Never".into()),
            None,
        );
        row(
            &mut lines,
            "Last render DPI",
            s.overlay
                .render_dpi
                .map_or_else(|| "Never".into(), |dpi| dpi.to_string()),
            None,
        );
        row(
            &mut lines,
            "Last shown",
            if s.overlay.last_shown.is_some() {
                "Available".into()
            } else {
                "Never".into()
            },
            None,
        );

        section(&mut lines, "Startup and logging");
        row(
            &mut lines,
            "Startup registration",
            s.startup.state.clone(),
            Some(s.startup.health),
        );
        if let Some(command) = &s.startup.registered_command {
            row(&mut lines, "Registered command", compact(command), None);
        }
        row(
            &mut lines,
            "Logging",
            s.logging.health.label().into(),
            Some(s.logging.health),
        );
        row(
            &mut lines,
            "Log directory",
            s.logging.directory.as_ref().map_or_else(
                || "Unavailable".into(),
                |path| compact(&path.to_string_lossy()),
            ),
            None,
        );
        row(&mut lines, "Log level", s.logging.level.clone(), None);
        if let Some(error) = &s.startup.error {
            row(
                &mut lines,
                "Startup detail",
                compact(error),
                Some(Health::Error),
            );
        }
        row(
            &mut lines,
            "Current log file",
            s.logging.current_file.as_ref().map_or_else(
                || "Unavailable".into(),
                |path| compact(&path.to_string_lossy()),
            ),
            None,
        );

        section(&mut lines, "Degraded subsystems");
        if s.degraded.is_empty() {
            row(
                &mut lines,
                "Runtime",
                "No degraded subsystems".into(),
                Some(Health::Healthy),
            );
        } else {
            for degraded in &s.degraded {
                row(
                    &mut lines,
                    &degraded.name,
                    compact(&degraded.reason),
                    Some(Health::Unavailable),
                );
            }
        }

        if let Some(report) = &self.self_test {
            section(&mut lines, "Self-test");
            row(&mut lines, "Summary", report.summary(), None);
            for check in &report.checks {
                row(
                    &mut lines,
                    &check.name,
                    compact(&check.detail),
                    Some(check.health),
                );
            }
        }
        lines
    }

    fn layout(&self, hwnd: HWND) -> Layout {
        let (width, height) = self
            .renderer
            .as_ref()
            .map(Renderer::client_size_dip)
            .unwrap_or_else(|| client_size_dip(hwnd, self.dpi));
        let content = Rect::new(
            0.0,
            HEADER_HEIGHT,
            width,
            (height - HEADER_HEIGHT - FOOTER_HEIGHT).max(80.0),
        );
        let lines = self.lines();
        let raw_height: f32 = lines
            .iter()
            .map(|line| if line.section { 34.0 } else { ROW_HEIGHT })
            .sum();
        let max_scroll = (raw_height - content.h + 12.0).max(0.0);
        let scroll = self.scroll.clamp(0.0, max_scroll);
        let footer = Rect::new(0.0, height - FOOTER_HEIGHT, width, FOOTER_HEIGHT);
        let button_widths = [150.0, 110.0, 166.0, 138.0, 86.0];
        let gap = 10.0;
        let mut x = 24.0;
        let y = footer.y + 48.0;
        let mut buttons = Vec::with_capacity(Action::ALL.len());
        for (action, button_width) in Action::ALL.into_iter().zip(button_widths) {
            buttons.push((action, Rect::new(x, y, button_width, 34.0)));
            x += button_width + gap;
        }
        Layout {
            width,
            content,
            footer,
            lines,
            max_scroll,
            scroll,
            buttons,
        }
    }

    fn paint(&mut self, hwnd: HWND) -> Result<()> {
        let layout = self.layout(hwnd);
        self.scroll = layout.scroll;
        let renderer = match self.renderer.take() {
            Some(renderer) => renderer,
            None => Renderer::new(hwnd, self.dpi, Theme::current())?,
        };
        renderer.begin();
        controls::draw_app_mark(&renderer, Rect::new(24.0, 22.0, 34.0, 34.0));
        renderer.text(
            "WinShort Diagnostics & Support",
            rect(70.0, 16.0, layout.width - 24.0, 44.0),
            TextStyle::Title,
            BrushRole::Text,
        );
        renderer.text(
            "Everything WinShort knows about its current runtime",
            rect(70.0, 44.0, layout.width - 24.0, 68.0),
            TextStyle::Subtitle,
            BrushRole::TextSecondary,
        );

        renderer.push_clip(layout.content.d2d());
        let mut y = layout.content.y + 8.0 - layout.scroll;
        for line in &layout.lines {
            if line.section {
                if y + 30.0 >= layout.content.y && y <= layout.content.bottom() {
                    renderer.text(
                        &line.key,
                        rect(24.0, y, layout.width - 24.0, y + 24.0),
                        TextStyle::Section,
                        BrushRole::Text,
                    );
                    renderer.line(
                        24.0,
                        y + 27.0,
                        layout.width - 24.0,
                        y + 27.0,
                        BrushRole::Border,
                        1.0,
                    );
                }
                y += 34.0;
            } else {
                if y + ROW_HEIGHT >= layout.content.y && y <= layout.content.bottom() {
                    renderer.text(
                        &line.key,
                        rect(32.0, y, 286.0, y + ROW_HEIGHT),
                        TextStyle::BodyStrong,
                        BrushRole::Text,
                    );
                    renderer.text(
                        &compact(&line.value),
                        rect(310.0, y, layout.width - 76.0, y + ROW_HEIGHT),
                        TextStyle::Body,
                        if line.health == Some(Health::Error) {
                            BrushRole::Danger
                        } else {
                            BrushRole::TextSecondary
                        },
                    );
                    if let Some(health) = line.health {
                        let role = match health {
                            Health::Healthy => BrushRole::Success,
                            Health::Warning => BrushRole::Warning,
                            Health::Unavailable => BrushRole::TextDisabled,
                            Health::Error => BrushRole::Danger,
                        };
                        renderer.ellipse(layout.width - 38.0, y + 14.0, 4.0, 4.0, role, true, 0.0);
                    }
                    renderer.line(
                        32.0,
                        y + ROW_HEIGHT - 1.0,
                        layout.width - 32.0,
                        y + ROW_HEIGHT - 1.0,
                        BrushRole::Border,
                        1.0,
                    );
                }
                y += ROW_HEIGHT;
            }
        }
        renderer.pop_clip();

        renderer.fill_rect(layout.footer.d2d(), BrushRole::BackgroundSubtle);
        renderer.line(
            0.0,
            layout.footer.y,
            layout.width,
            layout.footer.y,
            BrushRole::Border,
            1.0,
        );
        let status = self
            .action_status
            .as_deref()
            .unwrap_or("Support output is sanitized before it leaves this window");
        renderer.text(
            status,
            rect(
                24.0,
                layout.footer.y + 12.0,
                layout.width - 24.0,
                layout.footer.y + 34.0,
            ),
            TextStyle::Caption,
            BrushRole::TextSecondary,
        );
        for (action, button) in &layout.buttons {
            let disabled = *action == Action::Bundle && self.bundle_running;
            controls::draw_button(
                &renderer,
                *button,
                action.label(self.bundle_running),
                matches!(action, Action::Copy | Action::Bundle),
                Interaction {
                    hovered: self.hovered == Some(*action),
                    pressed: self.pressed == Some(*action),
                    focused: self.focused == Some(*action),
                    disabled,
                    hover_t: if self.hovered == Some(*action) {
                        1.0
                    } else {
                        0.0
                    },
                    state_t: 0.0,
                },
            );
        }
        let result = renderer.end();
        if result.is_ok() {
            self.renderer = Some(renderer);
        }
        result
    }

    fn action_at(&self, hwnd: HWND, x: f32, y: f32) -> Option<Action> {
        self.layout(hwnd)
            .buttons
            .into_iter()
            .find(|(_, rect)| rect.contains(x, y))
            .map(|(action, _)| action)
    }

    fn activate(&mut self, hwnd: HWND, action: Action) {
        if action == Action::Bundle && self.bundle_running {
            return;
        }
        match action {
            Action::Copy => post_main(crate::event::AppEvent::CopyDiagnostics),
            Action::OpenLogs => post_main(crate::event::AppEvent::OpenDiagnosticsLogs),
            Action::Bundle => post_main(crate::event::AppEvent::CreateSupportBundle),
            Action::SelfTest => post_main(crate::event::AppEvent::RunDiagnosticsSelfTest),
            Action::Close => unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            },
        }
    }

    fn focus_next(&mut self, reverse: bool) {
        let current = self
            .focused
            .and_then(|value| Action::ALL.iter().position(|action| *action == value));
        let mut index = current.unwrap_or(if reverse { 0 } else { Action::ALL.len() - 1 });
        for _ in 0..Action::ALL.len() {
            index = if reverse {
                if index == 0 {
                    Action::ALL.len() - 1
                } else {
                    index - 1
                }
            } else {
                (index + 1) % Action::ALL.len()
            };
            if !(Action::ALL[index] == Action::Bundle && self.bundle_running) {
                self.focused = Some(Action::ALL[index]);
                break;
            }
        }
    }
}

impl DiagnosticsWindow {
    pub fn create(snapshot: DiagnosticsSnapshot) -> Result<Self> {
        let _atom = *REGISTERED.get_or_init(|| {
            win::register_class(CLASS_NAME, Some(diagnostics_wndproc))
                .expect("register diagnostics class")
        });
        let primary = crate::platform::monitor::primary();
        let dpi = primary.as_ref().map_or(96, |monitor| monitor.dpi);
        let scale = dpi as f32 / 96.0;
        let width = (DESIGN_WIDTH * scale) as i32;
        let height = (DESIGN_HEIGHT * scale) as i32;
        let (x, y) = match &primary {
            Some(monitor) => (
                monitor.work.left + ((monitor.work.right - monitor.work.left) - width) / 2,
                monitor.work.top + ((monitor.work.bottom - monitor.work.top) - height) / 2,
            ),
            None => (0, 0),
        };
        let state = Box::new(DiagnosticsUi::new(dpi, snapshot));
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::PCWSTR(windows::core::HSTRING::from(CLASS_NAME).as_ptr()),
                windows::core::PCWSTR(
                    windows::core::HSTRING::from("WinShort Diagnostics & Support").as_ptr(),
                ),
                WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0),
                x,
                y,
                width,
                height,
                None,
                None,
                None,
                Some(Box::into_raw(state).cast()),
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(diagnostics)", &error))?;
        apply_chrome(hwnd, Theme::current());
        Ok(Self { hwnd })
    }

    pub fn show(&mut self) -> Result<()> {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(self.hwnd);
        }
        Ok(())
    }

    pub fn set_snapshot(
        &mut self,
        snapshot: DiagnosticsSnapshot,
        self_test: Option<SelfTestReport>,
    ) {
        if let Some(cell) = unsafe { win::state_cell::<DiagnosticsUi>(self.hwnd) } {
            let mut ui = cell.borrow_mut();
            ui.set_snapshot(snapshot, self_test);
            invalidate(self.hwnd);
        }
    }

    pub fn set_action_status(&mut self, status: String) {
        if let Some(cell) = unsafe { win::state_cell::<DiagnosticsUi>(self.hwnd) } {
            cell.borrow_mut().action_status = Some(status);
            invalidate(self.hwnd);
        }
    }

    pub fn set_bundle_running(&mut self, running: bool) {
        if let Some(cell) = unsafe { win::state_cell::<DiagnosticsUi>(self.hwnd) } {
            cell.borrow_mut().bundle_running = running;
            invalidate(self.hwnd);
        }
    }
}

fn endpoint_value(endpoint: &crate::diagnostics::snapshot::AudioEndpointDiagnostics) -> String {
    let description = endpoint
        .description
        .as_deref()
        .map_or_else(|| "unresolved".into(), compact);
    format!(
        "{} · {} · {description}",
        endpoint.health.label(),
        endpoint.role
    )
}

fn compact(value: &str) -> String {
    const MAX: usize = 92;
    let mut chars = value.chars();
    let compact: String = chars.by_ref().take(MAX).collect();
    if chars.next().is_some() {
        format!("{compact}…")
    } else {
        compact
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
        let preference = DWMWCP_ROUND.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&preference as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        let color = theme.bg;
        let caption = COLORREF(color.r as u32 | ((color.g as u32) << 8) | ((color.b as u32) << 16));
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            (&caption as *const COLORREF).cast(),
            std::mem::size_of::<COLORREF>() as u32,
        );
    }
}

unsafe extern "system" fn diagnostics_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let ui = Box::from_raw(create.lpCreateParams as *mut DiagnosticsUi);
            win::store_state_ptr(hwnd, win::WindowState::new(*ui));
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        let cell = match win::state_cell::<DiagnosticsUi>(hwnd) {
            Some(cell) => cell,
            None => return win::def_proc(hwnd, msg, wparam, lparam),
        };
        if msg == WM_NCDESTROY {
            drop(win::take_state::<DiagnosticsUi>(hwnd));
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                if let Err(error) = cell.borrow_mut().paint(hwnd) {
                    crate::error_!("diagnostics paint failed: {error}");
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
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_DPICHANGED => {
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
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                let theme = Theme::current();
                if let Some(renderer) = cell.borrow_mut().renderer.as_mut() {
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
                let next = ui.action_at(hwnd, x, y);
                if next != ui.hovered {
                    ui.hovered = next;
                    invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                let mut ui = cell.borrow_mut();
                ui.mouse_tracking = false;
                ui.hovered = None;
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let mut ui = cell.borrow_mut();
                let (x, y) = mouse_point(lparam, ui.dpi);
                if let Some(action) = ui.action_at(hwnd, x, y) {
                    if !(action == Action::Bundle && ui.bundle_running) {
                        ui.pressed = Some(action);
                        ui.focused = Some(action);
                        let _ = SetCapture(hwnd);
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
                if let Some(action) = pressed {
                    if ui.action_at(hwnd, x, y) == Some(action) {
                        ui.activate(hwnd, action);
                    }
                }
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let mut ui = cell.borrow_mut();
                let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
                let layout = ui.layout(hwnd);
                ui.scroll = (ui.scroll - delta / 120.0 * 56.0).clamp(0.0, layout.max_scroll);
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = wparam.0 as u16;
                let mut ui = cell.borrow_mut();
                match vk {
                    0x09 => {
                        ui.focus_next(is_shift_down());
                        invalidate(hwnd);
                        LRESULT(0)
                    }
                    0x0D | 0x20 => {
                        if let Some(action) = ui.focused {
                            ui.activate(hwnd, action);
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
            WM_KEYUP | WM_SYSKEYUP => win::def_proc(hwnd, msg, wparam, lparam),
            WM_GETMINMAXINFO => {
                let info =
                    &mut *(lparam.0 as *mut windows::Win32::UI::WindowsAndMessaging::MINMAXINFO);
                let scale = cell.borrow().dpi as f32 / 96.0;
                info.ptMinTrackSize.x = (720.0 * scale) as i32;
                info.ptMinTrackSize.y = (560.0 * scale) as i32;
                LRESULT(0)
            }
            _ => win::def_proc(hwnd, msg, wparam, lparam),
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

fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
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
    let x = (lparam.0 as u32 & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam.0 as u32 >> 16) & 0xFFFF) as u16 as i16 as f32;
    let scale = 96.0 / dpi.max(96) as f32;
    (x * scale, y * scale)
}

fn is_shift_down() -> bool {
    unsafe { (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 }
}
