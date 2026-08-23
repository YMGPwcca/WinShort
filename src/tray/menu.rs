//! Tray context menu (spec §6 layout).

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, PostMessageW, SetForegroundWindow, TrackPopupMenu,
    HMENU, MF_CHECKED, MF_ENABLED, MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_LEFTBUTTON, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPMPARAMS,
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
        append(menu, cmd::OPEN_SETTINGS, "&Open Settings", false, false)?;
        append(menu, cmd::SHOW_STATUS, "&Show Status", false, false)?;
        separator(menu)?;
        append(
            menu,
            cmd::SUSPEND_HOTKEYS,
            "&Suspend Hotkeys",
            state.suspended,
            true,
        )?;
        separator(menu)?;
        append(
            menu,
            cmd::START_WITH_WINDOWS,
            "&Start with Windows",
            state.start_with_windows,
            true,
        )?;
        separator(menu)?;
        append(menu, cmd::EXIT, "E&xit", false, false)?;
        Ok(menu)
    }
}

fn append(
    menu: HMENU,
    id: u32,
    text: &str,
    checked: bool,
    toggleable: bool,
) -> Result<()> {
    let flags =
        MF_STRING | if checked { MF_CHECKED } else { MF_ENABLED } | if toggleable { MF_ENABLED } else { MF_ENABLED };
    // SAFETY: valid HMENU from CreatePopupMenu.
    let ok = unsafe {
        windows::Win32::UI::WindowsAndMessaging::AppendMenuW(
            menu,
            flags,
            id as usize,
            windows::core::PCWSTR(
                windows::core::HSTRING::from(text).as_ptr(),
            ),
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
pub fn track_tray_menu(hwnd: HWND, pt: POINT, state: &MenuState) -> Option<u32> {
    // SAFETY: window owned by this thread; menu destroyed before return.
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let menu = match build(state) {
            Ok(m) => m,
            Err(_) => return None,
        };
        // With TPM_RETURNCMD the BOOL payload IS the chosen command id.
        let res = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_LEFTBUTTON,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let cmd = res.0 as u32;
        if cmd == 0 {
            None
        } else {
            Some(cmd)
        }
    }
}
