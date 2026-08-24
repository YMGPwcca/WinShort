//! Global low-level keyboard hook thread (spec §10–§15).
//!
//! The callback does only: decode, refresh the immutable binding table when its
//! revision changed, run the pure engine, PostMessageW, return. No COM, audio,
//! file access, rendering, or action execution.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, mpsc};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostMessageW, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED, MSG, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::config::ConfigHandle;
use crate::error::{Error, Result};
use crate::event::{HotkeyAction, WM_APP_ACTION};
use crate::keyboard::binding::{BindingTable, Hotkey, ModifierMask, VirtualKey};
use crate::keyboard::engine::{EngineOutcome, KeyboardEngine, RawKeyEvent};

struct HookState {
    main_hwnd_raw: isize,
    engine: KeyboardEngine,
    /// Empty table used while hotkeys are suspended.
    suspended_bindings: std::sync::Arc<BindingTable>,
    config: Arc<ConfigHandle>,
    suspended: Arc<AtomicBool>,
}

static HOOK_STATE: AtomicPtr<HookState> = AtomicPtr::new(std::ptr::null_mut());

pub struct KeyboardService {
    thread_id: u32,
    suspended: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl KeyboardService {
    pub fn start(hwnd: HWND, config: Arc<ConfigHandle>) -> Result<Self> {
        let suspended = Arc::new(AtomicBool::new(false));
        let suspended_worker = Arc::clone(&suspended);
        let hwnd_raw = hwnd.0 as isize;
        let (ready_tx, ready_rx) = mpsc::sync_channel::<std::result::Result<u32, Error>>(1);

        let join = std::thread::Builder::new()
            .name("winshort-keyboard".into())
            .spawn(move || keyboard_thread(hwnd_raw, config, suspended_worker, ready_tx))
            .map_err(|e| Error::internal(format!("spawn keyboard thread: {e}")))?;

        let thread_id = ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| Error::internal("keyboard hook startup timeout"))??;
        Ok(Self { thread_id, suspended, join: Some(join) })
    }

    pub fn set_suspended(&self, suspended: bool) {
        self.suspended.store(suspended, Ordering::Release);
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::Acquire)
    }

    pub fn shutdown(&mut self) {
        if self.thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            self.thread_id = 0;
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for KeyboardService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn keyboard_thread(
    hwnd_raw: isize,
    config: Arc<ConfigHandle>,
    suspended: Arc<AtomicBool>,
    ready: mpsc::SyncSender<std::result::Result<u32, Error>>,
) {
    let state = Box::new(HookState {
        main_hwnd_raw: hwnd_raw,
        engine: KeyboardEngine::new(),
        suspended_bindings: Arc::new(BindingTable::default()),
        config,
        suspended,
    });
    let state_ptr = Box::into_raw(state);
    HOOK_STATE.store(state_ptr, Ordering::Release);

    let hook = install_hook();
    let hook = match hook {
        Ok(hook) => hook,
        Err(e) => {
            HOOK_STATE.store(std::ptr::null_mut(), Ordering::Release);
            unsafe { drop(Box::from_raw(state_ptr)); }
            let _ = ready.send(Err(e));
            return;
        }
    };

    let thread_id = unsafe { GetCurrentThreadId() };
    let _ = ready.send(Ok(thread_id));
    crate::info!("keyboard hook installed (thread {thread_id})");

    let mut msg = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
    HOOK_STATE.store(std::ptr::null_mut(), Ordering::Release);
    unsafe { drop(Box::from_raw(state_ptr)); }
    crate::info!("keyboard hook uninstalled");
}

fn install_hook() -> Result<HHOOK> {
    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|e| Error::win("GetModuleHandleW(keyboard)", &e))?;
    unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            Some(HINSTANCE(module.0)),
            0,
        )
        .map_err(|e| Error::win("SetWindowsHookExW(WH_KEYBOARD_LL)", &e))
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let ptr = HOOK_STATE.load(Ordering::Acquire);
    if ptr.is_null() {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let state = &mut *ptr;

    let down = match wparam.0 as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => true,
        WM_KEYUP | WM_SYSKEYUP => false,
        _ => return CallNextHookEx(None, code, wparam, lparam),
    };
    let data = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let injected = data.flags.contains(LLKHF_INJECTED)
        || data.flags.contains(LLKHF_LOWER_IL_INJECTED);
    let event = RawKeyEvent {
        vk: data.vkCode as u16,
        extended: data.flags.contains(LLKHF_EXTENDED),
        down,
        injected,
    };

    // Lock-free snapshot read (#10): the table is rebuilt by ConfigHandle on
    // Save; the callback only loads it. No RwLock, no HashMap rebuild here.
    let table_guard = state.config.bindings();
    let table: &BindingTable = if state.suspended.load(Ordering::Acquire) {
        &state.suspended_bindings
    } else {
        &table_guard
    };

    match state.engine.on_event(event, table) {
        EngineOutcome::Pass => CallNextHookEx(None, code, wparam, lparam),
        EngineOutcome::Swallow => LRESULT(1),
        EngineOutcome::Dispatch { action, dirty_win_chord } => {
            let hwnd = HWND(state.main_hwnd_raw as *mut _);
            let _ = PostMessageW(
                Some(hwnd),
                WM_APP_ACTION,
                WPARAM(action.pack()),
                LPARAM(if dirty_win_chord { 1 } else { 0 }),
            );
            LRESULT(1)
        }
    }
}

pub fn build_bindings(config: &crate::config::Config) -> BindingTable {
    let mut table = BindingTable::default();
    if let Some(hotkey) = config.hotkeys.toggle_microphone {
        table.insert(hotkey, HotkeyAction::ToggleMicrophone);
    }
    if let Some(hotkey) = config.hotkeys.toggle_output {
        table.insert(hotkey, HotkeyAction::ToggleOutput);
    }
    if let Some(hotkey) = config.hotkeys.toggle_foreground_audio {
        table.insert(hotkey, HotkeyAction::ToggleForegroundAppAudio);
    }
    // Reserved Win+1..9 desktop shortcuts: insert only into vacant slots.
    // A user hotkey occupying the same (mods, key) keeps priority; validate()
    // rejects that combination when win_number_switching is on, so this is
    // defense in depth — never silently overwrite (#12).
    if config.virtual_desktops.enabled && config.virtual_desktops.win_number_switching {
        for number in 1u16..=9 {
            let reserved = Hotkey {
                modifiers: ModifierMask::WIN,
                key: VirtualKey(0x30 + number),
            };
            if table.conflicts(&reserved).is_none() {
                table.insert(reserved, HotkeyAction::SwitchDesktop((number - 1) as u8));
            } else {
                // User binding keeps the slot; validate() flags this config.
                crate::warn_!(concat!(
                    "user hotkey occupies a reserved Win+digit slot; ",
                    "desktop shortcut disabled for it"
                ));
            }
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_builds_all_default_bindings() {
        let table = build_bindings(&crate::config::Config::default());
        assert_eq!(table.len(), 12); // 3 audio + Win+1..9
        assert_eq!(
            table.lookup(ModifierMask::WIN, VirtualKey(b'9' as u16)),
            Some(HotkeyAction::SwitchDesktop(8))
        );
    }

    #[test]
    fn reserved_slots_never_overwrite_user_bindings() {
        let mut cfg = crate::config::Config::default();
        cfg.hotkeys.toggle_microphone = Some(Hotkey {
            modifiers: ModifierMask::WIN,
            key: VirtualKey(b'5' as u16),
        });
        let table = build_bindings(&cfg);
        // User binding keeps priority over the reserved Win+5 slot.
        assert_eq!(
            table.lookup(ModifierMask::WIN, VirtualKey(b'5' as u16)),
            Some(HotkeyAction::ToggleMicrophone)
        );
        // Other digits still get their desktop shortcuts (8 = 3 audio + 8 of 9 digits).
        assert_eq!(table.len(), 11);
    }
}
