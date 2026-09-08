//! Lightweight keyboard dispatch helpers. The hook posts typed action tags;
//! any synthetic key work happens later on the main thread, never in the hook.

use crate::error::{Error, Result};

const DUMMY_CHORD_VK: u16 = 0xFF;

fn dummy_chord_input() -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(DUMMY_CHORD_VK),
                dwFlags: KEYEVENTF_KEYUP,
                ..Default::default()
            },
        },
    }
}

/// Consume the physical Win chord before its release can open Start. The
/// non-semantic dummy key-up follows the Shell countermeasure pattern used by
/// Microsoft's PowerToys centralized keyboard hook and is ignored by our hook
/// via LLKHF_INJECTED.
pub fn dirty_win_chord() -> Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT};

    let inputs = [dummy_chord_input()];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        return Ok(());
    }
    Err(Error::os_ctx(
        "SendInput(chord dirtier)",
        unsafe { windows::Win32::Foundation::GetLastError().0 },
        format!("sent {sent}/{} events", inputs.len()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT_KEYBOARD, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    #[test]
    fn chord_dirtier_is_one_dummy_keyup_not_a_modifier() {
        let inputs = [dummy_chord_input()];
        assert_eq!(inputs.len(), 1);
        assert_eq!(DUMMY_CHORD_VK, 0xFF);

        let input = inputs[0];
        assert_eq!(input.r#type, INPUT_KEYBOARD);
        // SAFETY: `dummy_chord_input` constructs an INPUT_KEYBOARD value.
        let key = unsafe { input.Anonymous.ki };
        assert_eq!(key.wVk, VIRTUAL_KEY(DUMMY_CHORD_VK));
        assert_eq!(key.dwFlags, KEYEVENTF_KEYUP);
        assert!([0x10, 0x11, 0x12, 0x5B, 0x5C]
            .iter()
            .all(|modifier| DUMMY_CHORD_VK != *modifier));
    }
}
