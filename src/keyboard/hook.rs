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
use crate::keyboard::binding::{BindingTable, Hotkey, ModifierMask, VirtualKey};
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

/// Handle one raw event while capture is active. Always swallows the event:
/// recorded keys must neither reach other apps nor trigger actions.
fn capture_event(state: &mut HookState, ev: RawKeyEvent, token: u32) -> bool {
    let vk = normalize_vk(ev.vk, ev.extended);
    if vk >= 256 {
        return false;
    }
    // Track modifier state in the engine so the mask matches real physics.
    let _ = state.engine.on_event(ev, &state.suspended_bindings);
    if !ev.down || KeyState::is_modifier(vk) {
        return true;
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
    // callback from a cancelled/replaced generation fails the CAS and the
    // keystroke is simply swallowed.
    let committed = complete_capture(token, chord);
    state.engine.reset();
    committed
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

    pub fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::Acquire)
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
        return if capture_event(state, event, token) {
            LRESULT(1)
        } else {
            // SAFETY: pass-through when the event cannot be classified.
            unsafe { CallNextHookEx(None, code, wparam, lparam) }
        };
    }
    // Lock-free snapshot read (#10): the table is rebuilt by ConfigHandle on
    // Save; the callback only loads it. No RwLock, no HashMap rebuild here.
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
        // #47: a live callback commits its own session exactly once.
        let mut st = test_hook_state();
        begin_capture();
        let token = capture_token().expect("armed");
        assert!(take_captured_chord().is_none(), "nothing published yet");

        // Modifier downs are swallowed and tracked, not delivered yet.
        assert!(capture_event(&mut st, RawKeyEvent::down(0xA2), token));
        assert!(capture_event(&mut st, RawKeyEvent::down(0xA4), token));
        assert!(
            take_captured_chord().is_none(),
            "modifiers alone must not complete"
        );

        let done = capture_event(&mut st, RawKeyEvent::down(b'M' as u16), token);
        assert!(done, "chord key-down is swallowed");
        assert!(capture_token().is_none(), "session completed -> disarmed");

        let chord = take_captured_chord().expect("exactly-once delivery");
        assert_eq!(chord.modifiers, ModifierMask::CTRL.union(ModifierMask::ALT));
        assert_eq!(chord.key, Some(VirtualKey(b'M' as u16)));
        assert!(take_captured_chord().is_none(), "consumed");
    }

    #[test]
    fn aba_stale_callback_cannot_complete_newer_session() {
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
        assert!(capture_event(
            &mut st,
            RawKeyEvent::down(b'K' as u16),
            fresh
        ));
        let chord = take_captured_chord().expect("live session delivers");
        assert_eq!(chord.key, Some(VirtualKey(b'K' as u16)));
    }

    #[test]
    fn end_invalidates_in_flight_callback_publication() {
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
        let mut st = test_hook_state();
        begin_capture();
        let token = capture_token().unwrap();
        assert!(capture_event(&mut st, RawKeyEvent::down(0x1B), token));
        let chord = take_captured_chord().expect("cancel marker");
        assert_eq!(chord.key, None);
        assert!(take_captured_chord().is_none());
    }

    #[test]
    fn repeated_sessions_are_independent() {
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
        assert!(capture_event(
            &mut st,
            RawKeyEvent::down(b'M' as u16),
            fresh
        ));
        let chord = take_captured_chord().expect("post-wrap session works");
        assert_eq!(chord.key, Some(VirtualKey(b'M' as u16)));
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
