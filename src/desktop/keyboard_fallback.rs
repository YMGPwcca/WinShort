//! Graceful fallback using native SendInput Ctrl+Win+Arrow chords.
//!
//! The documented Windows API cannot enumerate/switch absolute desktops. To
//! reach N best-effort, walk left past any realistic desktop count, then right
//! N times. Diagnostics explicitly report that count/current are unavailable.

use crate::desktop::backend::{BackendAvailability, BackendKind, VirtualDesktopBackend};
use crate::error::{Error, Result};

pub struct KeyboardFallback;

impl KeyboardFallback {
    pub const fn new() -> Self {
        Self
    }
}

impl VirtualDesktopBackend for KeyboardFallback {
    fn availability(&self) -> BackendAvailability {
        BackendAvailability::Available
    }

    fn desktop_count(&self) -> Result<usize> {
        Err(Error::desktop(
            "keyboard fallback cannot enumerate virtual desktops",
        ))
    }

    fn current_desktop(&self) -> Result<usize> {
        Err(Error::desktop(
            "keyboard fallback cannot identify the current desktop",
        ))
    }

    fn switch_to(&self, index: usize) -> Result<()> {
        if index > 31 {
            return Err(Error::desktop(format!(
                "fallback target {} exceeds safety limit",
                index + 1
            )));
        }
        // 32 left chords saturate at Desktop 1 for the supported UX (1–9).
        for _ in 0..32 {
            send_chord(Arrow::Left)?;
            std::thread::sleep(std::time::Duration::from_millis(18));
        }
        for _ in 0..index {
            send_chord(Arrow::Right)?;
            std::thread::sleep(std::time::Duration::from_millis(28));
        }
        Ok(())
    }

    fn kind(&self) -> BackendKind {
        BackendKind::KeyboardFallback
    }
}

#[derive(Clone, Copy)]
enum Arrow {
    Left,
    Right,
}

fn send_chord(arrow: Arrow) -> Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    const VK_CONTROL: u16 = 0x11;
    const VK_LWIN: u16 = 0x5B;
    const VK_RWIN: u16 = 0x5C;
    const VK_LEFT: u16 = 0x25;
    const VK_RIGHT: u16 = 0x27;
    const TAG: usize = 0x5753_484F_5254; // "WSHORT"

    let win_held = unsafe {
        (GetAsyncKeyState(VK_LWIN as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RWIN as i32) as u16 & 0x8000) != 0
    };
    let arrow = match arrow {
        Arrow::Left => VK_LEFT,
        Arrow::Right => VK_RIGHT,
    };
    let key = |vk: u16, flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                dwFlags: flags,
                dwExtraInfo: TAG,
                ..Default::default()
            },
        },
    };

    let mut inputs = Vec::with_capacity(6);
    inputs.push(key(VK_CONTROL, Default::default()));
    if !win_held {
        inputs.push(key(VK_LWIN, Default::default()));
    }
    inputs.push(key(arrow, KEYEVENTF_EXTENDEDKEY));
    inputs.push(key(arrow, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP));
    if !win_held {
        inputs.push(key(VK_LWIN, KEYEVENTF_KEYUP));
    }
    inputs.push(key(VK_CONTROL, KEYEVENTF_KEYUP));

    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(Error::os_ctx(
            "SendInput(Ctrl+Win+Arrow)",
            unsafe { windows::Win32::Foundation::GetLastError().0 },
            format!("sent {sent}/{} events", inputs.len()),
        ))
    }
}
