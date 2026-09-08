//! State and typed actions for the native text prompt.

use windows::Win32::Foundation::HWND;

#[derive(Debug, Clone)]
pub(crate) enum PromptAction {
    RenameProfile {
        profile_id: String,
    },
    EditRoute {
        profile_id: String,
        route_index: usize,
    },
}

#[derive(Clone, Copy)]
pub(super) struct PromptControls {
    pub(super) edit: HWND,
    pub(super) ok: HWND,
    pub(super) cancel: HWND,
}

pub(super) struct PromptState {
    pub(super) action: PromptAction,
    pub(super) controls: Option<PromptControls>,
    pub(super) submit_text: String,
}
