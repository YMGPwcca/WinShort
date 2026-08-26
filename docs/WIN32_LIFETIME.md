# Win32 Lifetime & Ownership Rules

**Status: Implemented** — every rule below reflects current `main` and is the audit checklist
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
* WndProc handlers run on the main thread only; they may borrow mutably because no other
  borrow can be live inside a single-window message dispatch.

## Handles

| Object | Created by | Owner | Released |
|---|---|---|---|
| Instance mutex / activate + shutdown events | `platform/single_instance.rs` | process lifetime | OS reclaims; watcher signalled before window teardown (#24) |
| `HHOOK` (WH_KEYBOARD_LL) | keyboard thread | keyboard thread only | `HookGuard` drop → `UnhookWindowsHookEx`, then `HOOK_STATE` nulled — all on the hook's own pumping thread (#42) |
| Hidden main / settings / overlay HWNDs | main thread | main thread | overlay+settings destroyed in `begin_shutdown`; main via posted `WM_CLOSE` after borrows end |
| Tray icon (NOTIFYICON_VERSION_4) | main thread | tray module | `NIM_DELETE` in `begin_shutdown`; icon HICON owned by `OwnedIcon` → `DestroyIcon` on Drop/replacement |
| Settings renderer (D2D factory, `ID2D1HwndRenderTarget`, brushes, text formats) | main thread | `ui::renderer::Renderer` | dropped with the settings UI; any `EndDraw` failure drops the whole Renderer so the next paint rebuilds it from scratch |
| Overlay surface (WIC bitmap, DIB section, compatible DC) | main thread | `OverlayGraphics`/`LayeredSurface` | re-created per `show()`; released with the overlay |
| Diagnostics HWND | main thread | `DiagnosticsWindow` / main thread | hidden on close; destroyed during `App::begin_shutdown` |
| Support worker | one-shot filesystem thread | `App::support_bundle: Option<JoinHandle<()>>` | completion posts an event; App joins it before diagnostics HWND teardown |
| Clipboard HGLOBAL | main thread during Copy Diagnostics | Windows after successful `SetClipboardData(CF_UNICODETEXT, ...)` | WinShort frees it only when allocation/clipboard transfer fails |
| Settings picker HWND + LISTBOX | main thread | `PickerPopup` | focus loss/Escape/commit drops popup; Settings shutdown drops it before process exit |
| Settings UI Automation provider | main/UIA COM callers | `SettingsAutomation` shared snapshot + queued action mutex | provider owns only `Arc<RwLock<SettingsAutomationSnapshot>>`; reads never borrow `SettingsUi`, actions post `WM_APP_SETTINGS_AUTOMATION` to the Settings HWND |
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
5. desktop.shutdown()                    // release Shell COM, join
6. foreground tracker dropped            // WinEvent hook gone
7. overlay.hide(); DestroyWindow(overlay)
8. tray.remove()                         // NIM_DELETE (+ DestroyIcon via OwnedIcon drop)
9. DestroyWindow(settings)
10. PostMessageW(main_hwnd, WM_CLOSE)    // destruction outside any App borrow
```

## Failure handling

Win32/HRESULT failures return `Result<_, WinError>` carrying the API name and code
(`error.rs`). `unwrap()` appears only in tests and in the one message-box fatal path during
pre-window initialization failures. UI surfaces human-readable states ("Microphone unavailable");
diagnostics log keeps the HRESULT name.
