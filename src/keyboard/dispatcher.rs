//! Lightweight keyboard dispatch helpers. The hook posts typed action tags;
//! any synthetic key work happens later on the main thread, never in the hook.

use crate::error::{Error, Result};

/// Consume the physical Win chord before its release can open Start. A Ctrl tap
/// is harmless, visible to the Shell, and ignored by our hook via LLKHF_INJECTED.
pub fn dirty_win_chord() -> Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    let down = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0x11), // VK_CONTROL
                ..Default::default()
            },
        },
    };
    let up = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0x11),
                dwFlags: KEYEVENTF_KEYUP,
                ..Default::default()
            },
        },
    };
    let inputs = [down, up];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        return Ok(());
    }
    // Partial send (#35): if the Ctrl DOWN went out but the UP did not, the
    // user's Ctrl is synthetically stuck — release it best effort.
    if sent >= 1 {
        unsafe {
            let _ = SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
        }
    }
    Err(Error::os_ctx(
        "SendInput(chord dirtier)",
        unsafe { windows::Win32::Foundation::GetLastError().0 },
        format!("sent {sent}/{} events", inputs.len()),
    ))
}
