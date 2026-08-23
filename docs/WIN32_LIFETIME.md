# Win32 Lifetime & Ownership Rules

Every OS handle/interface has exactly one owner thread or struct. This document is the audit
checklist for callback-after-free and cross-apartment bugs.

## Handles

| Object | Created by | Owner | Released |
|---|---|---|---|
| Instance mutex / activation event | `single_instance.rs` | process lifetime | never (OS reclaims) |
| `HHOOK` (WH_KEYBOARD_LL) | keyboard thread | keyboard thread only | `UnhookWindowsHookEx` on shutdown, before audio teardown |
| Hidden main window / settings / overlay HWNDs | main thread | main thread | `DestroyWindow` after workers stopped posting |
| Tray icon | main thread | main thread | `Shell_NotifyIconW(NIM_DELETE)` before window destroy |
| D2D factory / device context, DWrite factory, DXGI factory/swapchain, DComp device/target/visual | main thread | renderer per-window | Drop order: DComp target → visual → swapchain → context; windows destroyed last |
| GDI icons (tray) | main thread | tray module | `DestroyIcon` at shutdown/replacement |
| WinEvent hook (foreground) | main thread | main thread | `UnhookWinEvent` on shutdown |

## COM interfaces

* **Audio thread (MTA)** owns every Core Audio pointer: `IMMDeviceEnumerator`,
  `IMMDevice`, `IAudioEndpointVolume`, `IAudioSessionManager2`, `IAudioSessionEnumerator`,
  `ISimpleAudioVolume`, plus registered `IMMNotificationClient` /
  `IAudioEndpointVolumeCallback` implementations.
* Callback COM objects are Rust structs with reference counts; they hold only a raw event-post
  window handle + thread id, never a reference into worker state. They are unregistered *before*
  endpoint interfaces drop (`UnregisterAudioEndpointNotificationCallback`,
  `UnregisterV2`), and the audio thread joins before main releases anything they could touch.
* Endpoint objects are recreated on every default-device-change / device-invalidation event;
  old callbacks are unregistered first. Device loss (`AUDCLNT_E_DEVICE_INVALIDATED`,
  `HRESULT_FROM_WIN32(ERROR_DEVICE_REMOVED)`) triggers rebuild, not panic.
* **Desktop thread (STA)** owns ImmersiveShell pointers for the process lifetime of the backend.
  Calls are serialized through its command channel. Pointers are released when the backend is
  dropped on that same thread.
* Main thread initializes nothing COM-dependent except via `CoCreateInstance` where documented
  (none currently). If that changes, apartment must be revisited.

## Message lifetime

Events posted to the main window as boxed payloads: sender `Box::into_raw`, main thread
reconstructs with `Box::from_raw` in exactly one place (`app.rs::handle_app_message`). A message
that fails reconstruction is a bug, not handled gracefully — no double-free path exists because
only the main thread ever frees.

## Shutdown ordering (avoids use-after-free)

```
1. suspended = true            // engine stops dispatching new actions
2. PostThreadMessage(QUIT) to keyboard thread -> UnhookWindowsHookEx inside its own thread
3. join keyboard thread        // guarantees no hook callback runs afterwards
4. audio thread: unregister callbacks, release interfaces, channel QUIT, join
5. desktop thread: release COM, QUIT, join
6. instance watcher thread stop
7. Shell_NotifyIconW(NIM_DELETE), DestroyIcon(s)
8. DestroyWindow(settings) DestroyWindow(overlay) DestroyWindow(main)
9. render resources dropped by struct field order
```

## Failure handling

Win32/HRESULT failures return `Result<_, WinError>` carrying the API name and code
(`error.rs`). `unwrap()` appears only in tests and in the one message-box fatal path during
pre-window initialization failures. UI surfaces human-readable states ("Microphone unavailable");
diagnostics log keeps the HRESULT name.
