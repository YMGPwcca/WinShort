# WinShort Architecture

**Status: Implemented** (this document describes current `main`; forward-looking ideas live in the issue tracker, not here).

Native Windows tray utility: audio hotkeys, status overlay, virtual desktop switching.
Pure Rust against Win32/COM via the Microsoft `windows` crate. No GUI framework, no WebView,
no other-language components.

## Subsystems

```
App Runtime (main thread, STA)
├── message loop + hidden main window (event pump)
├── Tray (Shell_NotifyIconW, NOTIFYICON_VERSION_4)
├── Settings window (Direct2D HwndRenderTarget + DirectWrite + DWM chrome)
├── Overlay window (WS_EX_NOACTIVATE/TRANSPARENT/LAYERED, WIC→DIB→UpdateLayeredWindow)
└── Action router: AppEvent -> worker threads

Keyboard thread          Audio thread (MTA)         Desktop thread (STA)
├── WH_KEYBOARD_LL       ├── IMMDeviceEnumerator    ├── ImmersiveShell IServiceProvider
├── modifier tracking    ├── IAudioEndpointVolume   ├── IVirtualDesktopManagerInternal
├── binding recognition  ├── IMMNotificationClient  └── SwitchDesktop / enumeration
└── PostMessage only     └── command channel loop
```

## Threading model

| Thread | COM apartment | Owns |
|---|---|---|
| Main/UI | STA (`ComApartment::init_sta`, `src/platform/com.rs`) | all HWNDs, settings renderer objects, tray, WinEvent hook (foreground tracking), timers |
| Keyboard | none | `SetWindowsHookExW(WH_KEYBOARD_LL)` handle, engine key state, capture state machine |
| Audio | MTA | all Core Audio interfaces and callbacks (`winshort-audio` worker) |
| Desktop | STA | undocumented Shell COM pointers (`winshort-desktop` worker) |
| Instance watcher | none | waits on named activate/shutdown events |

Rules:

* The keyboard hook callback does O(1) table lookups against a lock-free `ArcSwap<BindingTable>`
  snapshot and posts one window message. It takes no Mutex/RwLock, no COM, no file/registry
  access. The only heap allocation on the callback path is one small bounded `Vec` in the
  digit-first chord completion arm (see KEYBOARD_HOOK_DESIGN.md).
* Core Audio callbacks fire on the audio thread; they post typed events to the main window and
  never touch UI. Notification bursts are coalesced into a single rebuild (#19).
* Raw COM interfaces never cross threads. Cross-thread communication is `AppEvent` messages and
  `std::sync::mpsc` command channels.
* The desktop controller runs one bounded recovery attempt per user command; policy errors are
  classified by `DesktopError::permits_fallback` (VIRTUAL_DESKTOP_COMPAT.md).

## Events

Strongly typed (`src/event.rs`). Two transports:

* **Keyboard → main:** `PostMessageW(hwnd, WM_APP_ACTION, packed_action, dirty_flag)` — the
  action packs into a 32-bit WPARAM (`kind << 16 | arg`); the LPARAM carries the
  `dirty_win_chord` flag that triggers the Start-menu countermeasure injection.
* **Workers ↔ main:** a process-wide `EventQueue` (`Mutex<VecDeque<AppEvent>>`) drained by the
  main loop; producers push events then post a wake-only `WM_APP_EVENT`. Main → workers:
  mpsc command channels (`AudioCommand`, `DesktopCommand`). No string events anywhere.

```rust
enum AppEvent { ToggleMicrophone, ToggleOutput, ToggleForegroundAppAudio, SwitchDesktop(u8),
  MicrophoneStateChanged(AudioState), OutputStateChanged(OutputState),
  ForegroundAudioChanged(AppAudioState), DesktopBackendChanged(BackendStatus),
  ConfigApplied(u64), ShowSettings, Exit, ... }
```

## State separation

| Layer | Type | Mutability |
|---|---|---|
| Desired config | `ConfigHandle`: `RwLock<Arc<Config>>` value + `revision: AtomicU64` | replaced atomically on Save |
| Hotkey bindings | `arc_swap::ArcSwap<BindingTable>` inside `ConfigHandle` | swapped on Save; wait-free reads in the hook |
| Runtime state | suspended flag, overlay model, foreground pid slot, desktop status | event-driven updates |
| Observed Windows state | audio endpoint states, desktop backend status | owned by worker threads, published as events |
| UI draft | `Config` clone inside settings window | user edits only |

Saving rebuilds the `BindingTable` and swaps the `ArcSwap` pointer; the hook is never
reinstalled for config changes. Suspension selects a shared empty table.

## Diagnostics & support

**Status: Implemented.** `App::diagnostics_snapshot` copies cached subsystem state into an
immutable `DiagnosticsSnapshot` on the main thread. The Diagnostics window renders that copy
with the existing Direct2D renderer; `WM_PAINT` performs no Core Audio enumeration, Shell COM
call, registry query, filesystem scan, or process inspection.

Copy Diagnostics formats a sanitizer projection directly to `CF_UNICODETEXT`. Open Logs uses
`ShellExecuteW`. Support bundle creation owns a one-shot `winshort-support-bundle` worker;
the worker receives only the copied snapshot, reads a bounded set of recent logs, writes a
local ZIP, posts completion through `EventQueue`, and is joined during shutdown. It owns no
HWND/App references and performs no network operation.

## Startup sequence

single-instance check (named mutex) -> DPI awareness (PerMonitorV2) -> logging init ->
load/validate config -> COM init (STA on UI thread) -> hidden main window -> tray icon ->
overlay surface -> foreground tracker -> audio subsystem -> keyboard hook thread -> desktop
backend detection -> second-instance watcher -> message loop. Each subsystem install degrades
independently on failure (logged, surfaced in Settings → Advanced).

## Shutdown sequence

`App::begin_shutdown` (src/app.rs), in order:

1. `shutting_down = true` — dispatch gate for queued/stale events
2. signal the second-instance watcher shutdown event before any window goes away (#24)
3. keyboard service: suspend + shutdown (unhook happens on the keyboard thread)
4. audio service shutdown (unregister callbacks, join)
5. desktop service shutdown (release COM, join)
6. foreground tracker dropped
7. overlay: hide + `DestroyWindow`
8. tray: `Shell_NotifyIconW(NIM_DELETE)`
9. settings window destroyed
10. `PostMessageW(main_hwnd, WM_CLOSE)` — main window destruction happens outside any `App`
    borrow (reentrancy-safe; see WIN32_LIFETIME.md)

## Verification infrastructure

GitHub Actions (`.github/workflows/ci.yml`) independently verifies every push/PR on Windows
hosted runners: fmt, clippy `-D warnings`, full test suite, x86_64 release build with manifest
byte-check, i686/aarch64 compile checks, MSRV job (Rust 1.85), cargo-deny dependency/security
gate. Tag-driven `release.yml` packages signed-ready x86_64/i686 ZIPs with SHA256SUMS.
