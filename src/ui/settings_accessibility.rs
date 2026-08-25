//! Native child-control semantics layered onto the owner-drawn Settings surface.
//!
//! Buttons remain owner-drawn by the parent surface, but are real child HWNDs
//! with names, focus, enabled state, and UI Automation control types. Sliders
//! use native trackbars so RangeValue semantics are available to Narrator/UIA.

use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::Controls::{
    TOOLTIPS_CLASSW, TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTTOOLINFOW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, GetKeyState};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SetWindowPos, SetWindowTextW, ShowWindow, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_KEYDOWN, WM_KILLFOCUS,
    WM_NCDESTROY, WM_SETFOCUS, WS_CHILD, WS_EX_TRANSPARENT, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

use crate::ui::layout::{ElementId, Rect as UiRect, SettingsLayout};

const BUTTON_CLASS: &str = "BUTTON";
const TRACKBAR_CLASS: &str = "msctls_trackbar32";
const BS_OWNERDRAW: u32 = 0x0000000B;
const BS_AUTOCHECKBOX: u32 = 0x00000003;
const BM_SETCHECK: u32 = 0x00F1;
const BST_CHECKED: usize = 1;
const TBS_NOTICKS: u32 = 0x00000010;
const TBM_SETRANGE: u32 = 0x0400 + 6;
const TBM_SETPOS: u32 = 0x0400 + 5;
const TBM_GETPOS: u32 = 0x0400;

#[derive(Debug, Clone, Copy)]
struct AccessibleControl {
    id: ElementId,
    hwnd: HWND,
    slider: bool,
    toggle: bool,
}

const ACCESSIBILITY_SUBCLASS_ID: usize = 2;

unsafe extern "system" fn accessibility_child_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    ref_data: usize,
) -> windows::Win32::Foundation::LRESULT {
    let parent = HWND(ref_data as *mut _);
    if msg == WM_KEYDOWN && wparam.0 as u16 == 0x09 {
        let reverse = unsafe { (GetKeyState(0x10) as u16 & 0x8000) != 0 };
        crate::app::with_app(|app| app.focus_settings_from_child(reverse));
        return windows::Win32::Foundation::LRESULT(0);
    }
    if matches!(msg, WM_SETFOCUS | WM_KILLFOCUS) {
        unsafe {
            let _ = InvalidateRect(Some(parent), None, false);
        }
    }
    if msg == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(
                hwnd,
                Some(accessibility_child_subclass),
                ACCESSIBILITY_SUBCLASS_ID,
            );
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

pub struct SettingsAccessibility {
    controls: Vec<AccessibleControl>,
    tooltip: Option<TooltipManager>,
}

struct TooltipManager {
    hwnd: HWND,
    #[allow(dead_code)] // keeps TOOLINFO lpszText buffers alive
    texts: Vec<Vec<u16>>,
}

impl TooltipManager {
    fn create(parent: HWND, controls: &[AccessibleControl]) -> Option<Self> {
        let class = TOOLTIPS_CLASSW;
        let tooltip = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                PCWSTR(HSTRING::from("").as_ptr()),
                WINDOW_STYLE(WS_POPUP.0 | 0x01 | 0x02),
                0,
                0,
                0,
                0,
                Some(parent),
                None,
                None,
                None,
            )
        }
        .ok()?;
        let mut texts = Vec::with_capacity(controls.len());
        for control in controls {
            let mut text: Vec<u16> = help_text(control.id).encode_utf16().collect();
            text.push(0);
            let mut info = TTTOOLINFOW {
                cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
                uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                hwnd: parent,
                uId: control.hwnd.0 as usize,
                lpszText: PWSTR(text.as_mut_ptr()),
                ..Default::default()
            };
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    tooltip,
                    TTM_ADDTOOLW,
                    Some(WPARAM(0)),
                    Some(LPARAM((&mut info as *mut TTTOOLINFOW).cast::<()>() as isize)),
                );
            }
            texts.push(text);
        }
        Some(Self {
            hwnd: tooltip,
            texts,
        })
    }
}

impl Drop for TooltipManager {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn help_text(id: ElementId) -> &'static str {
    match id {
        ElementId::StartWithWindows => "Registry-backed startup applies immediately.",
        ElementId::StartHotkeysEnabled => {
            "Suspend hotkeys without changing the configured shortcuts."
        }
        ElementId::MicHotkey => "Record the shortcut used to toggle microphone mute.",
        ElementId::OutputHotkey => "Record the shortcut used to toggle output mute.",
        ElementId::ForegroundHotkey => "Mute sessions owned by the current foreground application.",
        ElementId::InputDevice => "Choose Default or preserve an explicit unavailable endpoint.",
        ElementId::OutputDevice => "Choose Default or preserve an explicit unavailable endpoint.",
        ElementId::InputRole | ElementId::OutputRole => {
            "Only applies when following the Windows default device."
        }
        ElementId::DesktopsEnabled => "Use the native desktop backend when compatible.",
        ElementId::WinNumberEnabled => "Reserve Win+1 through Win+9 for desktop switching.",
        ElementId::OverlayEnabled => "Show status changes without stealing focus.",
        ElementId::OverlayPosition => "Choose one of the seven overlay positions.",
        ElementId::OverlayMonitor => "Choose Foreground, Primary, or a stable monitor device name.",
        ElementId::OverlayDuration => "Adjust from 500 ms to 10 seconds.",
        ElementId::OverlayOpacity => "Adjust from 30 percent to 100 percent.",
        ElementId::OverlayScale => "Adjust from 0.7× to 1.6×.",
        ElementId::OverlayPreview => "Show a representative status overlay.",
        ElementId::DiagnosticsStatus => "Open runtime diagnostics and sanitized support actions.",
        ElementId::OpenConfigFolder => "Open WinShort's data directory.",
        ElementId::ResetSettings => {
            "Two-step reset changes the draft only; Save is still required."
        }
        ElementId::Cancel => "Discard the current draft.",
        ElementId::Save => "Validate and persist the current draft.",
    }
}

impl SettingsAccessibility {
    pub fn create(parent: HWND) -> Self {
        let mut controls = Vec::with_capacity(ElementId::FOCUS_ORDER.len());
        for id in ElementId::FOCUS_ORDER {
            let slider = matches!(
                id,
                ElementId::OverlayDuration | ElementId::OverlayOpacity | ElementId::OverlayScale
            );
            let toggle = matches!(
                id,
                ElementId::StartWithWindows
                    | ElementId::StartHotkeysEnabled
                    | ElementId::DesktopsEnabled
                    | ElementId::WinNumberEnabled
                    | ElementId::OverlayEnabled
            );
            let class = HSTRING::from(if slider { TRACKBAR_CLASS } else { BUTTON_CLASS });
            let style = if slider {
                WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | TBS_NOTICKS
            } else {
                WS_CHILD.0
                    | WS_VISIBLE.0
                    | WS_TABSTOP.0
                    | BS_OWNERDRAW
                    | if toggle { BS_AUTOCHECKBOX } else { 0 }
            };
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(WS_EX_TRANSPARENT.0),
                    PCWSTR(class.as_ptr()),
                    PCWSTR(HSTRING::from("").as_ptr()),
                    WINDOW_STYLE(style),
                    0,
                    0,
                    0,
                    0,
                    Some(parent),
                    None,
                    None,
                    None,
                )
            };
            if let Ok(hwnd) = hwnd {
                unsafe {
                    let _ = SetWindowSubclass(
                        hwnd,
                        Some(accessibility_child_subclass),
                        ACCESSIBILITY_SUBCLASS_ID,
                        parent.0 as usize,
                    );
                }
                controls.push(AccessibleControl {
                    id,
                    hwnd,
                    slider,
                    toggle,
                });
            }
        }
        let tooltip = TooltipManager::create(parent, &controls);
        Self { controls, tooltip }
    }

    pub fn id_for(&self, hwnd: HWND) -> Option<ElementId> {
        self.controls
            .iter()
            .find(|control| control.hwnd == hwnd)
            .map(|control| control.id)
    }

    pub fn sync(
        &mut self,
        layout: &SettingsLayout,
        values: &[(ElementId, String, bool, f32)],
        dpi: u32,
    ) {
        for control in &self.controls {
            let Some(element) = layout.element(control.id) else {
                continue;
            };
            let Some((_, value, enabled, ratio)) =
                values.iter().find(|(id, _, _, _)| *id == control.id)
            else {
                continue;
            };
            let clipped = clip_rect(element.rect, layout.content_clip, element.scrolls);
            let visible = clipped.is_some();
            let rect = physical_rect(clipped.unwrap_or(element.rect), dpi as f32 / 96.0);
            let text = HSTRING::from(format!(
                "{}: {}. {}",
                element.label, value, element.description
            ));
            unsafe {
                let _ = EnableWindow(control.hwnd, *enabled);
                let _ = SetWindowTextW(control.hwnd, PCWSTR(text.as_ptr()));
                let _ = ShowWindow(control.hwnd, if visible { SW_SHOW } else { SW_HIDE });
                let _ = SetWindowPos(
                    control.hwnd,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                if control.toggle {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        control.hwnd,
                        BM_SETCHECK,
                        Some(WPARAM(if value == "On" { BST_CHECKED } else { 0 })),
                        Some(LPARAM(0)),
                    );
                }
                if control.slider {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        control.hwnd,
                        TBM_SETRANGE,
                        Some(WPARAM(1)),
                        Some(LPARAM(((0i32 as u32) << 16 | 1000) as isize)),
                    );
                    let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        control.hwnd,
                        TBM_SETPOS,
                        Some(WPARAM(1)),
                        Some(LPARAM((ratio.clamp(0.0, 1.0) * 1000.0).round() as isize)),
                    );
                }
            }
        }
    }

    pub fn focused_id(&self) -> Option<ElementId> {
        let focus = unsafe { GetFocus() };
        self.id_for(focus)
    }

    pub fn focus_hwnd(&self, id: ElementId) -> Option<HWND> {
        self.controls
            .iter()
            .find(|control| control.id == id)
            .map(|control| control.hwnd)
    }
    pub fn slider_ratio(&self, hwnd: HWND) -> Option<f32> {
        let control = self.controls.iter().find(|control| control.hwnd == hwnd)?;
        if !control.slider {
            return None;
        }
        let position = unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                hwnd,
                TBM_GETPOS,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
        };
        Some((position.0 as f32 / 1000.0).clamp(0.0, 1.0))
    }
}

impl Drop for SettingsAccessibility {
    fn drop(&mut self) {
        let _ = self.tooltip.take();
        for control in self.controls.drain(..) {
            unsafe {
                let _ = DestroyWindow(control.hwnd);
            }
        }
    }
}

pub(crate) fn clip_rect(rect: UiRect, viewport: UiRect, scrolls: bool) -> Option<UiRect> {
    if !scrolls {
        return Some(rect);
    }
    let left = rect.x.max(viewport.x);
    let top = rect.y.max(viewport.y);
    let right = rect.right().min(viewport.right());
    let bottom = rect.bottom().min(viewport.bottom());
    (right > left && bottom > top).then_some(UiRect::new(left, top, right - left, bottom - top))
}

fn physical_rect(rect: UiRect, scale: f32) -> RECT {
    RECT {
        left: (rect.x * scale).round() as i32,
        top: (rect.y * scale).round() as i32,
        right: (rect.right() * scale).round() as i32,
        bottom: (rect.bottom() * scale).round() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_scroll_child_matches_rendered_viewport() {
        let viewport = UiRect::new(0.0, 100.0, 600.0, 300.0);
        let partial = clip_rect(UiRect::new(20.0, 80.0, 200.0, 80.0), viewport, true)
            .expect("partial row should remain visible");
        assert_eq!(partial.y, 100.0);
        assert_eq!(partial.h, 60.0);
        assert!(clip_rect(UiRect::new(20.0, 20.0, 200.0, 40.0), viewport, true).is_none());
        assert_eq!(
            clip_rect(UiRect::new(20.0, 20.0, 200.0, 40.0), viewport, false)
                .expect("fixed footer is not viewport-clipped")
                .y,
            20.0
        );
    }
}
