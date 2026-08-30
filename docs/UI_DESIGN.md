# UI Design

**Status: Implemented** sections describe current `main`; **Planned** sections are design
intent only (tracked in #29 and the remaining manual #30 acceptance) and must not be read as existing behavior.

## Stack — current

| Layer | Technology | Status |
|---|---|---|
| Window chrome | User32; DWM: dark titlebar, corner preference ROUND, caption color | **Implemented** |
| Settings rendering | Direct2D `ID2D1HwndRenderTarget` + DirectWrite (`ui/renderer.rs`); BGRA8, per-target `SetDpi` | **Implemented** |
| Overlay compositing | WIC software render target → `CopyPixels` → `CreateDIBSection` → `UpdateLayeredWindow` | **Implemented** |
| Text | DirectWrite, "Segoe UI Variable Text" with "Segoe UI" fallback | **Implemented** |

There is no DXGI swapchain, no `ID2D1DeviceContext`, no DirectComposition anywhere in the
codebase. The two windows do not share a renderer object: `renderer.rs` serves settings only;
the overlay owns an independent WIC/DIB stack.

Device-loss recovery is implicit: any `EndDraw` failure drops the whole settings Renderer and
the next paint rebuilds factory/target/brushes from scratch. The overlay re-creates its surface
per `show()`.

## Design tokens — current (`ui/theme.rs`)

Logical px at 96 DPI; all layout math happens in 96-DIP space and scales by target DPI.
Dark theme: bg `#1f1f1f`, card `#2b2b2b`, card-hover `#313131`, border `#393939`,
text `#ffffff`, text-dim `rgb(191,191,191)`, accent `#60cdff`, hover `#99e0ff`,
pressed `#0078d4`, danger `#e5795e`, ok `#6cccb5`. Light theme mirrors (`bg #f3f3f3`,
card `#ffffff`, …). Spacing/radius values are inline literals in `controls.rs`/`layout.rs`
(8 px grid), not named constants.

Theme follows `AppsUseLightTheme` (`HKCU\…\Themes\Personalize`) via `RegGetValueW`,
re-read on `WM_SETTINGCHANGE` by both Settings and the overlay. Settings keeps its
existing owner-drawn theme; the overlay resolves its System/Dark/Light appearance separately.

## Widget set — current (`ui/layout.rs`, `ui/controls.rs`)

Retained-lite: a deterministic layout pass produces `Element { id, rect, kind }`; paint walks
it, input hit-tests it. Animation tweens (cubic ease-out) tick on a 16 ms WM_TIMER that runs
only while motion or recording is active; idle UI has no render loop.

* Toggle row — animated knob
* Hotkey recorder row — capture-mode box with inline conflict/validation error; Esc cancels;
  modifier-only rejected (#35)
* Picker rows — explicit native LISTBOX popup for input/output device, endpoint role, overlay
  appearance, position, and monitor; current selection is visible, keyboard navigable, Escape
  cancels, Enter/click commits, focus loss closes
* Display profile rows — New/Update from Current, Select, stable-ID hotkey recorder,
  route/topology selection, Rename, Duplicate, Delete, Test Apply, Keep, and Revert;
  route editing uses a native text prompt with rational refresh input
* Slider rows — duration / opacity / scale, mouse drag plus logical UI Automation RangeValue semantics
* Buttons — footer Cancel / Save (Save disabled until draft differs from live)
* Scrollable content column — wheel scrolling with slim custom scrollbar
* Status row (Advanced) — composed virtual-desktop backend text

Keyboard accessibility is implemented by a custom UI Automation provider in `ui/settings_automation.rs`.
The Settings HWND owns the only visual and pointer surface; `WM_GETOBJECT` returns a fragment root
whose logical children are derived from the same layout and values used for Direct2D rendering.
Provider reads consume an `Arc<RwLock<SettingsAutomationSnapshot>>`; Invoke, Toggle, Slider, and
focus actions are queued back to the Settings HWND. Native child BUTTON/TRACKBAR semantic overlays
are not created. The separate native LISTBOX picker remains keyboard navigable with focus-loss,
Escape, and Enter/click behavior. The hotkey recorder returns focus to the Settings window before
capture so global capture remains generation-safe.

UI Automation contract policy: picker and hotkey rows are Button controls with Invoke; they do not
claim ComboBox or Edit semantics. Their displayed text is exposed read-only through ValuePattern,
and `SetValue` returns `UIA_E_INVALIDOPERATION`. The fragment root returns the Settings HWND host
provider; logical children return no host provider. Unsupported patterns, missing navigation
boundaries, and outside point queries complete successfully with null results. Runtime IDs are
null for the hosted root and use `UiaAppendRuntimeId` plus a stable child index for descendants.
The provider tracks whether focus belongs to Settings, the native picker, or outside WinShort, so
logical child focus is never advertised while the picker LISTBOX owns keyboard focus. Snapshot
updates raise UIA property events only when focus, toggle, slider value, enabled, offscreen,
bounds, name, or displayed value actually changes.

Snapshot publication only commits state and queues typed notifications; UIA delivery is deferred
to a Settings HWND message after the `SettingsUi` borrow is dropped. Duplicate target/property
changes coalesce. Picker and hotkey Invoke actions raise one deferred Invoked event when accepted.

Provider action errors remain state-specific: disabled controls return
`UIA_E_ELEMENTNOTENABLED`, invalid slider values return `E_INVALIDARG`, unsupported
direct pattern calls return `UIA_E_NOTSUPPORTED`, and retained providers after
teardown return `UIA_E_ELEMENTNOTAVAILABLE`.

## Settings interaction — current

**Status: Implemented.** Audio device pickers show Default, current inventory, and a synthetic
`Selected device unavailable` choice when an explicit opaque endpoint is missing. The missing
selection is preserved until the user chooses another value. Endpoint role rows remain visible
but disabled with explanatory help when an explicit endpoint is selected.

Monitor choices are Foreground, Primary, and stable `Device(String)` names with current
resolution/work-area labels. A disconnected configured device remains as an unavailable choice;
`index:N` is never reintroduced into the UI.

Reset Settings requires a second explicit `Confirm reset` activation. It changes only the draft;
Save is still required, and the registry-authoritative Start with Windows state is untouched.

Settings position is persisted in the separate WinShort-owned `settings-window.txt` UI-state file
when the window closes or the app shuts down. Restored rectangles scale from their saved DPI,
select the nearest current monitor, and clamp enough of the window/title area into the work area.

Help uses inline row descriptions, native accessibility names, and delayed native
`TOOLTIPS_CLASS` popups for non-obvious settings. The overlay reads Windows animation,
high-contrast, and overlapped-content preferences and refreshes them at runtime.

## Widget set — planned (NOT implemented)

No #30 overlay accessibility behavior remains planned here. Full live Narrator acceptance
of the native Settings surface remains tracked separately in #29.

## Settings layout — current

Width 610 logical dip; height fits content up to work area − 48. Sections top-to-bottom:
General, Hotkeys, Audio, Virtual Desktops, Overlay, Advanced (temporary Debug logging,
Diagnostics & support entry, config folder, reset draft).
Dirty-state: Save enabled only when draft ≠ live; Cancel restores the live snapshot.

## Diagnostics & Support — current

**Status: Implemented.** The Advanced entry opens a separate native owner-drawn window rather
than expanding the Settings scroll page. It uses the shared Direct2D HwndRenderTarget,
DirectWrite, theme tokens, rounded DWM chrome, and PMv2 DPI handling.

The page is a dense read-only operator view: application/Windows build, keyboard hook and
bindings, audio endpoint availability and foreground aggregate, desktop backend/count/last
served, config path/schema/latch/warnings, overlay target/DPI, startup registration, runtime log
level/default/retention/buffering, and degraded startup reasons. It also exposes Copy Diagnostics,
Open Logs, Create Support Bundle, Run Self-Test, and Close.

Self-Test is passive: it observes current cached services, endpoint inventory, config metadata,
desktop status, overlay availability, logging directory metadata, and startup readability. It
does not mute audio, inject keys, switch desktops, change the registry, restart services, or
show an overlay.

Support exports use an explicit sanitizer projection rather than scraping UI text. Config and
logs are sanitized before a bounded local ZIP is written; endpoint IDs become report-local
tokens, absolute executable paths become basename + path token, and raw key history, window
titles, command lines, and unrelated process identity are excluded. No upload or telemetry is
performed.

## Overlay — current

Window: `WS_POPUP` with `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE |
WS_EX_TRANSPARENT`. Never activates, never taskbar/Alt-Tab, click-through. Positioned per config
within the selected monitor's work area (foreground / primary / device match with fallback).

Rendering: one persistent layered HWND; each `show()` renders the card bitmap once at the
target monitor's effective DPI (#49), then a Phase state machine (`Appearing` 140 ms →
`Holding` → `Leaving` 180 ms) ticks a 16 ms timer varying layer alpha (ease curves) and slide
offset — CPU-composited frames through `UpdateLayeredWindow`, no GPU swapchain. When Windows
client-area animations are disabled, the overlay uses a single settled frame with no fade or
slide. High contrast uses system window/highlight colors, an opaque surface, strong borders,
and no shadow. `SPI_GETDISABLEOVERLAPPEDCONTENT` also selects an opaque, shadow-free palette.
`WM_SETTINGCHANGE`, `WM_SYSCOLORCHANGE`, and `WM_THEMECHANGED` refresh the live policy.
`WM_DPICHANGED` is deliberately ignored for the overlay: it owns its own size/position and
re-renders at the new monitor's DPI on next show.

Settings Preview posts the current draft `OverlayCfg` directly; it does not save
or replace the live `ConfigHandle`. Normal hotkey/status presentations continue
to use the saved config. A deterministic active microphone row is used for the
preview.

High-contrast state circles use `COLOR_WINDOW`/`COLOR_WINDOWTEXT` for normal
states and the paired `COLOR_HIGHLIGHT`/`COLOR_HIGHLIGHTTEXT` colors for Changed.
State text remains explicit. Coalesced updates during Appearing preserve the
full configured settled hold after the appearance completes; a matching delayed
status query refreshes the multi-row status presentation instead of creating a
foreground-only result.

Icons: hand-authored D2D path geometry (microphone, speaker, app window, desktop grid,
warning). Vector at every DPI; no emoji fonts, no bitmaps.

## DPI — current (#49)

Process-wide PerMonitorV2 before any window creation. Settings sizes itself to the primary
monitor's DPI and handles `WM_DPICHANGED` (clamp ≥ 96, retarget + rebuild formats, resize to
suggested rect, relayout). Overlay uses `effective_render_dpi(target_monitor)` — the selected
monitor's effective DPI, never maxed across monitors.

## Quality gate

Applies to changes under `src/ui/`: spacing consistent on the 8 px grid, text crisp at
100–200 %, hover/press/focus/disabled states present, dark+light checked, error/empty states
designed (device missing, backend unsupported, invalid hotkey). Screenshot automation does not
exist yet (planned; see TEST_PLAN.md).
