//! Selection for the picker.

use super::model::{PickerKind, PickerValue};
use super::window::{
    PickerUi, LB_GETCOUNT, LB_GETCURSEL, LB_GETSEL, LB_GETSELCOUNT, LB_GETSELITEMS, LB_SETSEL,
};
use crate::platform::window as win;
use crate::ui::presentation::AllowlistMode;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};

pub(super) fn normalize_multi_selection(kind: PickerKind, list: HWND) {
    if !kind.is_allowlist() {
        return;
    }
    let focused = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            list,
            LB_GETCURSEL,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
    }
    .0;
    if focused < 0 {
        return;
    }
    let focused = focused as usize;
    let count = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            list,
            LB_GETCOUNT,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
    }
    .0
    .max(0) as usize;
    let selected_at = |index: usize| unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            list,
            LB_GETSEL,
            Some(WPARAM(index)),
            Some(LPARAM(0)),
        )
    }
    .0
        != 0;

    if focused < 3 {
        let had_device_selection = (3..count).any(selected_at);
        for index in 0..count {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_SETSEL,
                    Some(WPARAM(0)),
                    Some(LPARAM(index as isize)),
                );
            }
        }
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                list,
                LB_SETSEL,
                Some(WPARAM(1)),
                Some(LPARAM(focused as isize)),
            );
        }
        if focused == 1 && !had_device_selection {
            for index in 3..count {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        list,
                        LB_SETSEL,
                        Some(WPARAM(1)),
                        Some(LPARAM(index as isize)),
                    );
                }
            }
        }
    } else if selected_at(focused) {
        // Device clicks select the explicit mode but retain the other device
        // checks, allowing a real multi-device allowlist.
        for index in 0..3.min(count) {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_SETSEL,
                    Some(WPARAM(0)),
                    Some(LPARAM(index as isize)),
                );
            }
        }
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                list,
                LB_SETSEL,
                Some(WPARAM(1)),
                Some(LPARAM(1)),
            );
        }
    }
}

pub(super) fn selected_value(parent: HWND, list: HWND) -> Option<(PickerKind, PickerValue)> {
    let cell = unsafe { win::state_cell::<PickerUi>(parent) }?;
    let kind = cell.borrow().kind;
    if kind.is_multi_select() {
        let count = unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                list,
                LB_GETSELCOUNT,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
        }
        .0;
        if count < 0 {
            return None;
        }
        let mut indices = vec![0i32; count as usize];
        if count > 0 {
            let copied = unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_GETSELITEMS,
                    Some(WPARAM(count as usize)),
                    Some(LPARAM(indices.as_mut_ptr() as isize)),
                )
            }
            .0;
            if copied < 0 {
                return None;
            }
            indices.truncate(copied as usize);
        }
        let ui = cell.borrow();
        if kind == PickerKind::DisplayOutputs {
            let outputs = indices
                .into_iter()
                .filter_map(|index| ui.choices.get(index as usize))
                .filter_map(|choice| match &choice.value {
                    PickerValue::DisplayOutput(route) => Some(route.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            return Some((kind, PickerValue::DisplayOutputs(outputs)));
        }
        let mut mode = None;
        let mut endpoints = Vec::new();
        for index in indices {
            let Some(choice) = ui.choices.get(index as usize) else {
                continue;
            };
            match &choice.value {
                PickerValue::AllowlistMode(value) => mode = Some(*value),
                PickerValue::Allowlist(Some(values)) => endpoints.extend(values.iter().cloned()),
                _ => {}
            }
        }
        let allowlist = match mode.unwrap_or(AllowlistMode::Disabled) {
            AllowlistMode::All => None,
            AllowlistMode::Selected => Some(endpoints),
            AllowlistMode::Disabled => Some(Vec::new()),
        };
        return Some((kind, PickerValue::Allowlist(allowlist)));
    }

    let index = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            list,
            LB_GETCURSEL,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
    }
    .0;
    if index < 0 {
        return None;
    }
    let ui = cell.borrow();
    ui.choices
        .get(index as usize)
        .map(|choice| (ui.kind, choice.value.clone()))
}
