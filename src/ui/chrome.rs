//! Shared WinShort-owned titlebar behavior.

use crate::ui::layout::TitlebarGeometry;
use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, HTCLIENT};

pub(crate) fn titlebar_hit_test_dip(chrome: TitlebarGeometry, x: f32, y: f32) -> u32 {
    if chrome.close.contains(x, y) {
        HTCLIENT
    } else if chrome.caption.contains(x, y) {
        HTCAPTION
    } else {
        HTCLIENT
    }
}
