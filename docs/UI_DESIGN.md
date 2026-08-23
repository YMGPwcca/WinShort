# UI Design

## Stack

| Layer | Technology |
|---|---|
| Window chrome | User32; `DwmSetWindowAttribute`: dark titlebar (`DWMWA_USE_IMMERSIVE_DARK_MODE`), `DWMWA_WINDOW_CORNER_PREFERENCE = ROUND`, backdrop probe with graceful fallback |
| Rendering | Direct2D (ID2D1DeviceContext) onto DXGI flip-model swapchain |
| Text | DirectWrite, Segoe UI Variable (falls back Segoe UI) |
| Overlay compositing | DirectComposition premultiplied-alpha visual; opacity/offset animations run in DWM |

Both windows share one renderer abstraction (`ui/renderer.rs`): device creation, swapchain
resize, BeginDraw/EndDraw, device-lost recovery (full re-create once per occurrence).

## Design tokens

Logical px at 96 DPI; everything multiplies by the window's current DPI scale.

```
radius.card 8   radius.control 6
space.xs 4  sm 8  md 12  lg 16  xl 24
font.caption 12  font.body 14  font.title 20

dark:  bg #202020  card #2b2b2b  card-hover #313131  stroke #383838
       text #ffffff  text-dim rgba(255,255,255,.62)
       accent #60cdff  accent-dim #0078d4? no → #4cc2ff hover #99e0ff
danger muted #e5795e  ok #6cccb5
light: bg #f3f3f3 card #fbfbfb ... mirrored
```

Theme follows `AppsUseLightTheme` (HKCU Themes\Personalize), re-read on `WM_SETTINGCHANGE`.

State colors (overlay): Muted `#e5795e` (muted red/orange), Active `#6cccb5`, Changed accent,
Unavailable gray. No saturation abuse.

## Widget set

Retained-lite: layout pass produces `Vec<Element { id, rect, kind }>`, paint walks it,
input hit-tests it. Animation state keyed by element id. Event-driven — a timer runs only while
animations are active (~120–220 ms transitions, ease-out cubic).

* `Section` header + rounded card container
* `ToggleRow` label + animated toggle (knob slides 140 ms)
* `HotkeyRow` label + recorder box ("Press a shortcut…" capture mode, conflict inline error)
* `DropdownRow` custom popup list window (device pickers, position/monitor/role enums)
* `SliderRow` duration / opacity / scale
* `Button` primary (Save) / secondary (Cancel); disabled state when draft == live config
* Scrollable content column with smooth wheel scrolling and slim scrollbar
* Footer bar: validation message slot · Cancel · Save · transient "✓ Applied"

Focus ring: 2 px accent outline; Tab cycles focusables; Space/Enter activate; arrows move sliders.
Tooltips via native `TOOLTIPS_CLASS` where useful (backend status rows).

## Settings layout

Width 580 logical px, height fits content up to work area − 48. Sections top-to-bottom:
General, Hotkeys, Audio, Virtual Desktops, Overlay, Advanced. Dirty-state: Save enabled only when
draft ≠ live; Cancel restores live snapshot into draft.

## Overlay

Card sized to content (icon + two-line rows), max ~3 rows. Entrance: fade 0→1 + slide-up 12 px,
120 ms. Hold: `overlay.duration_ms`. Exit: fade + slide-down 180 ms. Repeat actions reset the
timer and morph content in place. Rendered on its own premultiplied swapchain; DComp visual
opacity animation performs fades without CPU frames.

Window styles: `WS_POPUP`, `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE |
WS_EX_TRANSPARENT | WS_EX_NOREDIRECTIONBITMAP`. Never activates, never taskbar/Alt-Tab,
click-through. Positioned per config within the target monitor's work area
(`MonitorFromWindow` of foreground window / primary / specific HMONITOR).

Icons: hand-authored D2D path geometry (microphone, speaker, app window, desktop grid, warning).
Vector at every DPI; no emoji fonts, no bitmaps.

## Quality gate

Before calling any screen "done": spacing consistent, alignment on the 8 px grid, text renders
crisp at 100/125/150/175/200 %, hover/press/focus states all present, dark+light both checked,
window resize keeps footer pinned, animations ≥ 50 fps during motion only, error/disabled/empty
states designed (device missing, backend unsupported, invalid hotkey).
