# UI Design

**Status: Implemented** sections describe current `main`; **Planned** sections are design
intent only (tracked in #29/#30/#31) and must not be read as existing behavior.

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
re-read on `WM_SETTINGCHANGE` — **settings window only**; the overlay always renders dark
(planned: follow system there too).

## Widget set — current (`ui/layout.rs`, `ui/controls.rs`)

Retained-lite: a deterministic layout pass produces `Element { id, rect, kind }`; paint walks
it, input hit-tests it. Animation tweens (cubic ease-out) tick on a 16 ms WM_TIMER that runs
only while motion or recording is active; idle UI has no render loop.

* Toggle row — animated knob
* Hotkey recorder row — capture-mode box with inline conflict/validation error; Esc cancels;
  modifier-only rejected (#35)
* Value rows (device pickers, position, monitor) — click **cycles** values in place; no popup
* Slider rows — duration / opacity / scale, mouse drag with capture
* Buttons — footer Cancel / Save (Save disabled until draft differs from live config)
* Scrollable content column — wheel scrolling with slim custom scrollbar
* Status row (Advanced) — composed virtual-desktop backend text

Keyboard accessibility actually implemented: Tab / Shift-Tab cycles a fixed focus order
(skipping disabled rows), Space/Enter activate the focused row, focus ring drawn, focused row
scrolled into view.

## Widget set — planned (NOT implemented)

* Real dropdown popup list windows for device/enum pickers (#29)
* Native `TOOLTIPS_CLASS` tooltips (#29)
* Arrow-key slider control and richer keyboard interaction model (#29)
* Reduced-motion / high-contrast respect and calmer overlay motion (#30)

## Settings layout — current

Width 610 logical dip; height fits content up to work area − 48. Sections top-to-bottom:
General, Hotkeys, Audio, Virtual Desktops, Overlay, Advanced (read-only backend status).
Dirty-state: Save enabled only when draft ≠ live; Cancel restores the live snapshot.

## Overlay — current

Window: `WS_POPUP` with `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE |
WS_EX_TRANSPARENT`. Never activates, never taskbar/Alt-Tab, click-through. Positioned per config
within the selected monitor's work area (foreground / primary / device match with fallback).

Rendering: one persistent layered HWND; each `show()` renders the card bitmap once at the
target monitor's effective DPI (#49), then a Phase state machine (`Appearing` 140 ms →
`Holding` → `Leaving` 180 ms) ticks a 16 ms timer varying layer alpha (ease curves) and slide
offset — CPU-composited frames through `UpdateLayeredWindow`, no GPU swapchain. `WM_DPICHANGED`
is deliberately ignored for the overlay: it owns its own size/position and re-renders at the
new monitor's DPI on next show.

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
