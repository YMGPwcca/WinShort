//! Selection for the picker.

use super::model::{PickerChoiceValue, PickerCommit, PickerKind};
use super::window::{
    PickerUi, LB_GETCOUNT, LB_GETCURSEL, LB_GETSEL, LB_GETSELCOUNT, LB_GETSELITEMS, LB_SETSEL,
};
use crate::platform::window as win;
use crate::ui::presentation::{AllowlistMode, DeviceCycleSelection};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};

const ALLOWLIST_MODE_COUNT: usize = 3;

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

    if focused < ALLOWLIST_MODE_COUNT {
        let had_device_selection = (ALLOWLIST_MODE_COUNT..count).any(selected_at);
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
            for index in ALLOWLIST_MODE_COUNT..count {
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
        for index in 0..ALLOWLIST_MODE_COUNT.min(count) {
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

pub(super) fn selected_commit(
    parent: HWND,
    list: HWND,
) -> Result<Option<PickerCommit>, &'static str> {
    let cell = unsafe { win::state_cell::<PickerUi>(parent) }
        .ok_or("picker state missing while reading selection")?;
    let kind = cell.borrow().kind;
    if kind.is_multi_select() {
        let indices = selected_indices(list)?;
        let ui = cell.borrow();
        return if kind == PickerKind::DisplayOutputs {
            let mut outputs = Vec::with_capacity(indices.len());
            for index in indices {
                let choice = ui
                    .choices
                    .get(index)
                    .ok_or("picker selected index is out of range")?;
                match &choice.value {
                    PickerChoiceValue::DisplayOutput(route) => outputs.push(route.clone()),
                    PickerChoiceValue::Commit(_)
                    | PickerChoiceValue::AllowlistMode(_)
                    | PickerChoiceValue::AllowlistEndpoint(_) => {
                        return Err("display output picker contains a non-output choice")
                    }
                }
            }
            Ok(Some(PickerCommit::DisplayOutputs(outputs)))
        } else {
            selected_allowlist_commit(kind, &ui.choices, indices)
        };
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
        return Ok(None);
    }
    let ui = cell.borrow();
    let choice = ui
        .choices
        .get(index as usize)
        .ok_or("picker current selection is out of range")?;
    match &choice.value {
        PickerChoiceValue::Commit(commit) if commit.kind() == ui.kind => Ok(Some(commit.clone())),
        PickerChoiceValue::Commit(_) => Err("picker commit belongs to another picker kind"),
        PickerChoiceValue::AllowlistMode(_)
        | PickerChoiceValue::AllowlistEndpoint(_)
        | PickerChoiceValue::DisplayOutput(_) => {
            Err("single-select picker contains a multi-select choice")
        }
    }
}

fn selected_indices(list: HWND) -> Result<Vec<usize>, &'static str> {
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
        return Err("could not read picker multi-selection count");
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
            return Err("could not read picker multi-selection items");
        }
        indices.truncate(copied as usize);
    }
    indices
        .into_iter()
        .map(|index| {
            usize::try_from(index).map_err(|_| "picker returned a negative selected index")
        })
        .collect()
}

fn selected_allowlist_commit(
    kind: PickerKind,
    choices: &[super::model::PickerChoice],
    indices: Vec<usize>,
) -> Result<Option<PickerCommit>, &'static str> {
    let mut mode = None;
    let mut endpoints = Vec::new();
    for index in indices {
        let choice = choices
            .get(index)
            .ok_or("picker selected index is out of range")?;
        match &choice.value {
            PickerChoiceValue::AllowlistMode(value) => {
                if mode.replace(*value).is_some() {
                    return Err("allowlist picker has multiple modes selected");
                }
            }
            PickerChoiceValue::AllowlistEndpoint(endpoint) => endpoints.push(endpoint.clone()),
            PickerChoiceValue::Commit(_) | PickerChoiceValue::DisplayOutput(_) => {
                return Err("allowlist picker contains an incompatible choice")
            }
        }
    }
    let Some(mode) = mode else {
        return Ok(None);
    };
    let selection = match mode {
        AllowlistMode::All => DeviceCycleSelection::All,
        AllowlistMode::Disabled => DeviceCycleSelection::Disabled,
        AllowlistMode::Selected => DeviceCycleSelection::selected(endpoints)?,
    };
    match kind {
        PickerKind::InputAllowlist => Ok(Some(PickerCommit::InputAllowlist(selection))),
        PickerKind::OutputAllowlist => Ok(Some(PickerCommit::OutputAllowlist(selection))),
        _ => Err("non-allowlist picker requested allowlist selection"),
    }
}
