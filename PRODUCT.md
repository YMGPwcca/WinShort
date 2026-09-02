# Product

<!-- impeccable:product-schema 1 -->

## Users

WinShort is for ordinary Windows users who want fast, keyboard-driven control of audio devices, workspaces, display arrangements, and transient status feedback without learning Windows implementation details. The primary operating scene is a resident tray utility used throughout the day alongside games, communication apps, creative tools, and work applications.

## Product Purpose

WinShort gives people dependable global shortcuts for microphone and speaker control, current-app audio, numbered desktops, a dedicated Special Desktop, display profiles, and a compact action overlay. Success means a user can understand what is active, change a shortcut or preference safely, and recover from unavailable Windows services without opening a developer-oriented configuration editor.

## Positioning

WinShort is a native, event-driven Windows utility that turns several fragile system capabilities into one coherent shortcut-and-control-center experience while preserving Windows defaults, safe display rollback, and truthful degraded states. It does not replace the Windows shell or require a browser/runtime framework.

## Operating Context

The application starts as a quiet single-instance tray process, initializes Windows Per-Monitor V2 DPI awareness, and keeps audio, keyboard, desktop, display, overlay, diagnostics, and startup work on their established owning threads. Frequent actions happen through global hotkeys; the Control Center is opened from the tray or second-instance activation. Display changes can affect physical monitors and therefore require explicit Test, Keep, Revert, and timeout recovery.

## Capabilities and Constraints

- Native Windows application in Rust using Win32/User32, Direct2D, DirectWrite, DWM, Windows APIs, and the existing UI Automation/provider architecture.
- Primary destinations are Home, Shortcuts, Audio, Workspaces, Displays, Overlay, and System. Advanced and Diagnostics are secondary destinations.
- Existing Core Audio, keyboard hook, Virtual Desktop/Special Desktop, DisplayConfig, overlay, config, logging, startup, and support-bundle backends remain authoritative.
- Normal language uses human concepts such as speaker, microphone, current app, shortcut, desktop, Special Desktop, display profile, and overlay. Technical identifiers stay in Advanced or Diagnostics.
- Configuration uses the existing typed schema, migrations, canonical validation, future-schema protection, atomic persistence, and coherent runtime publication. Simple controls commit locally; risky multi-field display work uses drafts.
- Special Desktop is the user-facing name for the compatibility-named scratchpad actions. The compatibility keys `scratchpad_assign` and `scratchpad_toggle` remain internal wire names.
- Audio cycling preserves opaque endpoint identities, active-device filtering, explicit empty allowlists, reconnect eligibility, and notification-driven refresh without polling.
- Display profiles preserve stable IDs, route disambiguation, profile hotkeys, documented DisplayConfig validation, and bounded Test/Keep/Revert rollback. No unattended destructive topology experiments are part of normal operation.
- The UI must remain usable in light, dark, High Contrast, reduced-motion, and 100/125/150/200% DPI environments, with truthful keyboard and UI Automation focus.
- No network requests, telemetry, fake runtime data, placeholder controls, or WebView/Electron/React/Qt/WPF/WinUI substitutions.

## Brand Commitments

The application title remains WinShort. The control center should feel like restrained modern Windows software: crisp Segoe UI typography, deliberate 8-DIP spacing, meaningful surfaces, one clear system accent, readable light and dark themes, and vector/native-style icons rather than emoji.

## Evidence on Hand

Repository evidence at the pinned implementation base includes the typed configuration and migration system, event-driven audio worker, keyboard capture safety, build-pinned Virtual Desktop backend with Special Desktop recovery, DisplayConfig profile safety state machine, layered overlay renderer, native picker/listbox, custom UI Automation provider, diagnostics sanitizer, startup integration, and PMv2 window geometry helpers. No visual-regression framework or browser UI is authoritative for this native application.

## Product Principles

1. User intent first; implementation detail last.
2. Every displayed state must be real, current, and actionable.
3. Fast shortcuts and local transactions should feel safe and reversible where the backend permits.
4. Risky system changes require explicit confirmation and fail-safe recovery.
5. A degraded optional subsystem must not make the control center unusable.

## Accessibility & Inclusion

Keyboard-only operation, visible focus, deterministic traversal, truthful UI Automation roles/patterns, screen-reader-readable headings and controls, disabled-focus repair, native picker semantics, High Contrast support, reduced motion, readable long labels, and PMv2 DPI-safe geometry are release requirements.
