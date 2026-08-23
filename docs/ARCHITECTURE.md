# WinShort Architecture

Native Windows tray utility: audio hotkeys, status overlay, virtual desktop switching.
Pure Rust against Win32/COM via the Microsoft `windows` crate. No GUI framework, no WebView,
no other-language components.

## Subsystems

```
App Runtime (main thread)
├── message loop + hidden main window (event pump)
├── Tray (Shell_NotifyIconW, NOTIFYICON_VERSION_4)
├── Settings window (Direct2D + DirectWrite + DXGI flip swapchain + DWM attrs)
├── Overlay window (WS_EX_NOACTIVATE/TRANSPARENT, premultiplied DComp visual)
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
| Main/UI | none required | all HWNDs, D2D/DWrite/DXGI objects, tray, WinEvent hook (foreground tracking), timers |
| Keyboard | none | `SetWindowsHookExW(WH_KEYBOARD_LL)` handle, key state, binding table lookup |
| Audio | MTA | all Core Audio interfaces and callbacks |
| Desktop | STA (`CoInitializeEx STA`) | undocumented Shell COM pointers |
| Instance watcher | none | waits on named activation event |

Rules:

* The keyboard hook callback does O(1) table lookups on prebuilt state and posts one message.
  No COM, no allocation beyond a small stack struct, no file/registry access, no locks held by slow producers.
* Core Audio callbacks fire on the audio thread; they post typed events to the main window and never touch UI.
* Raw COM interfaces never cross threads. Cross-thread communication is `AppEvent` messages and
  `std::sync::mpsc` command channels.

## Events

Strongly typed (`src/event.rs`). Keyboard thread -> main: `WM_APP_ACTION` with a packed tag.
Audio/desktop workers -> main: `PostMessageW` of boxed `AppEvent` payloads (leak/send/reconstruct;
main thread reclaims the box). Main -> workers: mpsc channels. No string events anywhere.

```rust
enum AppEvent { ToggleMicrophone, ToggleOutput, ToggleForegroundAppAudio, SwitchDesktop(u8),
  MicrophoneStateChanged(AudioState), OutputStateChanged(OutputState),
  ForegroundAudioChanged(AppAudioState), ConfigApplied(u64), ShowSettings, Exit, ... }
```

## State separation

| Layer | Type | Mutability |
|---|---|---|
| Desired config | `Arc<ConfigSnapshot>` behind `RwLock` | replaced atomically on Save |
| Runtime state | suspended flag, overlay model, foreground pid slot | event-driven updates |
| Observed Windows state | audio endpoint states, desktop backend status | owned by worker threads, published as events |
| UI draft | `Config` clone inside settings window | user edits only |

The live hotkey bindings used by the keyboard engine are read from an `Arc` snapshot cloned at
recognition time. Saving swaps the `Arc`; the hook is never reinstalled for config changes.

## Startup sequence

single-instance check -> DPI awareness -> logging -> load/validate config -> COM init ->
main hidden window -> render resources -> overlay HWND -> tray icon -> audio subsystem ->
keyboard hook -> desktop backend detection -> message loop.

## Shutdown sequence

disable dispatch -> unhook keyboard -> unregister audio callbacks -> release COM -> remove tray icon ->
destroy windows/render resources -> exit. Ordered explicitly in `app.rs::shutdown`.
