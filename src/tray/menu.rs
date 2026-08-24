//! Tray context menu (spec §6 layout).

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, PostMessageW, SetForegroundWindow, TrackPopupMenu, HMENU,
    MF_CHECKED, MF_ENABLED, MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_LEFTBUTTON, TPM_RETURNCMD, TPM_RIGHTBUTTON,
};

use crate::error::{Error, Result};
pub mod cmd {
    pub const OPEN_SETTINGS: u32 = 100;
    pub const SHOW_STATUS: u32 = 101;
    pub const SUSPEND_HOTKEYS: u32 = 102;
    pub const START_WITH_WINDOWS: u32 = 103;
    pub const EXIT: u32 = 104;
}

/// Menu model reflecting live app state.
pub struct MenuState {
    pub suspended: bool,
    pub start_with_windows: bool,
}

pub fn build(state: &MenuState) -> Result<HMENU> {
    unsafe {
        let menu = CreatePopupMenu().map_err(|e| Error::win("CreatePopupMenu", &e))?;
        append(menu, cmd::OPEN_SETTINGS, "&Open Settings", false)?;
        append(menu, cmd::SHOW_STATUS, "&Show Status", false)?;
        separator(menu)?;
        append(
            menu,
            cmd::SUSPEND_HOTKEYS,
            "&Suspend Hotkeys",
            state.suspended,
        )?;
        separator(menu)?;
        append(
            menu,
            cmd::START_WITH_WINDOWS,
            "&Start with Windows",
            state.start_with_windows,
        )?;
        separator(menu)?;
        append(menu, cmd::EXIT, "E&xit", false)?;
        Ok(menu)
    }
}

fn append(menu: HMENU, id: u32, text: &str, checked: bool) -> Result<()> {
    let flags = MF_STRING | if checked { MF_CHECKED } else { MF_ENABLED };
    // SAFETY: valid HMENU from CreatePopupMenu.
    let ok = unsafe {
        windows::Win32::UI::WindowsAndMessaging::AppendMenuW(
            menu,
            flags,
            id as usize,
            windows::core::PCWSTR(windows::core::HSTRING::from(text).as_ptr()),
        )
    };
    if let Err(e) = ok {
        return Err(Error::win("AppendMenuW", &e));
    }
    Ok(())
}

fn separator(menu: HMENU) -> Result<()> {
    // SAFETY: valid HMENU.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::AppendMenuW(menu, MF_SEPARATOR, 0, None)
            .map_err(|e| Error::win("AppendMenuW(sep)", &e))?;
    }
    Ok(())
}
/// Show the menu modally at a screen point; returns selected command or None.
///
/// Implements the documented SetForegroundWindow dance so the menu dismisses
/// on outside click.
/// Owned popup menu: `DestroyMenu` runs on Drop (#23).
struct OwnedMenu(HMENU);

impl Drop for OwnedMenu {
    fn drop(&mut self) {
        // SAFETY: created by CreatePopupMenu in this module and not
        // transferred anywhere else.
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

pub fn track_tray_menu(hwnd: HWND, pt: POINT, state: &MenuState) -> Option<u32> {
    // SAFETY: window owned by this thread; menu destroyed via OwnedMenu.
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let menu = match build(state) {
            Ok(m) => OwnedMenu(m),
            Err(_) => return None,
        };
        // With TPM_RETURNCMD the BOOL payload IS the chosen command id.
        let res = TrackPopupMenu(
            menu.0,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_LEFTBUTTON,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let cmd = res.0 as u32;

        // Documented requirement after TrackPopupMenu on a notification-icon
        // context menu (#23): without it the next click can be swallowed and
        // the menu may re-open.
        use windows::Win32::UI::WindowsAndMessaging::WM_NULL;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));

        if cmd == 0 {
            None
        } else {
            Some(cmd)
        }
    }
}
