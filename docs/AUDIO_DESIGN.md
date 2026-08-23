# Audio Design

All Core Audio work lives on one MTA thread (`audio/`). The main thread never touches COM audio
interfaces.

## Endpoints

```
IMMDeviceEnumerator (CoCreateInstance MMDeviceEnumerator)
  → GetDefaultAudioEndpoint(eCapture, role) → IMMDevice → Activate(IAudioEndpointVolume)
  → GetDefaultAudioEndpoint(eRender, role)  → IMMDevice → Activate(IAudioEndpointVolume)
```

* Capture = microphone path: `GetMute` / `SetMute` (+ volume for overlay display).
* Render = output path: mute + volume scalar + device identity (endpoint ID + friendly name
  from the device property store, `PKEY_Device_FriendlyName`).
* Endpoint role is configurable per side (`eConsole` default; `eMultimedia`, `eCommunications`
  selectable). Default capture role is `eCommunications`? No — default `eConsole`: that is what
  the Sounds control panel and most apps manipulate; communications devices differ only when
  a conferencing client set them. Documented in CONFIG_SCHEMA.md.
* Endpoints are rebuilt on default-change and device-removal events. Rebuild =
  unregister old callbacks → drop old interfaces → create new → register callbacks → publish state.

## Notifications (no polling)

* `IMMNotificationClient` (registered on `IMMDeviceEnumerator`) —
  `OnDefaultDeviceChanged`, `OnDeviceAdded/Removed/StateChanged`. Device-state events refresh the
  device lists used by the settings pickers and rebuild endpoints if the current one died.
* `IAudioEndpointVolumeCallback` on each endpoint — volume/mute changes made anywhere
  (volume keys, tray slider, other apps) arrive here; posted as state events so the overlay and
  settings UI stay truthful without polling.

Both callbacks run on the audio thread, do nothing but post a compact event to the main window,
and hold no references to worker internals (only the raw post target). Registered objects are
kept alive until unregistered at endpoint rebuild/shutdown.

## Sessions (foreground app)

Executed synchronously inside the desktop/audio worker when the foreground-audio action fires:

```
GetForegroundWindow → GetWindowThreadProcessId → pid
IMMDevice(render) → Activate(IAudioSessionManager2) → GetSessionEnumerator
for each IAudioSessionControl:
    QI IAudioSessionControl2 → GetProcessId
    match pid → QI ISimpleAudioVolume → read GetMute
aggregate: all muted → Muted | none muted → Active | mixed → Mixed (toggle mutes all)
apply SetMute per matched session
```

Edge cases:

| Case | Behavior |
|---|---|
| no foreground window | "No external application selected" overlay |
| foreground is this utility | use last tracked external pid (WinEvent hook); else same message |
| zero matching sessions | "No audio session" |
| process exits mid-enumeration | skip session; enumeration errors tolerated |
| system/protected process | sessions absent or `GetProcessId` fails → treated as no-session |
| exclusive-mode owner | endpoint volume still toggles via its own path; session list may be empty |

Foreground tracking: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` on the main thread records
`(pid)` of every non-self foreground window into an atomic slot; defensive fresh query happens at
hotkey time regardless.

Session mute state changes by others are not watched continuously (no requirement to display live
per-app state); the overlay shows post-toggle aggregate truthfully because it re-reads after apply.

## Threading recap

Commands in: `mpsc::Sender<AudioCommand>` from main thread. Events out: boxed `AppEvent`
via `PostMessageW`. No interface leaves the thread. Callback-after-free prevented by unregistering
before drops and joining the thread before main teardown (see WIN32_LIFETIME.md).
