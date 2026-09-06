use crate::error::{Error, Result};
use crate::event::{self, AppEvent};
use crate::platform::window as win;
use std::cell::RefCell;
use std::sync::OnceLock;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT, HFONT};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetWindowTextLengthW, GetWindowTextW, MoveWindow, SendMessageW,
    SetWindowTextW, ShowWindow, BN_CLICKED, CREATESTRUCTW, CW_USEDEFAULT, ES_AUTOHSCROLL, HMENU,
    SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_KEYDOWN,
    WM_NCCREATE, WM_NCDESTROY, WM_SETFONT, WM_SIZE, WS_BORDER, WS_CAPTION, WS_CHILD,
    WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

const CLASS_NAME: &str = "WinShort.TextPrompt";
const IDC_EDIT: usize = 100;
const IDC_OK: usize = 101;
const IDC_CANCEL: usize = 102;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
static REGISTERED: OnceLock<u16> = OnceLock::new();

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

struct PromptState {
    action: PromptAction,
    edit: HWND,
    ok: HWND,
    cancel: HWND,
    submit_text: String,
}

pub(crate) struct TextPrompt {
    hwnd: HWND,
}

impl TextPrompt {
    pub(crate) fn create(
        owner: HWND,
        action: PromptAction,
        title: &str,
        submit_text: &str,
        initial: &str,
    ) -> Result<Self> {
        let _atom = win::register_class_once(&REGISTERED, CLASS_NAME, Some(prompt_wndproc))?;
        let mut state = win::WindowCreation::new(PromptState {
            action,
            edit: HWND::default(),
            ok: HWND::default(),
            cancel: HWND::default(),
            submit_text: submit_text.into(),
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                PCWSTR(HSTRING::from(CLASS_NAME).as_ptr()),
                PCWSTR(HSTRING::from(title).as_ptr()),
                WS_POPUP | WS_CAPTION | WS_SYSMENU,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                520,
                168,
                Some(owner),
                None,
                None,
                Some(state.parameter()),
            )
        }
        .map_err(|error| Error::win("CreateWindowExW(text prompt)", &error))?;
        // SAFETY: this constructor exclusively owns the newly created HWND.
        let construction = unsafe { win::WindowConstructionGuard::new(hwnd) };
        let Some(cell) = (unsafe { win::state_cell::<PromptState>(hwnd) }) else {
            return Err(Error::internal("text prompt state missing"));
        };
        let edit = cell.borrow().edit;
        let initial = HSTRING::from(initial);
        unsafe {
            if SetWindowTextW(edit, PCWSTR(initial.as_ptr())).is_err() {
                crate::warn_!("could not initialize text prompt contents");
            }
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetFocus(Some(edit));
        }
        let hwnd = construction.complete();
        Ok(Self { hwnd })
    }

    pub(crate) fn close(&mut self) {
        if !self.hwnd.0.is_null() {
            if unsafe { DestroyWindow(self.hwnd) }.is_err() {
                crate::warn_!("could not destroy text prompt window");
            }
            self.hwnd = HWND::default();
        }
    }
}

impl Drop for TextPrompt {
    fn drop(&mut self) {
        self.close();
    }
}

unsafe extern "system" fn prompt_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => handle_nccreate(hwnd, msg, wparam, lparam),
        WM_CREATE => handle_create(hwnd),
        WM_COMMAND => handle_command(hwnd, wparam),
        WM_KEYDOWN => handle_keydown(hwnd, wparam),
        WM_SIZE => handle_resize(hwnd),
        WM_CLOSE => cancel_prompt(hwnd),
        WM_NCDESTROY => handle_ncdestroy(hwnd, msg, wparam, lparam),
        _ => unsafe {
            windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
        },
    }
}

fn state_cell(hwnd: HWND) -> Option<&'static RefCell<PromptState>> {
    // SAFETY: only called while the prompt HWND is alive on its owning UI
    // thread. WM_NCDESTROY cleanup is owned by the shared window plumbing.
    unsafe { win::state_cell::<PromptState>(hwnd) }
}

fn handle_nccreate(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
    let Some(state) = (unsafe { win::WindowCreation::<PromptState>::take_from(cs.lpCreateParams) })
    else {
        return LRESULT(0);
    };
    win::store_state_ptr(hwnd, win::WindowState::new(state));
    win::def_proc(hwnd, msg, wparam, lparam)
}

fn handle_create(hwnd: HWND) -> LRESULT {
    match create_children(hwnd) {
        Ok(()) => LRESULT(0),
        Err(error) => {
            crate::error_!("text prompt child creation failed: {error}");
            LRESULT(-1)
        }
    }
}

fn create_children(hwnd: HWND) -> Result<()> {
    let Some(cell) = state_cell(hwnd) else {
        return Err(Error::internal("text prompt state missing during create"));
    };
    let submit_text = cell.borrow().submit_text.clone();
    let edit = create_child(
        hwnd,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        IDC_EDIT,
    )?;
    let ok = create_child(
        hwnd,
        "BUTTON",
        &submit_text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        IDC_OK,
    )?;
    let cancel = create_child(
        hwnd,
        "BUTTON",
        "Cancel",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        IDC_CANCEL,
    )?;
    apply_default_font(edit);
    apply_default_font(ok);
    apply_default_font(cancel);
    {
        let mut state = cell.borrow_mut();
        state.edit = edit;
        state.ok = ok;
        state.cancel = cancel;
    }
    layout_children(hwnd, edit, ok, cancel);
    Ok(())
}

fn create_child(
    parent: HWND,
    class: &str,
    text: &str,
    style: WINDOW_STYLE,
    id: usize,
) -> Result<HWND> {
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(HSTRING::from(class).as_ptr()),
            PCWSTR(HSTRING::from(text).as_ptr()),
            style,
            0,
            0,
            10,
            10,
            Some(parent),
            Some(HMENU(id as *mut _)),
            None,
            None,
        )
    }
    .map_err(|error| Error::win("CreateWindowExW(text prompt child)", &error))
}

fn apply_default_font(hwnd: HWND) {
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    if font.0.is_null() {
        return;
    }
    let hfont = HFONT(font.0);
    unsafe {
        let _ = SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(hfont.0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

fn handle_command(hwnd: HWND, wparam: WPARAM) -> LRESULT {
    if hiword(wparam.0) != BN_CLICKED as u16 {
        return LRESULT(0);
    }
    match loword(wparam.0) as usize {
        IDC_OK => submit_prompt(hwnd),
        IDC_CANCEL => cancel_prompt(hwnd),
        _ => LRESULT(0),
    }
}

fn handle_keydown(hwnd: HWND, wparam: WPARAM) -> LRESULT {
    match wparam.0 as u32 {
        VK_RETURN => submit_prompt(hwnd),
        VK_ESCAPE => cancel_prompt(hwnd),
        _ => LRESULT(0),
    }
}

fn submit_prompt(hwnd: HWND) -> LRESULT {
    let Some(cell) = state_cell(hwnd) else {
        return LRESULT(0);
    };
    // Copy everything needed before posting or destroying; those operations can
    // synchronously re-enter native window code.
    let (action, edit) = {
        let state = cell.borrow();
        (state.action.clone(), state.edit)
    };
    let text = read_window_text(edit);
    let event = match action {
        PromptAction::RenameProfile { profile_id } => AppEvent::DisplayProfileRenameSubmitted {
            profile_id,
            name: text,
        },
        PromptAction::EditRoute {
            profile_id,
            route_index,
        } => AppEvent::DisplayProfileRouteEditSubmitted {
            profile_id,
            route_index,
            value: text,
        },
    };
    event::post_main(event);
    close_prompt(hwnd)
}

fn cancel_prompt(hwnd: HWND) -> LRESULT {
    // The Control Center owns the TextPrompt wrapper even after the native HWND
    // closes itself. Wake the owner so it clears that wrapper instead of keeping
    // a stale handle until the next prompt or shutdown.
    event::post_main(AppEvent::DisplayProfileRenameCancelled);
    close_prompt(hwnd)
}

fn read_window_text(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; len as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if copied <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..copied as usize])
}

fn handle_resize(hwnd: HWND) -> LRESULT {
    let Some(cell) = state_cell(hwnd) else {
        return LRESULT(0);
    };
    let (edit, ok, cancel) = {
        let state = cell.borrow();
        (state.edit, state.ok, state.cancel)
    };
    layout_children(hwnd, edit, ok, cancel);
    LRESULT(0)
}

fn layout_children(hwnd: HWND, edit: HWND, ok: HWND, cancel: HWND) {
    let mut rect = windows::Win32::Foundation::RECT::default();
    if unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect) }.is_err() {
        return;
    }
    let width = (rect.right - rect.left).max(0);
    let button_width = 96;
    let gap = 10;
    unsafe {
        let _ = MoveWindow(edit, 16, 18, (width - 32).max(10), 30, true);
        let _ = MoveWindow(
            cancel,
            width - 16 - button_width,
            70,
            button_width,
            32,
            true,
        );
        let _ = MoveWindow(
            ok,
            width - 16 - button_width * 2 - gap,
            70,
            button_width,
            32,
            true,
        );
    }
}

fn close_prompt(hwnd: HWND) -> LRESULT {
    if unsafe { DestroyWindow(hwnd) }.is_err() {
        crate::warn_!("could not destroy text prompt window");
    }
    LRESULT(0)
}

fn handle_ncdestroy(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        drop(win::take_state::<PromptState>(hwnd));
    }
    win::def_proc(hwnd, msg, wparam, lparam)
}

fn loword(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

fn hiword(value: usize) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}
