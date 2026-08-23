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

/// Normalize BOOL-returning Shell APIs into our Result.
fn bool_ok(ok: windows::core::BOOL, api: &'static str) -> Result<()> {
    if ok.as_bool() {
        Ok(())
    } else {
        Err(Error::os(api, 0))
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
    icon_normal: windows::Win32::UI::WindowsAndMessaging::HICON,
    icon_suspended: windows::Win32::UI::WindowsAndMessaging::HICON,
    state: TrayState,
}

impl Tray {
    /// Add the notification area icon (spec §6).
    pub fn install(hwnd: HWND, hicons: (windows::Win32::UI::WindowsAndMessaging::HICON, windows::Win32::UI::WindowsAndMessaging::HICON)) -> Result<Tray> {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_UID;
        nid.uFlags = NOTIFY_ICON_DATA_FLAGS(NIF_MESSAGE | NIF_ICON | NIF_TIP);
        nid.uCallbackMessage = WM_APP_TRAY;
        nid.hIcon = hicons.0;
        set_tip(&mut nid, "WinShort");
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;

        unsafe {
            bool_ok(Shell_NotifyIconW(NIM_ADD, &nid), "Shell_NotifyIconW(NIM_ADD)")?;
            // Ask for version-4 semantics AFTER adding (documented order).
            let mut v = nid;
            v.uFlags = NOTIFY_ICON_DATA_FLAGS::default();
            bool_ok(Shell_NotifyIconW(NIM_SETVERSION, &v), "Shell_NotifyIconW(NIM_SETVERSION)")?;
        }

        Ok(Tray {
            hwnd,
            nid_base: nid,
            icon_normal: hicons.0,
            icon_suspended: hicons.1,
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
            TrayState::Normal => self.icon_normal,
            TrayState::HotkeysSuspended => self.icon_suspended,
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
            TrayState::Normal => self.icon_normal,
            TrayState::HotkeysSuspended => self.icon_suspended,
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
    nid.szTip[..wide.len()].copy_from_slice(&wide);
    nid.szTip[wide.len()] = 0;
}

/// Decode a version-4 tray callback. Returns what happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    DoubleClick { x: i32, y: i32 },
    ContextMenu { x: i32, y: i32 },
    Select { x: i32, y: i32 },
    Other,
}

pub fn decode_callback(wparam: WPARAM, lparam: LPARAM) -> TrayEvent {
    let event = (wparam.0 & 0xFFFF) as u32;
    let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    const WM_CONTEXTMENU: u32 = 0x007B;
    const WM_LBUTTONDBLCLK: u32 = 0x0203;
    const NIN_SELECT: u32 = 0x0400;
    match event {
        WM_LBUTTONDBLCLK => TrayEvent::DoubleClick { x, y },
        WM_CONTEXTMENU => TrayEvent::ContextMenu { x, y },
        NIN_SELECT => TrayEvent::Select { x, y },
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
