# Config Schema

File: `%LOCALAPPDATA%\WinShort\config.toml`. Written atomically (temp file + rename) on Save.
Schema version field allows future migration.

```toml
schema_version = 1

[general]
start_with_windows = false     # mirrors HKCU\...\Run value; toggling updates the registry
launch_minimized = true        # reserved: window starts hidden (always true today)
start_hotkeys_enabled = true   # = suspended=false at boot

[overlay]
enabled = true
duration_ms = 1300             # 500..=10000
position = "bottom-center"     # top-left|top-center|top-right|center|bottom-left|bottom-center|bottom-right
monitor = "foreground"         # foreground|primary|primary-alt? no → foreground|primary|"index:N"
scale = 1.0                    # 0.7..=1.6
opacity = 1.0                  # 0.3..=1.0

[audio]
input_role = "console"         # console|multimedia|communications
output_role = "console"
input_device = "default"       # "default" or endpoint GUID string "{...}"
output_device = "default"

[hotkeys]
toggle_microphone = "Ctrl+Alt+M"
toggle_output = "Ctrl+Alt+O"
toggle_foreground_audio = "Ctrl+Alt+P"

[virtual_desktops]
enabled = true
win_number_switching = true    # Win+1..9 override
prefer_native_backend = true   # false forces keyboard fallback (diagnostics)
```

## Internal representation

Raw strings exist only at the TOML boundary (`config/load.rs`, `config/save.rs`).
After parsing, everything is typed:

```rust
struct Hotkey { modifiers: ModifierMask /*bitflags u8*/, key: VirtualKey }
enum OverlayPosition { TopLeft, TopCenter, TopRight, Center, BottomLeft, BottomCenter, BottomRight }
enum MonitorChoice { Foreground, Primary, Index(u32) }
enum EndpointRole { Console, Multimedia, Communications }
struct Config { schema_version: u8, general: GeneralCfg, overlay: OverlayCfg,
                audio: AudioCfg, hotkeys: HotkeysCfg, virtual_desktops: VdCfg }
```

`Hotkey` display string canonicalization: `Ctrl+Alt+Shift+Win+<Key>` in fixed order;
parse accepts any order and side-insensitive modifier names; digits/letters/F1-24/media keys
accepted. `VirtualKey` is an enum over the supported range with `TryFrom<u16>`.

## Validation (`config/validate.rs`)

* hotkeys present + parseable + pairwise distinct (conflict lists both action names)
* numeric ranges clamped-or-rejected as above (reject, never silently clamp, on save)
* enum strings must match exactly (no fuzzy matching)
* device GUIDs shape-checked `^{8-4-4-4-12}$`
* unknown fields in TOML → warning logged, ignored (forward compatibility)
* load failure (missing/corrupt file) → defaults + log warning; UI still opens so user can fix

## Live snapshot flow

Save click → build candidate `Config` from draft → validate → serialize → atomic write →
construct `ConfigSnapshot { config: Arc<Config>, bindings: BindingTable }` → swap under
`ArcSwap`-style lock → post `ConfigApplied(seq)` → subsystems re-read snapshot on next use.
Cancel → draft discarded, live untouched.
