# Audio Design

**Status: Implemented** (endpoints, notifications, foreground resolver ladder, aggregate
semantics). Known limitations listed at the end are accepted behavior, not gaps.

All Core Audio work lives on one MTA worker thread (`winshort-audio`, `audio/controller.rs`).
The main thread never touches COM audio interfaces; it talks via an mpsc `AudioCommand`
channel, and state returns as `AppEvent`s posted to the main window.

## Endpoints

```
IMMDeviceEnumerator (CoCreateInstance MMDeviceEnumerator)
  → GetDefaultAudioEndpoint(eCapture, role) → IMMDevice → Activate(IAudioEndpointVolume)
  → GetDefaultAudioEndpoint(eRender, role)  → IMMDevice → Activate(IAudioEndpointVolume)
```

* Capture = microphone path: `GetMute` / `SetMute` (+ volume for overlay display).
* Render = output path: mute + volume scalar + device identity (endpoint ID + friendly name
  from the property store, `PKEY_Device_FriendlyName`).
* Endpoint role is configurable per side (`input_role`/`output_role`: console | multimedia |
  communications; both default **console**).
* Endpoints are rebuilt on default-change and device-removal events. Rebuild =
  unregister old callbacks → drop old interfaces → create new → register callbacks → publish
  state. Device enumeration failures degrade per-flow with warnings surfaced in Settings (#36),
  never aborting the whole subsystem.

## Notifications (no polling)

* `IMMNotificationClient` on the enumerator — device state/add/remove/default changes send
  `RefreshAll` commands.
* `IAudioEndpointVolumeCallback` per endpoint — external volume/mute changes arrive as commands.

Both run on the audio thread, hold no references into worker state (raw post target only), and
are unregistered before their interfaces drop.

**Own-event filtering (#19/#36):** WinShort passes distinct event-context GUIDs to its own
`SetMute` calls (endpoint context and session context); callbacks carrying those contexts are
dropped so toggles never echo back as external changes.

**Coalescing (#19):** the worker drains its command backlog after each command; any burst of
`RefreshAll`/config-changed commands collapses into a single rebuild.

## Foreground-app toggle

Executed on the audio worker when the foreground action fires
(`audio/sessions.rs::toggle_foreground`):

1. Resolve sessions by the ladder below.
2. Decide target: all-muted → unmute all; otherwise (Active or Mixed) → mute all.
3. Apply `SetMute` per matched session; individual failures accumulate instead of aborting.
4. **Re-read actual mute state via `GetMute` after apply (#17c)** — the published overlay state
   is derived from what is really set plus accumulated failures, never from assumed success.
   Partial failure publishes truthful `Mixed`; only "all sessions disappeared before apply"
   is an error.

Aggregate semantics (`audio/state.rs::Aggregate`): `NoSession`, `AllMuted`, `AllActive`,
`Mixed`, `NoExternalApp`, `Error` (operational failure — never masked as NoSession, #17d).

### Session resolution ladder (#46/#18)

1. **Exact PID on the configured render endpoint.**
2. **Cross-endpoint exact-PID sweep** over every active render endpoint.
3. **Name fallback, fail-closed**: collect every process owning a render session; keep those
   whose image stem matches the foreground stem; group candidates by lowercased FULL executable
   path. One group ⇒ serve all its sessions (covers Chrome/Electron-style multi-process apps).
4. **Ambiguity refusal**: more than one group ⇒ fail closed — nothing is muted, `Error`
   ("ambiguous audio target") naming the collision.
5. Candidates whose image path cannot be discovered form **pid-private groups** (sentinel key)
   so they can never merge with an unrelated installation.

Accepted limitation: two INDEPENDENT instances launched from the SAME executable full path share
one group and are treated as one application — enforced as an invariant by proptest
(`fallback_groups_never_merge_distinct_paths`, #37).

Edge cases:

| Case | Behavior |
|---|---|
| no foreground window / self | use last tracked external pid (WinEvent hook); else `NoExternalApp` |
| zero matching sessions | `NoSession` |
| process exits mid-enumeration | skip session; enumeration errors tolerated |
| system/protected processes | sessions absent or PID unavailable → skipped |
| exclusive-mode owner | endpoint volume still toggles via its own path |

Foreground tracking: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` records `(pid)` of non-self
foreground windows into an atomic slot; Show Status issues a live `QueryForeground` rather than
rendering cached state.

## Invalidation retry

Errors matching `AUDCLNT_E_DEVICE_INVALIDATED`,
`AUDCLNT_E_ENDPOINT_CREATE_FAILED`, `AUDCLNT_E_SERVICE_NOT_RUNNING` trigger **exactly one**
rebuild-and-retry of the toggle; a second failure surfaces the error. Missing endpoint bindings
get one rebuild attempt before reporting Unavailable.

## Threading recap

Commands in: `mpsc::Sender<AudioCommand>`. Events out: typed `AppEvent`s posted to the main
window. No interface leaves the thread. Callback-after-free prevented by unregistering before
drops and joining before main teardown (WIN32_LIFETIME.md).

## Known limitations

* Sessions whose PID cannot be obtained (including packaged/AppContainer apps not exposed via
  `IAudioSessionEnumerator`) are skipped and surface as `NoSession`. WinShort uses only classic
  session enumeration; there is no WASAPI2 package-session support.
* Same-full-path independent instances behave as one app (above).
