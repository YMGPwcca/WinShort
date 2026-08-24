//! Graceful fallback using native SendInput Ctrl+Win+Arrow chords.
//!
//! The documented Windows API cannot enumerate/switch absolute desktops. To
//! reach N best-effort, walk left past any realistic desktop count, then right
//! N times. Diagnostics explicitly report that count/current are unavailable.

use crate::desktop::backend::{
    BackendAvailability, BackendKind, DesktopError, VirtualDesktopBackend,
};

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

    fn desktop_count(&self) -> std::result::Result<usize, DesktopError> {
        Err(DesktopError::BackendUnavailable(
            "keyboard fallback cannot enumerate virtual desktops".into(),
        ))
    }

    fn current_desktop(&self) -> std::result::Result<usize, DesktopError> {
        Err(DesktopError::BackendUnavailable(
            "keyboard fallback cannot identify the current desktop".into(),
        ))
    }

    fn switch_to(&self, index: usize) -> std::result::Result<(), DesktopError> {
        if index > 31 {
            return Err(DesktopError::TargetOutOfRange { requested: index, count: 32 });
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

fn send_chord(arrow: Arrow) -> std::result::Result<(), DesktopError> {
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

    let sent = send_inputs(&inputs);
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        // Partial injection (#35/#22): release every distinct key the
        // sequence may have pressed so the user isn't left with stuck
        // Ctrl/Win/arrow. Best effort — errors here are logged upstream.
        let mut pressed: Vec<u16> = Vec::new();
        for input in inputs.iter().take(sent as usize) {
            // SAFETY: inputs were built as INPUT_KEYBOARD above.
            let ki = unsafe { &input.Anonymous.ki };
            let is_up = (ki.dwFlags & KEYEVENTF_KEYUP) == KEYEVENTF_KEYUP;
            if !is_up && !pressed.contains(&ki.wVk.0) {
                pressed.push(ki.wVk.0);
            }
        }
        let mut ups: Vec<INPUT> = Vec::new();
        for vk in pressed {
            ups.push(key(vk, KEYEVENTF_KEYUP));
        }
        if !ups.is_empty() {
            send_inputs(&ups);
        }
        Err(DesktopError::BackendUnavailable(format!(
            "SendInput(Ctrl+Win+Arrow) sent {sent}/{} events; key-up cleanup sent",
            inputs.len()
        )))
    }
}

/// Indirection over SendInput (test seam for batch sizes).
fn send_inputs(inputs: &[INPUT]) -> u32 {
    use windows::Win32::UI::Input::KeyboardAndMouse::SendInput;
    unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) }
}
use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, KEYEVENTF_KEYUP};
