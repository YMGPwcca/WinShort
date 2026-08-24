//! Tray subsystem: Shell_NotifyIconW lifecycle, runtime-drawn icons, context menu.

pub mod icon;
pub mod menu;

use std::mem::size_of;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFY_ICON_DATA_FLAGS, NOTIFY_ICON_STATE, NOTIFYICONDATAW,
    NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICON_VERSION_4,
};

use crate::error::{Error, Result};
use crate::event::WM_APP_TRAY;

/// Normalize BOOL-returning Shell APIs into our Result, preserving
/// GetLastError so diagnostics are truthful (#23).
fn bool_ok(ok: windows::core::BOOL, api: &'static str) -> Result<()> {
    if ok.as_bool() {
        Ok(())
    } else {
        Err(Error::os(api, unsafe {
            windows::Win32::Foundation::GetLastError().0
        }))
    }
}

/// Owned `HICON`: `DestroyIcon` runs on Drop (#23). The shell copies the
/// handle value; ownership of the GDI object stays with us.
#[derive(Debug)]
pub struct OwnedIcon(windows::Win32::UI::WindowsAndMessaging::HICON);

impl OwnedIcon {
    pub fn handle(&self) -> windows::Win32::UI::WindowsAndMessaging::HICON {
        self.0
    }
}

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;
        // SAFETY: created by CreateIconIndirect and not destroyed elsewhere.
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

/// Tray state reflected in the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Normal,
    HotkeysSuspended,
}

const TRAY_UID: u32 = 1;

pub struct Tray {
    hwnd: HWND,
    nid_base: NOTIFYICONDATAW,
    _icon_normal: OwnedIcon,
    _icon_suspended: OwnedIcon,
    state: TrayState,
}

impl Tray {
    /// Add the notification area icon (spec §6).
    pub fn install(
        hwnd: HWND,
        icons: (OwnedIcon, OwnedIcon),
    ) -> Result<Tray> {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_UID;
        nid.uFlags = NOTIFY_ICON_DATA_FLAGS(NIF_MESSAGE | NIF_ICON | NIF_TIP);
        nid.uCallbackMessage = WM_APP_TRAY;
        nid.hIcon = icons.0.handle();
        set_tip(&mut nid, "WinShort");
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;

        unsafe {
            bool_ok(Shell_NotifyIconW(NIM_ADD, &nid), "Shell_NotifyIconW(NIM_ADD)")?;
            // Ask for version-4 semantics AFTER adding (documented order).
            let mut v = nid;
            v.uFlags = NOTIFY_ICON_DATA_FLAGS::default();
            if let Err(e) =
                bool_ok(Shell_NotifyIconW(NIM_SETVERSION, &v), "Shell_NotifyIconW(NIM_SETVERSION)")
            {
                // Roll back the half-installed icon (#23).
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                return Err(e);
            }
        }

        Ok(Tray {
            hwnd,
            nid_base: nid,
            _icon_normal: icons.0,
            _icon_suspended: icons.1,
            state: TrayState::Normal,
        })
    }

    pub fn state(&self) -> TrayState {
        self.state
    }

    /// Reflect hotkey suspension visually (spec §6: "visibly change").
    pub fn set_state(&mut self, state: TrayState) -> Result<()> {
        if self.state == state {
            return Ok(());
        }
        let mut nid = self.nid_base;
        nid.uFlags = NOTIFY_ICON_DATA_FLAGS(NIF_ICON);
        nid.hIcon = match state {
            TrayState::Normal => self._icon_normal.handle(),
            TrayState::HotkeysSuspended => self._icon_suspended.handle(),
        };
        unsafe {
            bool_ok(Shell_NotifyIconW(NIM_MODIFY, &nid), "Shell_NotifyIconW(NIM_MODIFY)")?;
        }
        self.state = state;
        Ok(())
    }

    /// Recreate the icon after Explorer restart (TaskbarCreated broadcast).
    pub fn recreate(&mut self) -> Result<()> {
        let mut nid = self.nid_base;
        nid.uFlags = NOTIFY_ICON_DATA_FLAGS(NIF_MESSAGE | NIF_ICON | NIF_TIP);
        nid.hIcon = match self.state {
            TrayState::Normal => self._icon_normal.handle(),
            TrayState::HotkeysSuspended => self._icon_suspended.handle(),
        };
        unsafe {
            // Ignore "already exists": Explorer may have resurrected us partially.
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            bool_ok(Shell_NotifyIconW(NIM_ADD, &nid), "Shell_NotifyIconW(re-add)")?;
            let mut v = nid;
            v.uFlags = NOTIFY_ICON_DATA_FLAGS::default();
            v.Anonymous.uVersion = NOTIFYICON_VERSION_4;

            bool_ok(Shell_NotifyIconW(NIM_SETVERSION, &v), "Shell_NotifyIconW(NIM_SETVERSION)")?;
        }
        Ok(())
    }

    pub fn remove(&self) {
        let mut nid = self.nid_base;
        nid.uFlags = NOTIFY_ICON_DATA_FLAGS::default();
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
}

fn set_tip(nid: &mut NOTIFYICONDATAW, tip: &str) {
    let wide: Vec<u16> = tip.encode_utf16().take(127).collect();
    // SAFETY: szTip lives inside a packed struct — write through raw pointers
    // to avoid unaligned references (#28 found this on i686).
    let tip_ptr = std::ptr::addr_of_mut!(nid.szTip) as *mut u16;
    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), tip_ptr, wide.len());
        *tip_ptr.add(wide.len()) = 0;
    }
}

/// Decode a version-4 tray callback message.
///
/// Shell_NotifyIcon `NOTIFYICON_VERSION_4` layout (shellapi.h): lParam
/// LOWORD = notification event, lParam HIWORD = icon id, wParam = packed
/// client POINT (x LOWORD, y HIWORD). A callback from an unexpected icon id
/// decodes to [`TrayEvent::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    DoubleClick { icon_id: u32, x: i32, y: i32 },
    ContextMenu { icon_id: u32, x: i32, y: i32 },
    /// Icon activated by mouse select or keyboard (NIN_SELECT / NIN_KEYSELECT).
    Select { icon_id: u32 },
    Other,
}

const WM_CONTEXTMENU: u32 = 0x007B;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const NIN_SELECT: u32 = 0x0400;
const NIN_KEYSELECT: u32 = 0x0401;

pub fn decode_callback(wparam: WPARAM, lparam: LPARAM) -> TrayEvent {
    let event = (lparam.0 & 0xFFFF) as u16 as u32;
    let icon_id = ((lparam.0 >> 16) & 0xFFFF) as u16 as u32;
    let x = (wparam.0 & 0xFFFF) as u16 as i16 as i32;
    let y = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    if icon_id != TRAY_UID {
        return TrayEvent::Other;
    }
    match event {
        WM_LBUTTONDBLCLK => TrayEvent::DoubleClick { icon_id, x, y },
        WM_CONTEXTMENU => TrayEvent::ContextMenu { icon_id, x, y },
        NIN_SELECT | NIN_KEYSELECT => TrayEvent::Select { icon_id },
        _ => TrayEvent::Other,
    }
}

/// Show the context menu anchored near `pt` (screen coords), returning the
/// chosen command id (see [`menu`]) or None.
pub fn show_menu(hwnd: HWND, pt: POINT) -> Option<u32> {
    menu::track_tray_menu(hwnd, pt, &menu::MenuState { suspended: false, start_with_windows: false })
}

// NIF_* constants re-exported locally (bitflag values).
const NIF_MESSAGE: u32 = 0x01;
const NIF_ICON: u32 = 0x02;
const NIF_TIP: u32 = 0x04;

#[cfg(test)]
mod tests {
    use super::*;

    /// Pack a V4 callback the way the shell does: lParam = event | id<<16,
    /// wParam = x | y<<16 (i16 coordinates).
    fn pack(event: u32, id: u32, x: i32, y: i32) -> (WPARAM, LPARAM) {
        let lp = ((event as usize) & 0xFFFF) | (((id as usize) & 0xFFFF) << 16);
        let wp = ((x as u16) as usize) | (((y as u16) as usize) << 16);
        (WPARAM(wp), LPARAM(lp as isize))
    }

    #[test]
    fn v4_decode_event_and_id_come_from_lparam() {
        let (wp, lp) = pack(WM_CONTEXTMENU, TRAY_UID, 100, 200);
        assert_eq!(
            decode_callback(wp, lp),
            TrayEvent::ContextMenu { icon_id: TRAY_UID, x: 100, y: 200 }
        );
        let (wp, lp) = pack(WM_LBUTTONDBLCLK, TRAY_UID, 0, 0);
        assert!(matches!(
            decode_callback(wp, lp),
            TrayEvent::DoubleClick { icon_id: 1, .. }
        ));
    }

    #[test]
    fn v4_decode_coordinates_come_from_wparam_with_sign_extension() {
        let (wp, lp) = pack(WM_CONTEXTMENU, TRAY_UID, -1920, 500);
        match decode_callback(wp, lp) {
            TrayEvent::ContextMenu { x, y, .. } => {
                assert_eq!((x, y), (-1920, 500), "negative multi-monitor coords must survive");
            }
            other => panic!("wrong event {other:?}"),
        }
    }

    #[test]
    fn v4_decode_keyselect_opens_select_path() {
        let (wp, lp) = pack(NIN_KEYSELECT, TRAY_UID, 5, 6);
        assert!(matches!(decode_callback(wp, lp), TrayEvent::Select { icon_id: 1 }));
        let (wp, lp) = pack(NIN_SELECT, TRAY_UID, 5, 6);
        assert!(matches!(decode_callback(wp, lp), TrayEvent::Select { .. }));
    }

    #[test]
    fn v4_decode_foreign_icon_id_is_other() {
        let (wp, lp) = pack(WM_CONTEXTMENU, 42, 1, 2);
        assert_eq!(decode_callback(wp, lp), TrayEvent::Other);
    }
}
