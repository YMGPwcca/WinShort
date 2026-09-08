//! Tray context menu (spec §6 layout).

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, PostMessageW, SetForegroundWindow, TrackPopupMenu, HMENU,
    MF_CHECKED, MF_ENABLED, MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_LEFTBUTTON, TPM_RETURNCMD, TPM_RIGHTBUTTON,
};

use crate::error::{Error, Result};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum Command {
    OpenSettings = 100,
    ShowStatus = 101,
    PauseShortcuts = 102,
    Diagnostics = 103,
    Exit = 104,
}

impl Command {
    fn from_native(value: u32) -> Option<Self> {
        match value {
            100 => Some(Self::OpenSettings),
            101 => Some(Self::ShowStatus),
            102 => Some(Self::PauseShortcuts),
            103 => Some(Self::Diagnostics),
            104 => Some(Self::Exit),
            _ => None,
        }
    }
}

/// Menu state reflecting the one frequently changed tray action.
pub struct MenuState {
    pub suspended: bool,
}

fn build(state: &MenuState) -> Result<OwnedMenu> {
    unsafe {
        let menu = OwnedMenu(CreatePopupMenu().map_err(|e| Error::win("CreatePopupMenu", &e))?);
        append(menu.0, Command::OpenSettings, "&Open WinShort", false)?;
        append(menu.0, Command::ShowStatus, "Show &status", false)?;
        separator(menu.0)?;
        append(
            menu.0,
            Command::PauseShortcuts,
            "&Pause shortcuts",
            state.suspended,
        )?;
        separator(menu.0)?;
        append(menu.0, Command::Diagnostics, "&Diagnostics", false)?;
        append(menu.0, Command::Exit, "E&xit", false)?;
        Ok(menu)
    }
}

fn append(menu: HMENU, id: Command, text: &str, checked: bool) -> Result<()> {
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

pub fn track_tray_menu(hwnd: HWND, pt: POINT, state: &MenuState) -> Option<Command> {
    // SAFETY: window owned by this thread; menu destroyed via OwnedMenu.
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let menu = match build(state) {
            Ok(m) => m,
            Err(error) => {
                crate::warn_!("tray menu creation failed: {error}");
                return None;
            }
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

        Command::from_native(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::Command;
    #[test]
    fn native_selection_rejects_cancellation_and_unknown_commands() {
        assert_eq!(Command::from_native(0), None);
        assert_eq!(Command::from_native(999), None);
        for command in [
            Command::OpenSettings,
            Command::ShowStatus,
            Command::PauseShortcuts,
            Command::Diagnostics,
            Command::Exit,
        ] {
            assert_eq!(Command::from_native(command as u32), Some(command));
        }
    }
}
