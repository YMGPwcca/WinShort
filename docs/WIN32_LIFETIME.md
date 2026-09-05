# Win32 Lifetime & Ownership Rules

**Status: Implemented** — the rules below describe the application and UI ownership boundaries and form the audit checklist
for callback-after-free and cross-apartment bugs.

Every OS handle/interface has exactly one owner thread or struct.

## App singleton confinement

`App` lives in a `thread_local! { RefCell<Option<App>> }` on the main thread, accessed only via
`with_app(...)` (src/app.rs). Consequences:

* No `App` field is ever touched from another thread; workers communicate only through events
  and command channels.
* Reentrancy hazard: `App` methods must never destroy the main window while a `with_app`
  borrow is alive. Shutdown therefore *posts* `WM_CLOSE` instead of calling `DestroyWindow`
  directly (`begin_shutdown`, src/app.rs).
* Native operations can synchronously reenter WndProc on the same thread. Prepare owned
  plans or snapshots under a short borrow, release it, and only then perform reentrant
  window operations. Thread affinity does not itself make nested mutable borrowing safe.

## Handles

| Object | Created by | Owner | Released |
|---|---|---|---|
| Instance mutex / activate + shutdown events | `platform/single_instance.rs` | process lifetime | OS reclaims; watcher signalled before window teardown (#24) |
| `HHOOK` (WH_KEYBOARD_LL) | keyboard thread | keyboard thread only | `HookGuard` drop → `UnhookWindowsHookEx`, then `HOOK_STATE` nulled — all on the hook's own pumping thread (#42) |
| Hidden main / Control Center / overlay HWNDs | main thread | main thread | overlay+Control Center destroyed in `begin_shutdown`; main via posted `WM_CLOSE` after borrows end |
| Control Center renderer (D2D factory, `ID2D1HwndRenderTarget`, brushes, text formats) | main thread | `ui::renderer::Renderer` | dropped with the Control Center UI; any `EndDraw` failure drops the whole Renderer so the next paint rebuilds it from scratch |
| Overlay Composition/D3D/D2D resources | main thread | `ui::overlay` rendering backend | persistent visual tree; drawing surface recreated on size/DPI change; resources released with backend |
| Diagnostics HWND | main thread | `DiagnosticsWindow` / main thread | hidden on close; destroyed during `App::begin_shutdown` |
| Special Desktop GUID + return GUID | desktop thread | `DesktopController::{special_workspace,special_return}` plus `%LOCALAPPDATA%\WinShort\special-workspace.guid` for desktop identity only | `special_return` is process-only; external deletion clears stale identity; disable/orderly shutdown removes the dedicated desktop and clears persisted identity; a hard kill/reboot can leave the desktop alive, and the next process reclaims it only by the exact persisted GUID |
| Support worker | one-shot filesystem thread | `App::support_bundle: Option<JoinHandle<()>>` | completion posts an event; App joins it before diagnostics HWND teardown |
| Clipboard HGLOBAL | main thread during Copy Diagnostics | Windows after successful `SetClipboardData(CF_UNICODETEXT, ...)` | WinShort frees it only when allocation/clipboard transfer fails |
| Control Center picker HWND + LISTBOX | main thread | `PickerPopup` | focus loss/Escape/commit drops popup; Control Center shutdown drops it before process exit |
| Control Center UI Automation provider | main/UIA COM callers | `SettingsAutomation` shared snapshot + queued action mutex | provider retains `Arc<AutomationState>` with snapshot and deferred queues protected by short-lived mutex guards; reads never borrow Control Center UI, actions post `WM_APP_SETTINGS_AUTOMATION` to the Control Center HWND |
| Display rollback token | main thread | `App::display_rollback` | pre-apply paths/modes and route snapshot stay in memory; Keep commits, Revert/timeout restores, failed restore remains retryable |
| WinEvent hook (foreground) | main thread | foreground tracker | unhooked when tracker drops in shutdown |

## COM interfaces

* **Main/UI thread: STA.** Initialized via `ComApartment::init_sta`
  (`CoInitializeEx(COINIT_APARTMENTTHREADED)`) before window creation.
* **Audio thread (MTA)** owns every Core Audio pointer: `IMMDeviceEnumerator`, `IMMDevice`,
  `IAudioEndpointVolume`, `IAudioSessionManager2`, `IAudioSessionEnumerator`,
  `ISimpleAudioVolume`, plus registered `IMMNotificationClient` /
  `IAudioEndpointVolumeCallback` implementations.
* Callback COM objects are Rust structs with reference counts; they hold only a raw event-post
  window handle + thread id, never a reference into worker state. Volume callbacks are
  unregistered in `EndpointBinding::drop`; the device-notification client is unregistered in
  audio `shutdown`. The audio thread joins before main teardown touches anything they could
  reach.
* Endpoint objects are rebuilt on default-device-change / invalidation; old callbacks are
  unregistered first. Device loss HRESULTs trigger rebuild-and-retry, not panic
  (AUDIO_DESIGN.md).
* **Desktop thread (STA)** owns ImmersiveShell pointers for the backend's lifetime. Calls are
  serialized through its command channel; pointers are released on that same thread at
  shutdown.

## Message lifetime

Events flow through the process-wide `EventQueue` — a `Mutex<VecDeque<AppEvent>>` drained by
the main loop after a wake-only `WM_APP_EVENT` post. Producers push and wake; the main thread
pops. There is no boxed-payload handoff across threads and no manual allocation ownership in
messages. Keyboard actions arrive as packed integers on `WM_APP_ACTION` (no payload lifetime).

## Shutdown ordering (avoids use-after-free)

Exact sequence of `App::begin_shutdown`:

```
1. shutting_down = true                  // stale/queued events become no-ops
2. single_instance::signal_shutdown()    // watcher stops BEFORE windows disappear (#24)
3. keyboard.set_suspended(true); shutdown()   // unhooks on the keyboard thread, joins
4. audio.shutdown()                      // unregister callbacks, release COM, join
5. desktop.shutdown()                    // remove Special Desktop if present, clear its persisted GUID, release Shell COM, join
6. foreground tracker dropped            // WinEvent hook gone
7. overlay.hide(); DestroyWindow(overlay)
8. tray.remove()                         // NIM_DELETE (+ DestroyIcon via OwnedIcon drop)
9. DestroyWindow(control_center)
10. PostMessageW(main_hwnd, WM_CLOSE)    // destruction outside any App borrow
```

## Failure handling

Win32/HRESULT failures return `Result<_, WinError>` carrying the API name and code
(`error.rs`). `unwrap()` appears only in tests and in the one message-box fatal path during
pre-window initialization failures. UI surfaces human-readable states ("Microphone unavailable");
diagnostics log keeps the HRESULT name.

## UI construction and paint transactions

See [UI_ARCHITECTURE.md](UI_ARCHITECTURE.md) for `WindowCreation<T>`,
`WindowConstructionGuard`, `PaintSession`, picker font ownership and tray
menu/bitmap guards. UI class registration is fallible; a failed constructor
rolls back its state and native resources instead of relying on a later shutdown.
