//! Native, keyboard-accessible picker popups used by Settings.
//!
//! A real LISTBOX is hosted in a small owner window rather than cycling values
//! in the painted Settings row. The native list supplies selection semantics to
//! UI Automation/Narrator while the surrounding Settings surface remains D2D.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    FrameRect, InvalidateRect, SelectObject, SetBkMode, SetTextColor, BACKGROUND_MODE,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DT_END_ELLIPSIS,
    DT_LEFT, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL, HDC, HFONT, HGDIOBJ,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT,
};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_FOCUS, ODS_SELECTED, ODT_LISTBOX,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, IsChild, PostMessageW, SetForegroundWindow, ShowWindow,
    CREATESTRUCTW, MA_ACTIVATE, SW_SHOW, SW_SHOWNA, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
    WM_CLOSE, WM_COMMAND, WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND, WM_KEYDOWN, WM_KILLFOCUS,
    WM_LBUTTONUP, WM_MEASUREITEM, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY,
    WM_PAINT, WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VSCROLL,
};

use crate::config::model::{
    DeviceSelection, EndpointRole, MonitorChoice, OverlayAppearance, OverlayPosition,
};
use crate::error::{Error, Result};
use crate::platform::visual::SystemVisualPreferences;
use crate::platform::window as win;
use crate::ui::theme::{Color, Theme};

const CLASS_NAME: &str = "WinShort.SettingsPicker";
const LB_ADDSTRING: u32 = 0x0180;
const LB_SETCURSEL: u32 = 0x0186;
const LB_GETCURSEL: u32 = 0x0188;
const LBN_SELCHANGE: u16 = 1;
const LB_SETSEL: u32 = 0x0185;
const LB_GETSEL: u32 = 0x0187;
const LB_GETCOUNT: u32 = 0x018A;
const LB_GETSELCOUNT: u32 = 0x0190;
const LB_GETSELITEMS: u32 = 0x0191;
pub const fn loword(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

pub const fn hiword(value: usize) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}
const LBN_DBLCLK: u16 = 2;
const SUBCLASS_ID: usize = 1;
const WM_APP_PICKER_FOCUS_LOST: u32 = WM_APP + 4;
const LB_ITEMFROMPOINT: u32 = 0x01A9;
const WM_MOUSELEAVE: u32 = 0x02A3;
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
static REGISTERED: OnceLock<u16> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    InputDevice,
    OutputDevice,
    InputAllowlist,
    OutputAllowlist,
    DisplayProfile,
    InputRole,
    OutputRole,
    DesktopNumberModifier,
    MoveDesktopModifier,
    SilentMoveDesktopModifier,
    OverlayPosition,
    OverlayAppearance,
    OverlayMonitor,
}

impl PickerKind {
    pub const fn is_multi_select(self) -> bool {
        matches!(self, Self::InputAllowlist | Self::OutputAllowlist)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerValue {
    /// `None` means all active endpoints; `Some(empty)` means none.
    Allowlist(Option<Vec<String>>),
    Device(DeviceSelection),
    Role(EndpointRole),
    DisplayProfile(Option<String>),
    Modifier(crate::keyboard::binding::ModifierMask),
    Position(OverlayPosition),
    Appearance(OverlayAppearance),
    Monitor(MonitorChoice),
}

#[derive(Debug, Clone)]
pub struct PickerChoice {
    pub label: String,
    pub value: PickerValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PopupRect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// Place a popup below its anchor when possible, otherwise above it, then clamp
/// the complete rectangle to the monitor work area. Pure geometry is kept out
/// of Win32 calls so DPI/negative-coordinate behavior is testable.
pub fn place_popup(anchor: PopupRect, work: PopupRect, width: i32, height: i32) -> PopupRect {
    let width = width.min(work.width()).max(1);
    let height = height.min(work.height()).max(1);
    // Keep the attachment edge on the control when a wider popup cannot fit
    // to its right; final clamping still handles monitor edges.
    let mut left = if anchor.left + width <= work.right {
        anchor.left
    } else {
        anchor.right - width
    };
    let mut top = if work.bottom - anchor.bottom >= height {
        anchor.bottom
    } else {
        anchor.top - height
    };
    left = left.clamp(work.left, work.right - width);
    top = top.clamp(work.top, work.bottom - height);
    PopupRect::new(left, top, left + width, top + height)
}

pub struct PickerPopup {
    /// Popup owner HWND used for lifecycle and deferred close routing.
    pub hwnd: HWND,
    /// Native LISTBOX HWND that owns keyboard focus while open.
    pub list: HWND,
}
impl Drop for PickerPopup {
    fn drop(&mut self) {
        if !self.hwnd.0.is_null() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerCloseAction {
    Commit,
    Cancel,
}

struct PickerUi {
    generation: u64,
    kind: PickerKind,
    choices: Vec<PickerChoice>,
    list: HWND,
    close_action: Option<PickerCloseAction>,
    hovered_index: Option<usize>,
    font: HFONT,
}

impl Drop for PickerUi {
    fn drop(&mut self) {
        if !self.font.is_invalid() {
            unsafe {
                let _ = DeleteObject(self.font.into());
            }
        }
    }
}

const PICKER_FONT_SIZE_DIP: f32 = 14.0;
const PICKER_FONT_FAMILY: &str = "Segoe UI Variable Text";
const PICKER_FONT_FALLBACK: &str = "Segoe UI";

fn picker_font_height(dpi: u32) -> i32 {
    -((PICKER_FONT_SIZE_DIP * dpi.max(96) as f32 / 96.0).round() as i32).max(1)
}

fn create_picker_font(dpi: u32) -> HFONT {
    let height = picker_font_height(dpi);
    let create = |family: &str| {
        let family = HSTRING::from(family);
        unsafe {
            CreateFontW(
                height,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
                PCWSTR(family.as_ptr()),
            )
        }
    };
    let font = create(PICKER_FONT_FAMILY);
    if font.is_invalid() {
        create(PICKER_FONT_FALLBACK)
    } else {
        font
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
struct PickerColors {
    background: Color,
    foreground: Color,
    border: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerItemState {
    Idle,
    Hovered,
    Selected,
    Disabled,
}

fn picker_item_state(selected: bool, hovered: bool, disabled: bool) -> PickerItemState {
    if disabled {
        PickerItemState::Disabled
    } else if selected {
        PickerItemState::Selected
    } else if hovered {
        PickerItemState::Hovered
    } else {
        PickerItemState::Idle
    }
}

fn picker_colors(selected: bool) -> PickerColors {
    picker_colors_for(selected, SystemVisualPreferences::query(), Theme::current())
}

fn picker_colors_for(
    selected: bool,
    visual: SystemVisualPreferences,
    theme: Theme,
) -> PickerColors {
    picker_colors_for_state(picker_item_state(selected, false, false), visual, theme)
}

fn picker_colors_for_state(
    state: PickerItemState,
    visual: SystemVisualPreferences,
    theme: Theme,
) -> PickerColors {
    if visual.high_contrast {
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
        let highlight_foreground = Color::rgb(
            visual.high_contrast_highlight_foreground.r,
            visual.high_contrast_highlight_foreground.g,
            visual.high_contrast_highlight_foreground.b,
        );
        return match state {
            PickerItemState::Selected => PickerColors {
                background: highlight,
                foreground: highlight_foreground,
                border: foreground,
            },
            PickerItemState::Idle | PickerItemState::Hovered | PickerItemState::Disabled => {
                PickerColors {
                    background,
                    foreground,
                    border: foreground,
                }
            }
        };
    }
    match state {
        PickerItemState::Idle => PickerColors {
            background: theme.card,
            foreground: theme.text,
            border: theme.border_strong,
        },
        PickerItemState::Hovered => PickerColors {
            background: theme.picker_hover,
            foreground: theme.text,
            border: theme.border_strong,
        },
        PickerItemState::Selected => PickerColors {
            background: theme.accent,
            foreground: theme.accent_text,
            border: theme.accent_hover,
        },
        PickerItemState::Disabled => PickerColors {
            background: theme.card_pressed,
            foreground: theme.text_disabled,
            border: theme.border,
        },
    }
}

fn to_colorref(color: Color) -> COLORREF {
    COLORREF(color.r as u32 | (color.g as u32) << 8 | (color.b as u32) << 16)
}

unsafe fn draw_picker_surface(hwnd: HWND, hdc: HDC) {
    let mut rect = RECT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
        let colors = picker_colors(false);
        let background = CreateSolidBrush(to_colorref(colors.background));
        let border = CreateSolidBrush(to_colorref(colors.border));
        let _ = FillRect(hdc, &rect, background);
        let _ = FrameRect(hdc, &rect, border);
        let _ = DeleteObject(HGDIOBJ(background.0));
        let _ = DeleteObject(HGDIOBJ(border.0));
    }
}

unsafe fn draw_picker_item(
    item: &DRAWITEMSTRUCT,
    label: &str,
    hovered: bool,
    font: HFONT,
) -> LRESULT {
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    let focus = item.itemState.0 & ODS_FOCUS.0 != 0;
    let state = picker_item_state(selected, hovered, false);
    let colors = picker_colors_for_state(state, SystemVisualPreferences::query(), Theme::current());
    let background = unsafe { CreateSolidBrush(to_colorref(colors.background)) };
    let border = unsafe { CreateSolidBrush(to_colorref(colors.border)) };
    let _ = unsafe { FillRect(item.hDC, &item.rcItem, background) };
    let mut text_rect = item.rcItem;
    text_rect.left += 12;
    text_rect.right -= 12;
    let mut text = label.encode_utf16().collect::<Vec<_>>();
    let old_font = if !font.is_invalid() {
        unsafe { SelectObject(item.hDC, font.into()) }
    } else {
        HGDIOBJ::default()
    };
    unsafe {
        let _ = SetBkMode(item.hDC, BACKGROUND_MODE(1));
        let _ = SetTextColor(item.hDC, to_colorref(colors.foreground));
        let _ = DrawTextW(
            item.hDC,
            &mut text,
            &mut text_rect,
            DT_END_ELLIPSIS | DT_LEFT | DT_SINGLELINE | DT_VCENTER,
        );
        if focus || hovered {
            let _ = FrameRect(item.hDC, &item.rcItem, border);
        }
        if !old_font.is_invalid() {
            let _ = SelectObject(item.hDC, old_font);
        }
        let _ = DeleteObject(HGDIOBJ(background.0));
        let _ = DeleteObject(HGDIOBJ(border.0));
    }
    LRESULT(1)
}

impl PickerPopup {
    pub fn create(
        parent: HWND,
        kind: PickerKind,
        choices: Vec<PickerChoice>,
        current: usize,
        selected_indices: &[usize],
        geometry: PopupRect,
    ) -> Result<Self> {
        let _atom = *REGISTERED.get_or_init(|| {
            win::register_class(CLASS_NAME, Some(picker_wndproc)).expect("register picker class")
        });
        let state = Box::new(PickerUi {
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
            kind,
            choices,
            list: HWND::default(),
            close_action: None,
            hovered_index: None,
            font: HFONT::default(),
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0),
                PCWSTR(HSTRING::from(CLASS_NAME).as_ptr()),
                PCWSTR(HSTRING::from("").as_ptr()),
                WINDOW_STYLE(WS_POPUP.0),
                geometry.left,
                geometry.top,
                geometry.width(),
                geometry.height(),
                Some(parent),
                None,
                None,
                Some(Box::into_raw(state).cast()),
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(settings picker)", &error))?;

        let list_style = WINDOW_STYLE(
            WS_CHILD.0
                | WS_TABSTOP.0
                | WS_VSCROLL.0
                | 0x0001 // LBS_NOTIFY
                | 0x0010 // LBS_OWNERDRAWFIXED
                | 0x0040 // LBS_HASSTRINGS
                | if kind.is_multi_select() { 0x0008 } else { 0 }, // LBS_MULTIPLESEL
        );
        let list = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(HSTRING::from("LISTBOX").as_ptr()),
                PCWSTR(HSTRING::from("").as_ptr()),
                list_style,
                1,
                1,
                geometry.width() - 2,
                geometry.height() - 2,
                Some(hwnd),
                None,
                None,
                None,
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(settings picker list)", &error))?;

        let font =
            create_picker_font(unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96));
        let (labels, selected) = {
            let Some(cell) = (unsafe { win::state_cell::<PickerUi>(hwnd) }) else {
                if !font.is_invalid() {
                    unsafe {
                        let _ = DeleteObject(font.into());
                    }
                }
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(Error::internal("settings picker state missing"));
            };
            let mut ui = cell.borrow_mut();
            ui.list = list;
            ui.font = font;
            let labels = ui
                .choices
                .iter()
                .map(|choice| choice.label.clone())
                .collect::<Vec<_>>();
            let selected = current.min(ui.choices.len().saturating_sub(1));
            (labels, selected)
        };

        for label in labels {
            let text = HSTRING::from(label);
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_ADDSTRING,
                    Some(WPARAM(0)),
                    Some(LPARAM(text.as_ptr() as isize)),
                );
            }
        }
        unsafe {
            if kind.is_multi_select() {
                for index in selected_indices {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        list,
                        LB_SETSEL,
                        Some(WPARAM(1)),
                        Some(LPARAM(*index as isize)),
                    );
                }
            } else {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_SETCURSEL,
                    Some(WPARAM(selected)),
                    Some(LPARAM(0)),
                );
            }
            let _ = SetWindowSubclass(
                list,
                Some(picker_list_subclass),
                SUBCLASS_ID,
                hwnd.0 as usize,
            );
        }
        Ok(Self { hwnd, list })
    }

    pub fn activate(&self) {
        unsafe {
            let _ = ShowWindow(self.list, SW_SHOWNA);
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.list));
        }
    }
}

fn picker_focus_snapshot(cell: &std::cell::RefCell<PickerUi>) -> (HWND, u64) {
    let ui = cell.borrow();
    (ui.list, ui.generation)
}

fn defer_focus_loss(parent: HWND, next: HWND) {
    let Some(cell) = (unsafe { win::state_cell::<PickerUi>(parent) }) else {
        return;
    };
    let (list, generation) = picker_focus_snapshot(cell);
    let inside = next == parent || next == list || unsafe { IsChild(parent, next).as_bool() };
    if inside {
        return;
    }
    unsafe {
        let _ = PostMessageW(
            Some(parent),
            WM_APP_PICKER_FOCUS_LOST,
            WPARAM(next.0 as usize),
            LPARAM(generation as isize),
        );
    }
}

fn should_close_after_focus_loss(
    current_generation: u64,
    message_generation: u64,
    focus_is_internal: bool,
) -> bool {
    current_generation == message_generation && !focus_is_internal
}
fn picker_item_from_point(hwnd: HWND, lparam: LPARAM) -> Option<usize> {
    let result = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            hwnd,
            LB_ITEMFROMPOINT,
            Some(WPARAM(0)),
            Some(lparam),
        )
    };

    let packed = result.0 as usize;
    if ((packed >> 16) & 0xFFFF) != 0 {
        None
    } else {
        Some(packed & 0xFFFF)
    }
}

fn set_picker_hover(parent: HWND, list: HWND, hovered: Option<usize>) {
    let Some(cell) = (unsafe { win::state_cell::<PickerUi>(parent) }) else {
        return;
    };
    let changed = {
        let mut ui = cell.borrow_mut();
        let next = hovered.filter(|index| *index < ui.choices.len());
        if ui.hovered_index == next {
            false
        } else {
            ui.hovered_index = next;
            true
        }
    };
    if changed {
        unsafe {
            let _ = InvalidateRect(Some(list), None, false);
        }
    }
}
fn normalize_allowlist_selection(list: HWND) {
    let focused = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            list,
            LB_GETCURSEL,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
    }
    .0;
    if focused < 0 {
        return;
    }
    let focused = focused as usize;
    let is_selected = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            list,
            LB_GETSEL,
            Some(WPARAM(focused)),
            Some(LPARAM(0)),
        )
    }
    .0 != 0;
    if !is_selected {
        return;
    }

    let count = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            list,
            LB_GETCOUNT,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
    }
    .0
    .max(0) as usize;
    if focused < 2 {
        for index in 0..count {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_SETSEL,
                    Some(WPARAM(0)),
                    Some(LPARAM(index as isize)),
                );
            }
        }
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                list,
                LB_SETSEL,
                Some(WPARAM(1)),
                Some(LPARAM(focused as isize)),
            );
        }
    } else {
        for index in 0..2.min(count) {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_SETSEL,
                    Some(WPARAM(0)),
                    Some(LPARAM(index as isize)),
                );
            }
        }
    }
}
unsafe extern "system" fn picker_list_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    ref_data: usize,
) -> LRESULT {
    let parent = HWND(ref_data as *mut _);
    match msg {
        WM_ERASEBKGND => {
            let hdc = HDC(wparam.0 as *mut _);
            let mut rect = RECT::default();
            let _ =
                unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect) };
            let colors = picker_colors(false);
            let brush = unsafe { CreateSolidBrush(to_colorref(colors.background)) };
            let _ = unsafe { FillRect(hdc, &rect, brush) };
            let _ = unsafe { DeleteObject(HGDIOBJ(brush.0)) };
            LRESULT(1)
        }
        WM_MOUSEMOVE => {
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut track);
            }
            set_picker_hover(parent, hwnd, picker_item_from_point(hwnd, lparam));
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSELEAVE => {
            set_picker_hover(parent, hwnd, None);
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_KEYDOWN if wparam.0 as u16 == 0x1B => {
            cancel_picker(parent, true);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 as u16 == 0x09 => {
            let reverse = unsafe { (GetKeyState(0x10) as u16 & 0x8000) != 0 };
            cancel_picker(parent, true);
            crate::event::post_main(crate::event::AppEvent::FocusSettingsFromPicker { reverse });
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 as u16 == 0x0D => {
            commit_selected(parent, hwnd);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            let multi_select = unsafe { win::state_cell::<PickerUi>(parent) }
                .is_some_and(|cell| cell.borrow().kind.is_multi_select());
            if multi_select {
                normalize_allowlist_selection(hwnd);
            } else {
                commit_selected(parent, hwnd);
            }
            result
        }
        WM_KILLFOCUS => {
            let next = HWND(wparam.0 as *mut _);
            defer_focus_loss(parent, next);
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(picker_list_subclass), SUBCLASS_ID);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        },
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

unsafe extern "system" fn picker_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let state = Box::from_raw(create.lpCreateParams as *mut PickerUi);
            win::store_state_ptr(hwnd, win::WindowState::new(*state));
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        let cell = match win::state_cell::<PickerUi>(hwnd) {
            Some(cell) => cell,
            None => return win::def_proc(hwnd, msg, wparam, lparam),
        };
        if msg == WM_NCDESTROY {
            drop(win::take_state::<PickerUi>(hwnd));
            return win::def_proc(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_DPICHANGED => {
                let font =
                    create_picker_font(windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96));
                if font.is_invalid() {
                    return win::def_proc(hwnd, msg, wparam, lparam);
                }
                let (old, list) = {
                    let mut ui = cell.borrow_mut();
                    let old = ui.font;
                    ui.font = font;
                    (old, ui.list)
                };
                if !old.is_invalid() {
                    let _ = DeleteObject(old.into());
                }
                let _ = InvalidateRect(Some(list), None, false);
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_ACTIVATE as isize),
            WM_ERASEBKGND => {
                let hdc = HDC(wparam.0 as *mut _);
                draw_picker_surface(hwnd, hdc);
                LRESULT(1)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut paint);
                draw_picker_surface(hwnd, paint.hdc);
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_MEASUREITEM => {
                let measure = &mut *(lparam.0 as *mut MEASUREITEMSTRUCT);
                if measure.CtlType == ODT_LISTBOX {
                    let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96);
                    measure.itemHeight = (30 * dpi).div_ceil(96).max(1);
                    LRESULT(1)
                } else {
                    win::def_proc(hwnd, msg, wparam, lparam)
                }
            }
            WM_DRAWITEM => {
                let item = &*(lparam.0 as *const DRAWITEMSTRUCT);
                if item.CtlType != ODT_LISTBOX {
                    return win::def_proc(hwnd, msg, wparam, lparam);
                }
                let (list, hovered, label, font) = {
                    let ui = cell.borrow();
                    (
                        ui.list,
                        ui.hovered_index == Some(item.itemID as usize),
                        ui.choices
                            .get(item.itemID as usize)
                            .map(|choice| choice.label.clone()),
                        ui.font,
                    )
                };
                if item.hwndItem != list {
                    return win::def_proc(hwnd, msg, wparam, lparam);
                }
                label.map_or(LRESULT(1), |value| {
                    draw_picker_item(item, &value, hovered, font)
                })
            }
            WM_COMMAND => {
                let _control_id = loword(wparam.0);
                let notification = hiword(wparam.0);
                let source = HWND(lparam.0 as *mut _);
                let (list, multi_select) = {
                    let ui = cell.borrow();
                    (ui.list, ui.kind.is_multi_select())
                };
                if source == list && notification == LBN_DBLCLK && !multi_select {
                    commit_selected(hwnd, source);
                    LRESULT(0)
                } else if source == list && notification == LBN_SELCHANGE {
                    if multi_select {
                        normalize_allowlist_selection(source);
                    }
                    LRESULT(0)
                } else {
                    win::def_proc(hwnd, msg, wparam, lparam)
                }
            }
            WM_APP_PICKER_FOCUS_LOST => {
                let generation = lparam.0 as u64;
                let next = HWND(wparam.0 as *mut _);
                let (list, current_generation) = picker_focus_snapshot(cell);
                let focus_is_internal =
                    next == hwnd || next == list || IsChild(hwnd, next).as_bool();
                if should_close_after_focus_loss(current_generation, generation, focus_is_internal)
                {
                    cancel_picker(hwnd, false);
                }
                LRESULT(0)
            }
            WM_KILLFOCUS => {
                let next = HWND(wparam.0 as *mut _);
                defer_focus_loss(hwnd, next);
                LRESULT(0)
            }
            WM_CLOSE => {
                cancel_picker(hwnd, true);
                LRESULT(0)
            }
            _ => win::def_proc(hwnd, msg, wparam, lparam),
        }
    }
}
fn selected_value(parent: HWND, list: HWND) -> Option<(PickerKind, PickerValue)> {
    let cell = unsafe { win::state_cell::<PickerUi>(parent) }?;
    let kind = cell.borrow().kind;
    if kind.is_multi_select() {
        let count = unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                list,
                LB_GETSELCOUNT,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
        }
        .0;
        if count < 0 {
            return None;
        }
        let mut indices = vec![0i32; count as usize];
        if count > 0 {
            let copied = unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_GETSELITEMS,
                    Some(WPARAM(count as usize)),
                    Some(LPARAM(indices.as_mut_ptr() as isize)),
                )
            }
            .0;
            if copied < 0 {
                return None;
            }
            indices.truncate(copied as usize);
        }
        let ui = cell.borrow();
        let mut use_all = false;
        let mut endpoints = Vec::new();
        for index in indices {
            let Some(choice) = ui.choices.get(index as usize) else {
                continue;
            };
            match &choice.value {
                PickerValue::Allowlist(None) => use_all = true,
                PickerValue::Allowlist(Some(values)) => endpoints.extend(values.iter().cloned()),
                _ => {}
            }
        }
        let allowlist = if !endpoints.is_empty() {
            Some(endpoints)
        } else if use_all {
            None
        } else {
            Some(Vec::new())
        };
        return Some((kind, PickerValue::Allowlist(allowlist)));
    }

    let index = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            list,
            LB_GETCURSEL,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
    }
    .0;
    if index < 0 {
        return None;
    }
    let ui = cell.borrow();
    ui.choices
        .get(index as usize)
        .map(|choice| (ui.kind, choice.value.clone()))
}
fn claim_close(cell: &std::cell::RefCell<PickerUi>, action: PickerCloseAction) -> bool {
    let mut ui = cell.borrow_mut();
    if ui.close_action.is_some() {
        return false;
    }
    ui.close_action = Some(action);
    true
}

fn commit_selected(parent: HWND, list: HWND) {
    let Some((kind, value)) = selected_value(parent, list) else {
        return;
    };
    let Some(cell) = (unsafe { win::state_cell::<PickerUi>(parent) }) else {
        return;
    };
    if !claim_close(cell, PickerCloseAction::Commit) {
        return;
    }
    crate::event::post_main(crate::event::AppEvent::CommitSettingsPicker { kind, value });
}

fn cancel_picker(hwnd: HWND, restore_focus: bool) {
    let Some(cell) = (unsafe { win::state_cell::<PickerUi>(hwnd) }) else {
        return;
    };
    if !claim_close(cell, PickerCloseAction::Cancel) {
        return;
    }
    crate::event::post_main(crate::event::AppEvent::CancelSettingsPicker {
        popup_hwnd: hwnd.0 as isize,
        restore_focus,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::visual::VisualRgb;

    #[test]
    fn popup_prefers_below_then_flips_above() {
        let work = PopupRect::new(0, 0, 1000, 800);
        let below = place_popup(PopupRect::new(100, 100, 300, 140), work, 240, 200);
        assert_eq!(below.top, 140);
        let above = place_popup(PopupRect::new(100, 700, 300, 740), work, 240, 200);
        assert_eq!(above.bottom, 700);
    }

    #[test]
    fn popup_clamps_negative_and_right_edges() {
        let work = PopupRect::new(-500, -200, 500, 600);
        let rect = place_popup(PopupRect::new(450, 100, 480, 140), work, 300, 200);
        assert_eq!(rect.right, 480);
        assert!(rect.left >= work.left);
        assert!(rect.top >= work.top);
    }

    #[test]
    fn wider_popup_preserves_control_attachment_at_right_edge() {
        let anchor = PopupRect::new(700, 100, 800, 140);
        let popup = place_popup(anchor, PopupRect::new(0, 0, 900, 800), 300, 180);
        assert_eq!(popup.right, anchor.right);
        assert_eq!(popup.top, anchor.bottom);
    }
    #[test]
    fn picker_palette_uses_shared_dark_light_theme_colors() {
        let visual = SystemVisualPreferences::default();
        for theme in [Theme::dark(), Theme::light()] {
            let normal = picker_colors_for(false, visual, theme);
            let selected = picker_colors_for(true, visual, theme);
            assert_eq!(normal.background, theme.card);
            assert_eq!(normal.foreground, theme.text);
            assert_eq!(selected.background, theme.accent);
            assert_eq!(selected.foreground, theme.accent_text);
        }
    }

    #[test]
    fn picker_palette_pairs_high_contrast_colors() {
        let visual = SystemVisualPreferences {
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
        let normal = picker_colors_for(false, visual, Theme::dark());
        let selected = picker_colors_for(true, visual, Theme::dark());
        assert_eq!(normal.background, Color::rgb(10, 20, 30));
        assert_eq!(normal.foreground, Color::rgb(240, 200, 160));
        assert_eq!(selected.background, Color::rgb(50, 100, 150));
        assert_eq!(selected.foreground, Color::rgb(1, 2, 3));
    }
    #[test]
    fn picker_item_state_prioritizes_disabled_then_selected_then_hover() {
        assert_eq!(
            picker_item_state(false, false, false),
            PickerItemState::Idle
        );
        assert_eq!(
            picker_item_state(false, true, false),
            PickerItemState::Hovered
        );
        assert_eq!(
            picker_item_state(true, true, false),
            PickerItemState::Selected
        );
        assert_eq!(
            picker_item_state(true, true, true),
            PickerItemState::Disabled
        );
    }

    #[test]
    fn picker_hover_uses_shared_light_and_dark_hover_surfaces() {
        let visual = SystemVisualPreferences::default();
        for theme in [Theme::dark(), Theme::light()] {
            let colors = picker_colors_for_state(PickerItemState::Hovered, visual, theme);
            assert_eq!(colors.background, theme.picker_hover);
            assert_eq!(colors.foreground, theme.text);
        }
    }

    #[test]
    fn selected_picker_item_stays_selected_when_pointer_leaves() {
        let visual = SystemVisualPreferences::default();
        let colors = picker_colors_for_state(
            picker_item_state(true, false, false),
            visual,
            Theme::light(),
        );
        assert_eq!(colors.background, Theme::light().accent);
        assert_eq!(colors.foreground, Theme::light().accent_text);
    }

    #[test]
    fn high_contrast_hover_uses_system_pair_and_outline() {
        let visual = SystemVisualPreferences {
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
            ..SystemVisualPreferences::default()
        };
        let colors = picker_colors_for_state(PickerItemState::Hovered, visual, Theme::dark());
        assert_eq!(colors.background, Color::rgb(10, 20, 30));
        assert_eq!(colors.foreground, Color::rgb(240, 200, 160));
        assert_eq!(colors.border, colors.foreground);
    }
    #[test]
    fn picker_font_policy_is_dpi_scaled_and_explicit() {
        assert_eq!(picker_font_height(96), -14);
        assert_eq!(picker_font_height(144), -21);
        assert_eq!(PICKER_FONT_FAMILY, "Segoe UI Variable Text");
        assert_eq!(PICKER_FONT_FALLBACK, "Segoe UI");
    }
}

#[cfg(test)]
mod wm_command_tests {
    use super::{hiword, loword};

    #[test]
    fn wm_command_words_are_decoded_by_contract() {
        let packed = (0x1234usize << 16) | 0x0056;
        assert_eq!(loword(packed), 0x0056);
        assert_eq!(hiword(packed), 0x1234);
    }
}

#[cfg(test)]
mod focus_loss_tests {
    use super::{
        claim_close, picker_focus_snapshot, should_close_after_focus_loss, PickerCloseAction,
        PickerKind, PickerUi,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::HFONT;

    #[test]
    fn internal_focus_does_not_request_close() {
        assert!(!should_close_after_focus_loss(4, 4, true));
    }

    #[test]
    fn external_focus_closes_only_current_generation() {
        assert!(should_close_after_focus_loss(4, 4, false));
        assert!(!should_close_after_focus_loss(5, 4, false));
    }

    #[test]
    fn focus_snapshot_releases_refcell_borrow_before_win32_work() {
        let cell = std::cell::RefCell::new(PickerUi {
            generation: 9,
            kind: PickerKind::OverlayPosition,
            choices: Vec::new(),
            list: HWND(std::ptr::null_mut()),
            close_action: None,
            hovered_index: None,
            font: HFONT::default(),
        });
        let (list, generation) = picker_focus_snapshot(&cell);
        assert!(list.0.is_null());
        assert_eq!(generation, 9);
        assert!(cell.try_borrow_mut().is_ok());
    }

    #[test]
    fn picker_close_action_is_claimed_once() {
        let cell = std::cell::RefCell::new(PickerUi {
            generation: 9,
            kind: PickerKind::OverlayPosition,
            choices: Vec::new(),
            list: HWND(std::ptr::null_mut()),
            close_action: None,
            hovered_index: None,
            font: HFONT::default(),
        });
        assert!(claim_close(&cell, PickerCloseAction::Commit));
        assert!(!claim_close(&cell, PickerCloseAction::Cancel));
        assert_eq!(cell.borrow().close_action, Some(PickerCloseAction::Commit));
    }
}
