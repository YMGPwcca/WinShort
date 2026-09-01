//! Global low-level keyboard hook thread (spec §10–§15).
//!
//! The callback does only: decode, refresh the immutable binding table when its
//! revision changed, run the pure engine, PostMessageW, return. No COM, audio,
//! file access, rendering, or action execution.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{mpsc, Arc};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostMessageW, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::config::ConfigHandle;
use crate::error::{Error, Result};
use crate::event::{HotkeyAction, WM_APP_ACTION};
use crate::keyboard::binding::{numbered_desktop_family, BindingTable, ModifierMask, VirtualKey};
use crate::keyboard::engine::{EngineOutcome, KeyboardEngine, RawKeyEvent};
use crate::keyboard::keystate::{normalize_vk, KeyState};

struct HookState {
    main_hwnd_raw: isize,
    engine: KeyboardEngine,
    /// Empty table used while hotkeys are suspended.
    suspended_bindings: std::sync::Arc<BindingTable>,
    config: Arc<ConfigHandle>,
    suspended: Arc<AtomicBool>,
}

static HOOK_STATE: AtomicPtr<HookState> = AtomicPtr::new(std::ptr::null_mut());

/// Whether the low-level hook state is currently published to callbacks.
pub fn hook_active() -> bool {
    !HOOK_STATE.load(Ordering::Acquire).is_null()
}

/// One recorded shortcut delivered to the Settings hotkey recorder (#14).
/// `key: None` means cancelled (Esc).
#[derive(Debug, Clone, Copy)]
pub struct CapturedChord {
    pub modifiers: ModifierMask,
    pub key: Option<VirtualKey>,
}

/// Capture state machine (#47), one `AtomicU64` word:
///
/// ```text
/// [generation: u32][state: 2 bits][chord: 24 bits]
/// ```
///
/// state: 0 = Inactive, 1 = Armed, 2 = Completed.
///
/// The generation increments on every [`begin_capture`], so a stale LL
/// callback that observed an older session can never publish into, disarm,
/// or complete a newer one: completion is a single CAS
/// `Armed(N) -> Completed(N, chord)` that fails the moment any UI operation
/// changed the word. No locks, no allocation — safe for WH_KEYBOARD_LL.
///
/// Generation wraparound: the counter is u32. A stale callback would need to
/// outlive 2^32 intervening `begin_capture` calls to be mistaken for current;
/// treated as impossible and documented here.
use std::sync::atomic::AtomicU64;

const CAPTURE_STATE_MASK: u64 = 0xC000_0000;
const CAPTURE_CHORD_MASK: u64 = 0x3FFF_FFFF;
const CAPTURE_INACTIVE: u64 = 0 << 30;
const CHORD_TAG_KEY: u32 = 1;
const CHORD_TAG_CANCEL: u32 = 2;
const CAPTURE_ARMED: u64 = 1 << 30;
const CAPTURE_COMPLETED: u64 = 2 << 30;

static CAPTURE_STATE: AtomicU64 = AtomicU64::new(0);

fn capture_word(gen: u32, state: u64, chord: u32) -> u64 {
    ((gen as u64) << 32) | state | (chord as u64 & CAPTURE_CHORD_MASK)
}

fn capture_generation(word: u64) -> u32 {
    (word >> 32) as u32
}

fn capture_state_of(word: u64) -> u64 {
    word & CAPTURE_STATE_MASK
}

fn capture_chord_of(word: u64) -> u32 {
    (word & CAPTURE_CHORD_MASK) as u32
}

/// Start global capture: the hook swallows all keyboard input and publishes
/// the first completed chord (or Esc cancellation) into this session.
/// Poll [`take_captured_chord`] from the recorder UI. Ends automatically when
/// a chord is delivered or via [`end_capture`].
pub fn begin_capture() {
    // New generation on every begin; CAS loop makes the increment
    // linearizable against a concurrently completing previous session.
    loop {
        let cur = CAPTURE_STATE.load(Ordering::Acquire);
        let next = capture_word(capture_generation(cur).wrapping_add(1), CAPTURE_ARMED, 0);
        if CAPTURE_STATE
            .compare_exchange(cur, next, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
    }
}

/// Stop capturing. Invalidates every callback still holding the previous
/// generation: their completion CAS can no longer succeed.
pub fn end_capture() {
    loop {
        let cur = CAPTURE_STATE.load(Ordering::Acquire);
        let next = capture_word(capture_generation(cur), CAPTURE_INACTIVE, 0);
        if CAPTURE_STATE
            .compare_exchange(cur, next, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
    }
}

/// The generation a callback captured while arming was active, if armed.
#[cfg(test)]
pub fn capture_token() -> Option<u32> {
    let cur = CAPTURE_STATE.load(Ordering::Acquire);
    (capture_state_of(cur) == CAPTURE_ARMED).then(|| capture_generation(cur))
}

/// Production-safe token fetch used by the hook callback.
fn capture_token_live() -> Option<u32> {
    let cur = CAPTURE_STATE.load(Ordering::Acquire);
    (capture_state_of(cur) == CAPTURE_ARMED).then(|| capture_generation(cur))
}

/// Whether a recorder capture session is currently armed.
pub fn capture_active() -> bool {
    capture_token_live().is_some()
}

fn pack_chord(modifiers: ModifierMask, key: Option<VirtualKey>) -> u32 {
    let tag = if key.is_some() {
        CHORD_TAG_KEY
    } else {
        CHORD_TAG_CANCEL
    };
    (tag << 24) | ((modifiers.bits() as u32) << 16) | (key.map_or(0, |k| k.code()) as u32)
}

fn unpack_chord(word: u32) -> Option<CapturedChord> {
    match word >> 24 {
        CHORD_TAG_KEY => Some(CapturedChord {
            modifiers: ModifierMask::from_bits((word >> 16) as u8),
            key: Some(VirtualKey((word & 0xFFFF) as u16)),
        }),
        CHORD_TAG_CANCEL => Some(CapturedChord {
            modifiers: ModifierMask::NONE,
            key: None,
        }),
        _ => None,
    }
}

/// Consume the captured chord, if this session completed. Exactly-once via
/// Completed -> Inactive CAS; each completed session yields one result.
pub fn take_captured_chord() -> Option<CapturedChord> {
    loop {
        let cur = CAPTURE_STATE.load(Ordering::Acquire);
        if capture_state_of(cur) != CAPTURE_COMPLETED {
            return None;
        }
        let next = capture_word(capture_generation(cur), CAPTURE_INACTIVE, 0);
        if CAPTURE_STATE
            .compare_exchange(cur, next, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return unpack_chord(capture_chord_of(cur));
        }
    }
}

/// Commit a completion for session `token`. Succeeds only if that exact
/// generation is STILL armed at commit time (#47): end_capture/begin_capture
/// in between make the CAS fail and the stale callback is rejected.
fn complete_capture(token: u32, chord: CapturedChord) -> bool {
    loop {
        let cur = CAPTURE_STATE.load(Ordering::Acquire);
        let want = capture_word(token, CAPTURE_ARMED, 0);
        if cur != want {
            return false;
        }
        let done = capture_word(
            token,
            CAPTURE_COMPLETED,
            pack_chord(chord.modifiers, chord.key),
        );
        match CAPTURE_STATE.compare_exchange(cur, done, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(actual) => {
                // Raced with end/begin/another completion of THIS generation:
                // another callback already completed it, or the session died.
                if capture_generation(actual) == token
                    && capture_state_of(actual) == CAPTURE_COMPLETED
                {
                    return false; // exactly-once preserved
                }
                if capture_generation(actual) != token {
                    return false; // session replaced
                }
                // Same generation still armed but word changed? Impossible
                // (armed words are identical); retry defensively.
            }
        }
    }
}

/// Hook disposition for an event that arrived while a capture session was
/// armed (#48). Completion success and hook disposition are independent: a
/// STALE callback (session replaced/cancelled mid-flight) must still swallow
/// the physical key even though its completion was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureDisposition {
    /// Suppress the event from all other applications.
    Swallow,
    /// Event outside the supported VK domain; forward normally.
    Pass,
}

/// Handle one raw event while capture is active. Disposition is SWALLOW for
/// everything in the supported VK domain — including stale-token events whose
/// completion was rejected by #47's generation check — so a capture-owned key
/// can never leak into the foreground application.
fn capture_event(state: &mut HookState, ev: RawKeyEvent, token: u32) -> CaptureDisposition {
    let vk = normalize_vk(ev.vk, ev.extended);
    if vk >= 256 {
        // Unclassifiable VK: intentionally forwarded (pre-#48 behavior).
        return CaptureDisposition::Pass;
    }
    // Track modifier state in the engine so the mask matches real physics.
    // SAFE across stale tokens: LL callbacks are serialized on this thread and
    // engine state models PHYSICAL keys/modifiers that persist across a
    // cancel/re-arm boundary; only publication is generation-guarded (CAS).
    let _ = state.engine.on_event(ev, &state.suspended_bindings);
    if !ev.down || KeyState::is_modifier(vk) {
        return CaptureDisposition::Swallow;
    }

    // Publish lock-free (#45): `swap` guarantees exactly-once delivery even
    // under autorepeat races, and no receiver object can go stale.
    let chord = if vk == 0x1B {
        CapturedChord {
            modifiers: ModifierMask::NONE,
            key: None,
        } // Esc cancels
    } else {
        CapturedChord {
            modifiers: state.engine.current_modifiers(),
            key: Some(VirtualKey(vk)),
        }
    };
    // Commit only if session `token` is STILL the armed one (#47). A stale
    // callback from a cancelled/replaced generation fails the CAS — but the
    // disposition remains SWALLOW so the key never reaches the foreground app.
    let _committed = complete_capture(token, chord);
    state.engine.reset();
    CaptureDisposition::Swallow
}

/// Thread message asking the hook thread to clear engine state (#13).
const WM_APP_RESET_STATE: u32 = 0x8003; // WM_APP + 3

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
        Ok(Self {
            thread_id,
            suspended,
            join: Some(join),
        })
    }

    pub fn set_suspended(&self, suspended: bool) {
        self.suspended.store(suspended, Ordering::Release);
    }

    /// Ask the keyboard thread to clear engine state (lock/sleep safety
    /// valve). Never touches engine memory cross-thread — posts a wake (#13).
    pub fn reset_state(&self) {
        if self.thread_id != 0 {
            unsafe {
                let _ =
                    PostThreadMessageW(self.thread_id, WM_APP_RESET_STATE, WPARAM(0), LPARAM(0));
            }
        }
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

    // Install first; on success ownership of uninstall moves to HookGuard.
    let hook = match install_hook() {
        Ok(hook) => HookGuard(hook),
        Err(e) => {
            HOOK_STATE.store(std::ptr::null_mut(), Ordering::Release);
            // SAFETY: publication above is undone before the free.
            unsafe {
                drop(Box::from_raw(state_ptr));
            }
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
        if msg.message == WM_APP_RESET_STATE {
            // Session lock/unlock or power transition: clear chord state so
            // keys physically released while away cannot stay "held".
            let ptr = HOOK_STATE.load(Ordering::Acquire);
            if !ptr.is_null() {
                // SAFETY: HOOK_STATE is owned by this thread; it is only
                // nulled after GetMessageW returns (thread teardown).
                unsafe {
                    (*ptr).engine.reset();
                }
            }
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    // Ordered teardown (#42). WH_KEYBOARD_LL callbacks run only on THIS
    // thread while it pumps messages — none can be in flight here, and none
    // can start once the hook is gone.
    //
    // 1) Stop receiving callbacks entirely.
    drop(hook);
    // 2) Clear callback-visible global state.
    HOOK_STATE.store(std::ptr::null_mut(), Ordering::Release);
    // SAFETY: publication was undone immediately above; this thread is the
    // sole owner of the allocation.
    // 3) Free the state.
    unsafe {
        drop(Box::from_raw(state_ptr));
    }
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

/// Owns an installed `HHOOK`; `UnhookWindowsHookEx` runs on Drop (#42).
///
/// For a `WH_KEYBOARD_LL` hook installed without a DLL, callbacks are
/// dispatched only on the installing thread while it pumps messages, so
/// dropping the guard from that same thread after its message loop has
/// exited cannot race an in-flight callback.
struct HookGuard(HHOOK);

impl Drop for HookGuard {
    fn drop(&mut self) {
        // SAFETY: handle was returned by SetWindowsHookExW and is dropped
        // exactly once here.
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
        }
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code < 0 {
        // SAFETY: required pass-through per LowLevelKeyboardProc contract.
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let ptr = HOOK_STATE.load(Ordering::Acquire);
    if ptr.is_null() {
        // SAFETY: no state installed; plain pass-through.
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    // SAFETY: HOOK_STATE is owned by this hook thread and only nulled after
    // the thread exits its message loop.
    let state = unsafe { &mut *ptr };

    let down = match wparam.0 as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => true,
        WM_KEYUP | WM_SYSKEYUP => false,
        _ => {
            // SAFETY: required pass-through.
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }
    };
    // SAFETY: KBDLLHOOKSTRUCT is owned by the OS for this callback.
    let data = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let injected =
        data.flags.contains(LLKHF_INJECTED) || data.flags.contains(LLKHF_LOWER_IL_INJECTED);
    let event = RawKeyEvent {
        vk: data.vkCode as u16,
        extended: data.flags.contains(LLKHF_EXTENDED),
        down,
        injected,
    };

    if let Some(token) = capture_token_live() {
        // Recorder capture mode (#14/#47): swallow everything; completion is
        // generation-validated, so a delayed callback cannot leak into a
        // newer session.
        // #48: supported events are always swallowed; a stale completion is
        // rejected by the generation check without changing the disposition.
        return match capture_event(state, event, token) {
            CaptureDisposition::Swallow => LRESULT(1),
            CaptureDisposition::Pass => unsafe { CallNextHookEx(None, code, wparam, lparam) },
        };
    }
    // Lock-free snapshot read (#10): the table is rebuilt by ConfigHandle on
    // every main-thread config commit; the callback only loads it. No RwLock,
    // no HashMap rebuild here.
    let table_guard = state.config.bindings();
    let table: &BindingTable = if state.suspended.load(Ordering::Acquire) {
        &state.suspended_bindings
    } else {
        &table_guard
    };

    match state.engine.on_event(event, table) {
        // SAFETY: standard hook chain pass-through.
        EngineOutcome::Pass => unsafe { CallNextHookEx(None, code, wparam, lparam) },
        EngineOutcome::Swallow => LRESULT(1),
        EngineOutcome::Dispatch {
            action,
            dirty_win_chord,
        } => {
            let hwnd = HWND(state.main_hwnd_raw as *mut _);
            // SAFETY: hwnd was valid at thread start and outlives the hook.
            let _ = unsafe {
                PostMessageW(
                    Some(hwnd),
                    WM_APP_ACTION,
                    WPARAM(action.pack()),
                    LPARAM(if dirty_win_chord { 1 } else { 0 }),
                )
            };
            LRESULT(1)
        }
    }
}

fn insert_number_family<F>(table: &mut BindingTable, modifier: ModifierMask, make: F)
where
    F: Fn(u8) -> HotkeyAction,
{
    if modifier.is_empty() {
        return;
    }
    for (number, hotkey) in numbered_desktop_family(modifier).enumerate() {
        if !table.insert(hotkey, make(number as u8)) {
            crate::warn_!("duplicate virtual-desktop family binding ignored: {hotkey}");
        }
    }
}

pub fn build_bindings(config: &crate::config::Config) -> BindingTable {
    let mut table = BindingTable::default();
    let configured = [
        (
            config.hotkeys.toggle_microphone,
            HotkeyAction::ToggleMicrophone,
        ),
        (config.hotkeys.toggle_output, HotkeyAction::ToggleOutput),
        (
            config.hotkeys.toggle_foreground_audio,
            HotkeyAction::ToggleForegroundAppAudio,
        ),
        (
            config.hotkeys.cycle_input_device,
            HotkeyAction::CycleInputDevice,
        ),
        (
            config.hotkeys.cycle_output_device,
            HotkeyAction::CycleOutputDevice,
        ),
        (
            config.hotkeys.foreground_volume_up,
            HotkeyAction::ForegroundVolumeUp,
        ),
        (
            config.hotkeys.foreground_volume_down,
            HotkeyAction::ForegroundVolumeDown,
        ),
    ];
    for (hotkey, action) in configured
        .into_iter()
        .filter_map(|(hotkey, action)| hotkey.map(|hotkey| (hotkey, action)))
    {
        if !table.insert(hotkey, action) {
            crate::warn_!("duplicate hotkey binding ignored: {hotkey}");
        }
    }
    if config.display_profiles.enabled {
        for binding in &config.hotkeys.display_profiles {
            let profile_key = crate::display::profile_id_key(&binding.profile_id);
            if !table.insert(
                binding.hotkey,
                HotkeyAction::ApplyDisplayProfile(profile_key),
            ) {
                crate::warn_!(
                    "duplicate display profile hotkey binding ignored: {}",
                    binding.hotkey
                );
            }
        }
    }
    if config.virtual_desktops.enabled {
        if let Some(previous) = config.virtual_desktops.previous_desktop {
            if !table.insert(previous, HotkeyAction::SwitchPreviousDesktop) {
                crate::warn_!("duplicate previous-desktop binding ignored: {previous}");
            }
        }
        for (hotkey, action, label) in [
            (
                config.virtual_desktops.scratchpad_assign,
                HotkeyAction::AssignScratchpad,
                "scratchpad assignment",
            ),
            (
                config.virtual_desktops.scratchpad_toggle,
                HotkeyAction::ToggleScratchpad,
                "scratchpad toggle",
            ),
        ] {
            if let Some(hotkey) = hotkey {
                if !table.insert(hotkey, action) {
                    crate::warn_!("duplicate {label} binding ignored: {hotkey}");
                }
            }
        }
        if config.virtual_desktops.win_number_switching {
            insert_number_family(
                &mut table,
                config.virtual_desktops.number_modifier,
                HotkeyAction::SwitchDesktop,
            );
        }
        if let Some(modifier) = config.virtual_desktops.move_follow_modifier {
            insert_number_family(&mut table, modifier, HotkeyAction::MoveForegroundToDesktop);
        }
        if let Some(modifier) = config.virtual_desktops.move_silent_modifier {
            insert_number_family(
                &mut table,
                modifier,
                HotkeyAction::MoveForegroundToDesktopSilent,
            );
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::binding::Hotkey;

    /// All capture-state tests mutate process-global CAPTURE_* atomics and
    /// cargo runs tests in parallel threads — serialize them so interleaved
    /// sessions cannot flake (observed on hosted CI).
    static CAPTURE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn capture_guard() -> std::sync::MutexGuard<'static, ()> {
        match CAPTURE_TEST_LOCK.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
    #[test]
    fn profile_hotkeys_bind_stable_id_keys() {
        let mut config = crate::config::Config::default();
        config.hotkeys.display_profiles = vec![
            crate::config::model::DisplayProfileHotkey {
                profile_id: "ai-profile".into(),
                hotkey: Hotkey::parse("Ctrl+Alt+F1").unwrap(),
            },
            crate::config::model::DisplayProfileHotkey {
                profile_id: "gaming-profile".into(),
                hotkey: Hotkey::parse("Ctrl+Alt+F2").unwrap(),
            },
        ];
        let table = build_bindings(&config);
        assert_eq!(
            table.lookup(
                ModifierMask::CTRL.union(ModifierMask::ALT),
                VirtualKey(0x70)
            ),
            Some(HotkeyAction::ApplyDisplayProfile(
                crate::display::profile_id_key("ai-profile",)
            ))
        );
        assert_eq!(
            table.lookup(
                ModifierMask::CTRL.union(ModifierMask::ALT),
                VirtualKey(0x71)
            ),
            Some(HotkeyAction::ApplyDisplayProfile(
                crate::display::profile_id_key("gaming-profile",)
            ))
        );
    }
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
    fn disabled_shortcut_chord_is_not_added_to_active_bindings() {
        let mut config = crate::config::Config::default();
        let hotkey = Hotkey::parse("Ctrl+Alt+F20").unwrap();
        config.hotkeys.toggle_output = None;
        config
            .hotkeys
            .set_disabled_hotkey("toggle_output".into(), hotkey);

        let table = build_bindings(&config);
        assert_eq!(
            table.lookup(hotkey.modifiers, hotkey.key),
            None,
            "disabled chord must stay out of the active table"
        );
        assert_eq!(table.len(), 11);
    }

    #[test]
    fn configurable_desktop_families_bind_all_number_keys() {
        let mut config = crate::config::Config::default();
        config.virtual_desktops.number_modifier = ModifierMask::WIN.union(ModifierMask::ALT);
        config.virtual_desktops.move_follow_modifier = Some(ModifierMask::CTRL);
        config.virtual_desktops.move_silent_modifier =
            Some(ModifierMask::CTRL.union(ModifierMask::ALT));
        config.virtual_desktops.previous_desktop = Some(Hotkey::parse("Ctrl+Alt+F12").unwrap());
        config.virtual_desktops.scratchpad_assign = Some(Hotkey::parse("Ctrl+Alt+F13").unwrap());
        config.virtual_desktops.scratchpad_toggle = Some(Hotkey::parse("Ctrl+Alt+F14").unwrap());
        let table = build_bindings(&config);
        assert_eq!(
            table.lookup(
                ModifierMask::WIN.union(ModifierMask::ALT),
                VirtualKey(b'9' as u16)
            ),
            Some(HotkeyAction::SwitchDesktop(8))
        );
        assert_eq!(
            table.lookup(ModifierMask::CTRL, VirtualKey(b'1' as u16)),
            Some(HotkeyAction::MoveForegroundToDesktop(0))
        );
        assert_eq!(
            table.lookup(
                ModifierMask::CTRL.union(ModifierMask::ALT),
                VirtualKey(b'9' as u16)
            ),
            Some(HotkeyAction::MoveForegroundToDesktopSilent(8))
        );
        assert_eq!(
            table.lookup(
                ModifierMask::CTRL.union(ModifierMask::ALT),
                VirtualKey(0x7B)
            ),
            Some(HotkeyAction::SwitchPreviousDesktop)
        );
        assert_eq!(
            table.lookup(
                ModifierMask::CTRL.union(ModifierMask::ALT),
                VirtualKey(0x7C)
            ),
            Some(HotkeyAction::AssignScratchpad)
        );
        assert_eq!(
            table.lookup(
                ModifierMask::CTRL.union(ModifierMask::ALT),
                VirtualKey(0x7D)
            ),
            Some(HotkeyAction::ToggleScratchpad)
        );
    }

    #[test]
    fn assigned_phase_one_bindings_resolve_without_changing_defaults() {
        let mut config = crate::config::Config::default();
        config.hotkeys.cycle_input_device = Some(Hotkey::parse("Ctrl+Alt+F13").unwrap());
        config.hotkeys.cycle_output_device = Some(Hotkey::parse("Ctrl+Alt+F14").unwrap());
        config.hotkeys.foreground_volume_up = Some(Hotkey::parse("Ctrl+Alt+F15").unwrap());
        config.hotkeys.foreground_volume_down = Some(Hotkey::parse("Ctrl+Alt+F16").unwrap());
        let table = build_bindings(&config);
        assert_eq!(
            table.lookup(
                ModifierMask::CTRL.union(ModifierMask::ALT),
                VirtualKey(0x7C)
            ),
            Some(HotkeyAction::CycleInputDevice)
        );
        assert_eq!(
            table.lookup(
                ModifierMask::CTRL.union(ModifierMask::ALT),
                VirtualKey(0x7F)
            ),
            Some(HotkeyAction::ForegroundVolumeDown)
        );
        assert_eq!(build_bindings(&crate::config::Config::default()).len(), 12);
    }

    fn test_hook_state() -> HookState {
        HookState {
            main_hwnd_raw: 0,
            engine: KeyboardEngine::new(),
            suspended_bindings: Arc::new(BindingTable::default()),
            config: Arc::new(ConfigHandle::new(crate::config::Config::default())),
            suspended: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn capture_delivers_chord_exactly_once() {
        let _guard = capture_guard();
        // #47: a live callback commits its own session exactly once.
        let mut st = test_hook_state();
        begin_capture();
        let token = capture_token().expect("armed");
        assert!(take_captured_chord().is_none(), "nothing published yet");

        // Modifier downs are swallowed and tracked, not delivered yet.
        assert_eq!(
            capture_event(&mut st, RawKeyEvent::down(0xA2), token),
            CaptureDisposition::Swallow
        );
        assert_eq!(
            capture_event(&mut st, RawKeyEvent::down(0xA4), token),
            CaptureDisposition::Swallow
        );
        assert!(
            take_captured_chord().is_none(),
            "modifiers alone must not complete"
        );

        let done = capture_event(&mut st, RawKeyEvent::down(b'M' as u16), token);
        assert_eq!(
            done,
            CaptureDisposition::Swallow,
            "chord key-down is swallowed"
        );
        assert!(capture_token().is_none(), "session completed -> disarmed");

        let chord = take_captured_chord().expect("exactly-once delivery");
        assert_eq!(chord.modifiers, ModifierMask::CTRL.union(ModifierMask::ALT));
        assert_eq!(chord.key, Some(VirtualKey(b'M' as u16)));
        assert!(take_captured_chord().is_none(), "consumed");
    }

    #[test]
    fn aba_stale_callback_cannot_complete_newer_session() {
        let _guard = capture_guard();
        // #47 core regression: callback pauses holding generation N; UI
        // cancels N and arms N+1; stale commit MUST be rejected and session
        // N+1 stays armed with no visible result from N.
        let mut st = test_hook_state();
        begin_capture();
        let stale = capture_token().expect("gen N");

        end_capture(); // recorder cancelled
        begin_capture(); // immediately re-armed

        // Old callback resumes with its stale token:
        let _swallowed = capture_event(&mut st, RawKeyEvent::down(b'M' as u16), stale);

        assert!(
            take_captured_chord().is_none(),
            "no result from N may surface"
        );
        let fresh = capture_token().expect("N+1 still armed");
        assert_ne!(fresh, stale, "generation advanced");

        // Session N+1 completes normally afterwards:
        assert_eq!(
            capture_event(&mut st, RawKeyEvent::down(b'K' as u16), fresh),
            CaptureDisposition::Swallow
        );
        let chord = take_captured_chord().expect("live session delivers");
        assert_eq!(chord.key, Some(VirtualKey(b'K' as u16)));
    }

    #[test]
    fn end_invalidates_in_flight_callback_publication() {
        let _guard = capture_guard();
        // Simpler variant: end between the ACTIVE observation and commit.
        let mut st = test_hook_state();
        begin_capture();
        let token = capture_token().unwrap();
        end_capture();

        let _swallowed = capture_event(&mut st, RawKeyEvent::down(b'M' as u16), token);
        assert!(
            take_captured_chord().is_none(),
            "cancelled session publishes nothing"
        );
        assert!(capture_token().is_none());
    }

    #[test]
    fn esc_cancel_publishes_cancellation_once() {
        let _guard = capture_guard();
        let mut st = test_hook_state();
        begin_capture();
        let token = capture_token().unwrap();
        assert_eq!(
            capture_event(&mut st, RawKeyEvent::down(0x1B), token),
            CaptureDisposition::Swallow
        );
        let chord = take_captured_chord().expect("cancel marker");
        assert_eq!(chord.key, None);
        assert!(take_captured_chord().is_none());
    }

    #[test]
    fn repeated_sessions_are_independent() {
        let _guard = capture_guard();
        let mut st = test_hook_state();
        for vk in [b'M' as u16, b'O' as u16] {
            begin_capture();
            let token = capture_token().unwrap();
            capture_event(&mut st, RawKeyEvent::down(0xA2), token);
            capture_event(&mut st, RawKeyEvent::down(0xA4), token);
            capture_event(&mut st, RawKeyEvent::down(vk), token);
            let chord = take_captured_chord().expect("one chord per session");
            assert_eq!(chord.key, Some(VirtualKey(vk)));
        }
    }

    #[test]
    fn several_generation_old_callback_is_rejected() {
        let _guard = capture_guard();
        let mut st = test_hook_state();
        begin_capture();
        let ancient = capture_token().unwrap();
        for _ in 0..4 {
            end_capture();
            begin_capture();
        }
        let _swallowed = capture_event(&mut st, RawKeyEvent::down(b'M' as u16), ancient);
        assert!(take_captured_chord().is_none());
        let fresh = capture_token().expect("current still armed");
        assert_ne!(fresh, ancient);
    }

    #[test]
    fn generation_wraparound_remains_safe() {
        let _guard = capture_guard();
        // Bounded u32 generation: force the counter to u32::MAX and verify a
        // begin wraps to 0 while a stale MAX-generation callback is rejected.
        CAPTURE_STATE.store(((u32::MAX as u64) << 32) | CAPTURE_ARMED, Ordering::Release);
        let ancient = capture_token().unwrap();
        assert_eq!(ancient, u32::MAX);
        begin_capture(); // wraps to generation 0
        let fresh = capture_token().unwrap();
        assert_eq!(fresh, 0);
        let mut st = test_hook_state();
        let _swallowed = capture_event(&mut st, RawKeyEvent::down(b'M' as u16), ancient);
        assert!(
            take_captured_chord().is_none(),
            "pre-wrap callback rejected"
        );
        assert_eq!(
            capture_event(&mut st, RawKeyEvent::down(b'M' as u16), fresh),
            CaptureDisposition::Swallow
        );
        let chord = take_captured_chord().expect("post-wrap session works");
        assert_eq!(chord.key, Some(VirtualKey(b'M' as u16)));
    }

    #[test]
    fn stale_non_modifier_event_is_swallowed_not_passed() {
        let _guard = capture_guard();
        // #48 core: arm N -> token N -> cancel N -> arm N+1 -> stale event.
        // Completion must be rejected AND the hook disposition must be
        // SWALLOW — the key may not leak into the foreground application.
        let mut st = test_hook_state();
        begin_capture();
        let stale = capture_token().expect("gen N");
        end_capture(); // cancel N
        begin_capture(); // arm N+1

        let disp = capture_event(&mut st, RawKeyEvent::down(b'M' as u16), stale);
        assert_eq!(
            disp,
            CaptureDisposition::Swallow,
            "stale key must be swallowed"
        );

        // No observable result from N; N+1 remains armed and uncorrupted.
        assert!(take_captured_chord().is_none());
        let fresh = capture_token().expect("N+1 still armed");
        assert_ne!(fresh, stale);

        // A live N+1 event still completes exactly once.
        assert_eq!(
            capture_event(&mut st, RawKeyEvent::down(b'K' as u16), fresh),
            CaptureDisposition::Swallow
        );
        let chord = take_captured_chord().expect("N+1 delivers its own chord");
        assert_eq!(chord.key, Some(VirtualKey(b'K' as u16)));
    }

    #[test]
    fn stale_esc_event_is_swallowed_and_cancels_nothing() {
        let _guard = capture_guard();
        let mut st = test_hook_state();
        begin_capture();
        let stale = capture_token().unwrap();
        end_capture();
        begin_capture();

        let disp = capture_event(&mut st, RawKeyEvent::down(0x1B), stale);
        assert_eq!(disp, CaptureDisposition::Swallow);
        // Stale Esc must NOT cancel session N+1:
        assert!(capture_token().is_some(), "N+1 still armed after stale Esc");
        assert!(take_captured_chord().is_none());

        // Live Esc in N+1 still cancels properly:
        let fresh = capture_token().unwrap();
        assert_eq!(
            capture_event(&mut st, RawKeyEvent::down(0x1B), fresh),
            CaptureDisposition::Swallow
        );
        assert!(take_captured_chord().unwrap().key.is_none());
    }

    #[test]
    fn stale_event_after_plain_end_capture_is_swallowed_without_rearm() {
        let _guard = capture_guard();
        // end_capture alone (no immediate re-arm): a paused callback must not
        // resurrect a completed session or publish anything.
        let mut st = test_hook_state();
        begin_capture();
        let stale = capture_token().unwrap();
        end_capture();

        let disp = capture_event(&mut st, RawKeyEvent::down(b'X' as u16), stale);
        assert_eq!(disp, CaptureDisposition::Swallow);
        assert!(take_captured_chord().is_none());
        assert!(capture_token().is_none(), "still disarmed");
    }

    #[test]
    fn unsupported_vk_still_passes_during_capture() {
        let _guard = capture_guard();
        // Pre-existing intentional policy preserved (#48): events outside the
        // supported VK domain are forwarded even in capture mode.
        let mut st = test_hook_state();
        begin_capture();
        let token = capture_token().unwrap();
        // 0x100+ is outside the supported VK domain (normalize leaves it
        // untouched), which is the intended pass-through class.
        assert_eq!(
            capture_event(
                &mut st,
                RawKeyEvent {
                    vk: 0x100,
                    extended: false,
                    down: true,
                    injected: false
                },
                token
            ),
            CaptureDisposition::Pass
        );
        end_capture();
    }

    #[test]
    fn modifier_state_sane_after_stale_token_scenario() {
        let _guard = capture_guard();
        let mut st = test_hook_state();
        begin_capture();
        let stale = capture_token().unwrap();
        end_capture();
        begin_capture();

        // Physical modifiers pressed during the stale window are tracked.
        capture_event(&mut st, RawKeyEvent::down(0xA2), stale); // LCtrl, swallowed
        assert_eq!(st.engine.current_modifiers(), ModifierMask::CTRL);

        // Release clears them (LL callbacks serialized; no cross-session bleed).
        capture_event(&mut st, RawKeyEvent::up(0xA2), stale);
        assert_eq!(st.engine.current_modifiers(), ModifierMask::NONE);
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

    #[test]
    fn hook_guard_plumbing_round_trip() {
        // #42: exercises install -> guard-drop -> reinstall plumbing.
        //
        // LIMITATION: a successful second SetWindowsHookExW does NOT prove
        // the first hook was removed — Windows allows multiple hooks of the
        // same type in a chain, so user-mode tests cannot observe the global
        // hook count. Actual unhook-on-drop is verified by the Drop impl
        // (single UnhookWindowsHookEx call) plus manual/live verification:
        // after KeyboardService::shutdown, keystrokes must no longer route
        // through low_level_keyboard_proc (log the callback or check with a
        // system hook enumerator).
        let first = install_hook().expect("first install");
        drop(HookGuard(first));
        let second = install_hook().expect("reinstall after guard drop");
        drop(HookGuard(second));
    }

    #[test]
    fn state_publication_is_null_before_teardown_test_double() {
        // #42 ordering contract, exercised on plain memory: clear must happen
        // BEFORE the state allocation is freed. Mirrors keyboard_thread's
        // teardown sequence without a live message loop.
        let state = Box::new(HookState {
            main_hwnd_raw: 0,
            engine: KeyboardEngine::new(),
            suspended_bindings: Arc::new(BindingTable::default()),
            config: Arc::new(ConfigHandle::new(crate::config::Config::default())),
            suspended: Arc::new(AtomicBool::new(false)),
        });
        let ptr = Box::into_raw(state);
        HOOK_STATE.store(ptr, Ordering::Release);

        // Teardown order under test: uninstall (no-op here) -> clear -> free.
        HOOK_STATE.store(std::ptr::null_mut(), Ordering::Release);
        assert!(HOOK_STATE.load(Ordering::Acquire).is_null());
        // SAFETY: ownership was taken by into_raw above and publication was
        // just undone.
        unsafe { drop(Box::from_raw(ptr)) };
    }
}
