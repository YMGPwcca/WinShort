use super::{
    claim_close, picker_focus_snapshot, should_close_after_focus_loss, PickerCloseAction,
    PickerKind, PickerUi,
};
use windows::Win32::Foundation::HWND;
#[cfg(not(test))]
use windows::Win32::Graphics::Gdi::HFONT;

#[test]
fn internal_focus_does_not_request_close() {
    assert!(!should_close_after_focus_loss(4, 4, true));
}

#[test]
fn external_focus_closes_only_current_generation() {
    assert!(should_close_after_focus_loss(4, 4, false));
    assert!(!should_close_after_focus_loss(5, 4, false));
}

#[test]
fn focus_snapshot_releases_refcell_borrow_before_win32_work() {
    let cell = std::cell::RefCell::new(PickerUi {
        generation: 9,
        kind: PickerKind::OverlayPosition,
        choices: Vec::new(),
        list: HWND(std::ptr::null_mut()),
        close_action: None,
        hovered_index: None,
        font: None,
    });
    let (list, generation) = picker_focus_snapshot(&cell);
    assert!(list.0.is_null());
    assert_eq!(generation, 9);
    assert!(cell.try_borrow_mut().is_ok());
}

#[test]
fn picker_close_action_is_claimed_once() {
    let cell = std::cell::RefCell::new(PickerUi {
        generation: 9,
        kind: PickerKind::OverlayPosition,
        choices: Vec::new(),
        list: HWND(std::ptr::null_mut()),
        close_action: None,
        hovered_index: None,
        font: None,
    });
    assert!(claim_close(&cell, PickerCloseAction::Commit));
    assert!(!claim_close(&cell, PickerCloseAction::Cancel));
    assert_eq!(cell.borrow().close_action, Some(PickerCloseAction::Commit));
}
