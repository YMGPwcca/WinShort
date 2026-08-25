# Config Schema

**Status: Implemented** — reconstructed from `src/config/` (model.rs, load.rs, save.rs,
validate.rs) and `src/keyboard/binding.rs`. Every field/range/default below is quoted from code.

File: `%LOCALAPPDATA%\WinShort\config.toml` (resolved via `SHGetKnownFolderPath`; fallbacks
`%LOCALAPPDATA%\WinShort`, then `%TEMP%\WinShort`). Written atomically: sibling temp file →
`write_all` → `flush` → `sync_all` → `rename` over the target; temp removed on rename failure.
No backup copies are kept.

```toml
schema_version = 1              # u8; CURRENT value is 1 (hard-coded in model.rs/to_toml)

[general]
start_hotkeys_enabled = true    # engine starts unsuspended
start_with_windows = false      # LEGACY (#16): always false; registry owns startup

[overlay]
enabled = true
duration_ms = 1300              # valid 500..=10000
position = "bottom-center"      # top-left|top-center|top-right|center|bottom-left|bottom-center|bottom-right
monitor = "foreground"          # foreground | primary | "device:\\\\.\\DISPLAY1"
scale = 1.0                     # valid 0.7..=1.6
opacity = 1.0                   # valid 0.3..=1.0

[audio]
input_role = "console"          # console|multimedia|communications (default console)
output_role = "console"
input_device = "default"        # "default" or an opaque endpoint ID string (#7)
output_device = "default"


[virtual_desktops]
enabled = true
win_number_switching = true     # reserves Win+1..9 while true
```

## Internal representation

Raw strings exist only at the TOML boundary (`config/load.rs`). After parsing:

```rust
struct Hotkey { modifiers: ModifierMask /*u8 bitflags*/, key: VirtualKey }
enum OverlayPosition { TopLeft, TopCenter, TopRight, Center, BottomLeft, BottomCenter, BottomRight }
enum MonitorChoice { Foreground, Primary, Device(String) }   // Device = stable monitor name "\\.\DISPLAYn" (#26)
enum EndpointRole { Console, Multimedia, Communications }
struct Config { schema_version: u8, general: GeneralCfg, overlay: OverlayCfg,
                audio: AudioCfg, hotkeys: HotkeysCfg, virtual_desktops: VdCfg }
```

Monitor wire format is `"device:{name}"`; the legacy `"index:N"` string still parses but maps to
`Primary` (#26 migration). `Hotkey` display order is fixed `Ctrl+Alt+Shift+Win+<Key>`;
parse accepts any order and side-insensitive modifier names. **At least one modifier is
required — bare keys are rejected** (#35). Numpad tokens (`Numpad0`–`Numpad9`,
`NumpadAdd`, …) stay distinct from top-row siblings (#11). Supported key universe:
letters, digits 0–9, F1–F24, navigation/edit/OEM punctuation, CapsLock; no media keys.

## Future-schema read-only latch

Loading a document with `schema_version > 1` (`config/load.rs::load`):

* logs an error and warns "config written by a newer WinShort; not overwriting",
* returns **default values for every section** (the future document is never partially applied),
* sets the process-global `CONFIG_READONLY: AtomicBool` latch (`config/mod.rs`).

While latched, every `save()` refuses with an error — a future-schema config can never be
silently overwritten or downgraded through any normal Save path. The latch is memory-only and
clears on process restart.

## Unknown fields

Serde does not deny unknown fields; instead load performs a manual double-parse against
`known_keys()` (`config/model.rs`) and emits warnings: `unknown key {section}.{key}`,
`unknown section {section}`, bare top-level values, unknown `position`/`monitor`/role strings
(these fall back to defaults). Load always proceeds; warnings are surfaced in the UI footer.

## Validation vs repair

`config/validate.rs::validate` produces `Vec<Violation>` (dotted field paths):

* numeric ranges: `duration_ms 500..=10000`, `scale 0.7..=1.6`, `opacity 0.3..=1.0`
* hotkeys must parse, be pairwise distinct (conflict message names both actions), and require a
  modifier (#35)
* reserved slots: when `virtual_desktops.enabled && win_number_switching`, exactly
  `Win + digit 1..9` conflicts ("conflicts with reserved virtual-desktop shortcut Win+n");
  allowed again when the reserve is off; other modifiers+digits never conflict

`Config::repair` then fixes violations in-memory so the app stays usable:
out-of-range `duration_ms → 2000`, `scale → 1.0`, `opacity → 0.85`; conflicting hotkey binding
→ `None`. Repair is idempotent (repaired values are themselves valid). Whitespace-only endpoint
IDs are rejected as defense-in-depth.

## Endpoint device IDs

Endpoint IDs are **opaque strings**, never GUID-validated (#7): `"default"` selects the role's
default device; any other non-empty string is passed through to Core Audio unchanged. Typical
Windows form is `{0.0.0.00000000}.{guid}`, but nothing may assume it.

## Startup authority (#16)

The registry is the single source of truth: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
value `WinShort` = quoted current exe path. `platform/startup.rs` compares registered vs running
command case-insensitively (`StartupState::{Enabled, Stale, Disabled}`). Toggling in Settings or
the tray writes/deletes immediately — it does **not** wait for Save. The legacy
`general.start_with_windows` config key is warned about, ignored, and always written back as
`false`.

## Defaults (as coded)

| Field | Default |
|---|---|
| hotkeys | `Ctrl+Alt+M` / `Ctrl+Alt+O` / `Ctrl+Alt+P` |
| overlay | enabled, 1300 ms, bottom-center, foreground monitor, scale 1.0, opacity 1.0 |
| audio roles / devices | console / console, default devices |
| `start_hotkeys_enabled` | true |
| virtual desktops | enabled, `win_number_switching` true |

Repair fallbacks (2000 / 1.0 / 0.85) differ from these defaults by design.

## Migration

There is no generic migration machinery — only forward-latch (future schema) plus two
incidental legacy parsings: `"index:N"` monitors → `Primary` (#26), ignored
`start_with_windows` (#16).
