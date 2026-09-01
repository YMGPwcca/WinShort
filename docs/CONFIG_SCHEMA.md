# Config Schema

**Status: Implemented** — reconstructed from `src/config/` (model.rs, load.rs, save.rs,
validate.rs) and `src/keyboard/binding.rs`. Every field/range/default below is quoted from code.

File: `%LOCALAPPDATA%\WinShort\config.toml` (resolved via `SHGetKnownFolderPath`; fallbacks
`%LOCALAPPDATA%\WinShort`, then `%TEMP%\WinShort`). Written atomically: sibling temp file →
`write_all` → `flush` → `sync_all` → `rename` over the target; temp removed on rename failure.
No backup copies are kept.

schema_version = 9              # u8; CURRENT value is 9 (v1/v2/v3/v4/v5/v6/v7/v8 files migrate on load)

[general]
start_hotkeys_enabled = true    # engine starts unsuspended
start_with_windows = false      # LEGACY (#16): always false; registry owns startup

[overlay]
enabled = true
duration_ms = 1300              # valid 500..=10000
position = "bottom-center"      # top-left|top-center|top-right|center-left|center|center-right|bottom-left|bottom-center|bottom-right
monitor = "foreground"          # foreground | primary | "device:\\\\.\\DISPLAY1"
scale = 1.0                     # valid 0.7..=1.6
opacity = 1.0                   # valid 0.3..=1.0
appearance = "system"           # system | dark | light
show_external_audio_changes = true

[audio]
input_role = "console"          # console|multimedia|communications (default console)
output_role = "console"
input_device = "default"        # "default" or an opaque endpoint ID string (#7)
output_device = "default"
cycle_input_allowlist = []      # omitted = all active; [] = disable input cycling
cycle_output_allowlist = []     # omitted = all active; [] = disable output cycling

[hotkeys]
toggle_microphone = "Ctrl+Alt+M"
toggle_output = "Ctrl+Alt+O"
toggle_foreground_audio = "Ctrl+Alt+P"
cycle_input_device = ""         # unassigned by default
cycle_output_device = ""
foreground_volume_up = ""
foreground_volume_down = ""
# Each record binds one stable profile ID; profile names are not references.
[[hotkeys.display_profiles]]
profile_id = "gaming-id"
hotkey = "Ctrl+Alt+F13"

[virtual_desktops]
enabled = true
win_number_switching = true     # numbered family enabled
number_modifier = "Win"         # one modifier family for 1..9
move_follow_modifier = ""       # optional modifier family
move_silent_modifier = ""       # optional modifier family
previous_desktop = ""           # optional ordinary hotkey
scratchpad_assign = ""          # optional hotkey; legacy wire name: send foreground window to Special Workspace
scratchpad_toggle = ""           # optional hotkey; legacy wire name: toggle Special Workspace / return desktop

[display_profiles]
enabled = true
active_profile = ""
# Profiles are persisted as [[display_profiles.profiles]] tables with
# id/name/topology and nested [[...routes]] DisplayConfig scalar fields.
# `confirmed = true` is written only after Test Apply + Keep. New, duplicated,
# updated, or edited profiles remain false and require the safe test workflow.
[[display_profiles.profiles]]
id = "gaming-id"
name = "Gaming"
topology = "extend"             # internal|clone|extend|external|custom
confirmed = false
[[display_profiles.profiles.routes]]
target_path = "\\\\?\\DISPLAY#MONITOR-A"
# Route tables also contain source/target adapter IDs, path/status flags,
# source position/size/pixel format, rotation/scaling/refresh, and target
# signal timing/active/total/video-standard fields captured from DisplayConfig.

## Internal representation

Raw strings exist only at the TOML boundary (`config/load.rs`). After parsing:

```rust
struct Hotkey { modifiers: ModifierMask /*u8 bitflags*/, key: VirtualKey }
enum OverlayPosition { TopLeft, TopCenter, TopRight, CenterLeft, Center, CenterRight, BottomLeft, BottomCenter, BottomRight }
enum OverlayAppearance { System, Dark, Light }
enum MonitorChoice { Foreground, Primary, Device(String) }   // Device = stable monitor name "\\.\DISPLAYn" (#26)
enum EndpointRole { Console, Multimedia, Communications }
struct AudioCfg {
  cycle_input_allowlist: Option<Vec<String>>,
  cycle_output_allowlist: Option<Vec<String>>,
}
struct Config { general: GeneralCfg, overlay: OverlayCfg,
                audio: AudioCfg, hotkeys: HotkeysCfg, virtual_desktops: VdCfg,
                display_profiles: DisplayProfilesCfg }
struct DisplayProfilesCfg { enabled: bool, active_profile: Option<String>,
                             profiles: Vec<DisplayProfile> }
struct DisplayProfile { id: String, name: String, topology: DisplayTopology,
                        confirmed: bool, routes: Vec<DisplayRoute> }
```

Monitor wire format is `"device:{name}"`; the legacy `"index:N"` string still parses but maps to
`Primary` (#26 migration). `Hotkey` display order is fixed `Ctrl+Alt+Shift+Win+<Key>`;
parse accepts any order and side-insensitive modifier names. **At least one modifier is
required — bare keys are rejected** (#35). Numpad tokens (`Numpad0`–`Numpad9`,
`NumpadAdd`, …) stay distinct from top-row siblings (#11). Supported key universe:
letters, digits 0–9, F1–F24, navigation/edit/OEM punctuation, CapsLock; no media keys.

`VdCfg` stores the numbered modifier family, optional move/follow and silent
modifier families, an optional previous-desktop hotkey, and the two legacy-named
`scratchpad_*` hotkeys. Those wire names are retained for schema compatibility, but
the actions now send the foreground window to a dedicated Special Workspace and
toggle that Virtual Desktop. Neither workspace identity nor return-desktop identity
is serialized into `config.toml`: the return GUID is process-only, while the exact
Special Workspace GUID is stored separately as operational recovery state in
`%LOCALAPPDATA%\WinShort\special-workspace.guid` so a surviving workspace can be
reclaimed after a hard kill or Windows reboot. Legacy schema-v6 `routing_rules`
tables are accepted only at the TOML boundary, ignored with a warning, and omitted
on the next Save.

`AudioCfg` stores optional endpoint-ID allowlists for input and output cycling.
`None` means all currently active endpoints; `Some(empty)` is an explicit deny-all
policy. Endpoint IDs remain opaque strings and are retained across offline periods.

`DisplayProfilesCfg` stores persisted profile IDs/names, a `confirmed` safety bit, topology kind,
stable target paths, source positions, and mode values. `confirmed` is false on migration and
after any edit; only a successful Test Apply followed by Keep makes it true. Runtime DisplayConfig
buffers and rollback tokens are never serialized.
Route identity combines the target device path with source/target IDs; saved adapter LUIDs
disambiguate same-panel connector collisions, while unresolved or missing routes fail closed.

`HotkeysCfg` retains the three existing defaulted toggle bindings, four optional audio/volume
fields, and `display_profiles`, a list of `{ profile_id, hotkey }` records. Profile IDs are stable
references; renaming does not change them and deleting a profile removes its record.

## Display profile lifecycle

New from Current, Update from Current, Duplicate, and route edits produce an unconfirmed
profile. Test Apply validates every route, applies the supplied DisplayConfig temporarily,
and starts a 15-second main-window timer. Keep persists the active Windows topology and the
confirmed profile; Revert or timeout restores the captured paths and modes. A failed restore
keeps the pending token and exposes a high-priority recovery error instead of accepting or
discarding the change.

## Future-schema read-only latch

Loading a document with `schema_version > 9` (`config/load.rs::load`):

An absent `schema_version` is treated as legacy source schema v1. New files
serialized by `Config::to_toml()` always write schema v9.

Diagnostics separates `source_schema_version` from `effective_schema_version`:
missing/corrupt input has no source version and effective v9; v1/v2/v3/v4/v5/v6/v7/v8 input has
its source version and effective v9; v9 input has source and effective v9. A future source version
is retained while runtime state falls back to safe defaults and the read-only latch remains active.

* logs an error and warns "config written by a newer WinShort; not overwriting",
* returns **default values for every section** (the future document is never partially applied),
* sets the process-global `CONFIG_READONLY: AtomicBool` latch (`config/mod.rs`).

While latched, every `save()` refuses with an error — a future-schema config can
never be silently overwritten or downgraded through any normal Save path. The
latch is memory-only and clears on process restart.

## Unknown fields

Serde does not deny unknown fields; instead load performs a manual double-parse against
`known_keys()` (`config/model.rs`) and emits warnings: `unknown key {section}.{key}`,
`unknown section {section}`, bare top-level values, unknown `position`/`monitor`/role strings
(these fall back to defaults). Load always proceeds; warnings are surfaced in the UI footer.

## Validation vs repair

`config/validate.rs::validate` produces `Vec<Violation>` (dotted field paths):

* hotkeys must parse, be pairwise distinct across all configurable actions and numbered modifier families, and require a modifier (#35)
* when `virtual_desktops.enabled && win_number_switching`, `number_modifier + digit 1..9` is reserved;
  collisions with explicit hotkeys are rejected using the configured modifier family
* optional move/follow and silent modifier families must not overlap each other, the numbered family,
  or explicit hotkeys
* `previous_desktop` participates in the same centralized conflict validation
* allowlist entries must be non-empty endpoint IDs with no duplicate exact IDs; an explicit empty list is valid
* display profiles require unique non-empty IDs/names, at least one route, stable target paths, positive modes, valid rotation, a compatible clone/extend source shape, and an active profile that exists
* display profile hotkeys require existing profile IDs, unique stable ID keys, and no conflict with any ordinary or virtual-desktop binding

`Config::repair` then fixes violations in-memory so the app stays usable:
out-of-range `duration_ms → 2000`, `scale → 1.0`, `opacity → 0.85`; conflicting hotkey binding
→ `None`; an invalid numbered modifier returns to `Win`; invalid allowlist entries,
malformed/duplicate display profiles, and stale/conflicting display profile hotkeys are removed
while preserving the first valid entry. Repair is idempotent (repaired values are
themselves valid).

## Endpoint device IDs

Endpoint IDs are **opaque strings**, never GUID-validated (#7): `"default"` selects the role's
default device; any other non-empty string is passed through to Core Audio unchanged. Typical
Windows form is `{0.0.0.00000000}.{guid}`, but nothing may assume it.

The Control Center device picker lists only real active endpoints. The `"default"`
selection remains a configuration binding mode and is displayed using
current-system-default metadata; it is never emitted as a fake picker endpoint.

## Startup authority (#16)

The registry is the single source of truth: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
value `WinShort` = quoted current exe path. `platform/startup.rs` compares registered vs running
command case-insensitively (`StartupState::{Enabled, Stale, Disabled}`). Toggling in the Control
Center or the tray writes/deletes immediately — it does **not** wait for a global Save. The legacy
`general.start_with_windows` config key is warned about, ignored, and always written back as
`false`.

## Defaults (as coded)

| Field | Default |
| overlay | enabled, 1300 ms, bottom-center, foreground monitor, scale 1.0, opacity 1.0, System appearance, external audio changes shown |
| audio roles / devices | console / console, default devices |
| audio cycle allowlists | omitted (`None`, all active endpoints) |
| existing toggle hotkeys | Ctrl+Alt+M / Ctrl+Alt+O / Ctrl+Alt+P |
| cycle and foreground-volume hotkeys | unassigned |
| display profiles | enabled, no active profile, no stored profiles; profile hotkeys empty |
| `start_hotkeys_enabled` | true |
| virtual desktops | enabled, `win_number_switching` true, number family `Win`, move families/previous/Special Workspace hotkeys unassigned |

Repair fallbacks (2000 / 1.0 / 0.85) differ from these defaults by design.

## Logging policy

Runtime logging level is operational state, not configuration. Release builds
start at Info; debug builds start at Debug. The Advanced page can enable
temporary Debug logging until restart, but no logging level field is persisted
in `config.toml` and #32 does not require a schema bump.

## Migration

Schema v1 files, including versionless legacy files, load with the v2 defaults for
`overlay.appearance` (`system`) and `overlay.show_external_audio_changes` (`true`),
v3 defaults for the four Phase-1 hotkeys, v4 defaults for the Virtual Desktop
workflow fields, v5 defaults for the legacy-named `scratchpad_*` hotkeys (now Special
Workspace actions), v7 defaults for
input/output allowlists, v8 defaults for display profiles, and v9 defaults for
display profile hotkeys plus
`confirmed = false` on profiles that predate the safety bit. Schema v2/v3/v4/v5/v6/v7/v8
files preserve all existing values and default only the newly introduced fields.
Schema-v6 executable-routing tables remain parse-compatible but are ignored as a removed,
unreleased feature and are not serialized again. Load diagnostics records the source/effective
transition. A successful Save writes schema v9
and updates active load diagnostics to source v9. Legacy `overlay.monitor = "index:N"` still
maps to `primary`, and `general.start_with_windows` remains ignored because startup is
registry-owned.
