# Audio Design

**Status: Implemented** (endpoints, notifications, system-default device cycling,
foreground resolver, mute, and volume semantics). Known limitations listed at
the end are accepted behavior, not gaps.

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
state. Device enumeration failures degrade per-flow with warnings surfaced in the Control Center
and Diagnostics (#36), never aborting the whole subsystem.

## System-default device cycling

Phase-1 cycle hotkeys enumerate active real endpoints, read the current Console
default as the cycle cursor, select the next endpoint by opaque ID, and set
that endpoint as the Windows default for Console, Multimedia, and
Communications. The isolated `IPolicyConfig` wrapper performs the native
default switch; successful switches immediately rebuild both endpoint flows
and publish the normal device-cycle overlay. The Control Center draft is not
rewritten by a cycle hotkey.

`audio.cycle_input_allowlist` and `audio.cycle_output_allowlist` optionally restrict each
cycle ring by opaque endpoint ID. Omitted (`None`) preserves all-active behavior; an explicit
empty list disables that direction. The allowlist is applied to the active inventory at
planning time, so a disconnected endpoint is skipped without deleting its configured ID.
`RefreshAll` after a device notification rebuilds the inventory; a later reconnect makes the
configured endpoint eligible again without polling or migration.

The Control Center device picker contains only real active endpoints. Selecting
one sends an MTA-owned command that sets it as the Windows default for Console,
Multimedia, and Communications; the UI thread never touches Core Audio COM. A
`Default` configuration selection remains the internal follow-the-system-default
binding mode and is represented by current-default metadata rather than a
selectable pseudo-device. Missing explicit bindings remain readable for backward
compatibility but are not exposed as system-switch targets while disconnected.

## Notifications (no polling)

* `IMMNotificationClient::OnDefaultDeviceChanged` carries flow, role, and endpoint ID to the
  worker; external changes trigger a coherent refresh of endpoint state and current-default
  metadata.
* Other endpoint state/add/remove/property changes send `RefreshAll` commands.
* `IAudioEndpointVolumeCallback` per endpoint — external volume/mute changes arrive as commands.

Both run on the audio thread, hold no references into worker state (raw post target only), and
are unregistered before their interfaces drop.

**Own-event filtering (#19/#36):** WinShort passes distinct event-context GUIDs to its own
`SetMute` calls (endpoint context and session context); callbacks carrying those contexts are
dropped so toggles never echo back as external changes.

**Coalescing (#19):** the worker drains its command backlog after each command; any burst of
`RefreshAll`/config-changed commands collapses into a single rebuild.

Each rebuild obtains one owned `ConfigSnapshot`; Capture and Render use its same
`Arc<Config>` and provenance stamp, then release the config lock before Core Audio work.

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

### Persistent muted-executable inventory

`audio/applications.rs` reads live sessions across all active render endpoints on the audio worker. It skips expired sessions and system-sound PID zero, combines sessions from the same normalized full executable path, and publishes only executable groups whose live sessions are all muted. Unavailable image paths retain a PID-private identity; same-name installations at different paths stay independent. The UI uses this inventory for persistent executable badges, separate from the selected-app state used by shortcuts and Show Status.

The worker scans at most once per second during normal operation and immediately after app mute toggles. It publishes changed snapshots only. Operational scan failures leave the previous snapshot intact, while successful scans remove programs that unmuted or no longer have live sessions. This observer does not write session mute/volume, configuration, or persisted rules. Existing foreground mute/volume resolution and operation semantics stay intact.

### Foreground-app volume adjustment

The Phase-1 volume actions reuse the same foreground session resolution ladder
as mute toggling. Each matched `ISimpleAudioVolume` is read, adjusted by
`±0.05`, clamped to `[0, 1]`, written with the existing session event context,
and read again. `SetMute` is never called. Published `AppVolumeState` retains
the number of matched sessions, the minimum and maximum successfully re-read
percentages, and any partial-operation error; it never presents an average for
different session levels.


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

* System-default switching uses the de-facto undocumented `IPolicyConfig`
  interface; Windows desktop/hardware acceptance remains required.
