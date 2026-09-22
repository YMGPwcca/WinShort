//! Small native shell helpers shared by user-facing windows.

use crate::error::{Error, Result};
use std::path::Path;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub(crate) fn open_folder(folder: &Path) -> Result<()> {
    std::fs::create_dir_all(folder)
        .map_err(|error| Error::config(format!("create folder: {error}")))?;
    let operation = HSTRING::from("open");
    let target = HSTRING::from(folder.to_string_lossy().as_ref());
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as usize;
    if !shell_execute_succeeded(code) {
        return Err(Error::config(format!("ShellExecuteW failed ({code})")));
    }
    Ok(())
}

fn shell_execute_succeeded(code: usize) -> bool {
    code > 32
}

#[cfg(test)]
mod tests {
    use super::shell_execute_succeeded;

    #[test]
    fn shell_execute_uses_documented_success_boundary() {
        assert!(!shell_execute_succeeded(0));
        assert!(!shell_execute_succeeded(32));
        assert!(shell_execute_succeeded(33));
    }
}
