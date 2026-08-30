from pathlib import Path


def replace_once(path, old, new, label):
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected 1 match in {path}, got {count}")
    p.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    "README.md",
    """- **Virtual desktop workflow** — configurable numbered switching creates only missing
  desktops through the build-pinned Shell COM backend; optional move/follow, silent
  move, previous-desktop focus restoration, and runtime scratchpad show/hide actions
  stay disabled or unassigned by default.
""",
    """- **Virtual desktop workflow** — configurable numbered switching creates only missing
  normal desktops through the build-pinned Shell COM backend; optional move/follow, silent
  move, previous-desktop focus restoration, and a native-only dedicated Special Workspace
  stay disabled or unassigned by default. The Special Workspace is excluded from 1–9 ordinals.
""",
    "README feature",
)
replace_once(
    "README.md",
    """Optional desktop workflow and scratchpad shortcuts are unassigned by default and can be
configured in Settings; the scratchpad assignment is runtime-only and is forgotten when
the window closes.
""",
    """Optional desktop workflow and Special Workspace shortcuts are unassigned by default and can
be configured in Settings. Sending a window moves it into one runtime-only dedicated Virtual
Desktop; toggling switches to that workspace and back to the remembered normal desktop.
""",
    "README shortcut semantics",
)
replace_once(
    "README.md",
    """The Settings → Advanced area opens Diagnostics & Support; the diagnostics page shows the active
backend and reason. Details and test evidence: [docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md).
""",
    """The Settings → Advanced area opens Diagnostics & Support; the diagnostics page shows the active
backend and reason. The Special Workspace requires the native Shell backend; on unsupported builds
normal numbered switching may still use keyboard fallback, but Special Workspace actions fail
closed instead of simulating creation or GUID-addressed switching. Details and test evidence:
[docs/VIRTUAL_DESKTOP_COMPAT.md](docs/VIRTUAL_DESKTOP_COMPAT.md).
""",
    "README native-only note",
)

replace_once(
    "docs/CONFIG_SCHEMA.md",
    'scratchpad_assign = ""          # optional hotkey; runtime-only window assignment\nscratchpad_toggle = ""           # optional hotkey; runtime-only show/hide\n',
    'scratchpad_assign = ""          # optional hotkey; legacy wire name: send foreground window to Special Workspace\nscratchpad_toggle = ""           # optional hotkey; legacy wire name: toggle Special Workspace / return desktop\n',
    "config field comments",
)
replace_once(
    "docs/CONFIG_SCHEMA.md",
    """`VdCfg` stores the numbered modifier family, optional move/follow and silent
modifier families, an optional previous-desktop hotkey, and optional scratchpad
hotkeys. Scratchpad HWND/ownership state is runtime-only and is never serialized.
Legacy schema-v6 `routing_rules` tables are accepted only at the TOML boundary,
""",
    """`VdCfg` stores the numbered modifier family, optional move/follow and silent
modifier families, an optional previous-desktop hotkey, and the two legacy-named
`scratchpad_*` hotkeys. Those wire names are retained for schema compatibility, but
the actions now send the foreground window to a dedicated Special Workspace and
toggle that Virtual Desktop. The workspace GUID and return-desktop GUID are
runtime-only and are never serialized. Legacy schema-v6 `routing_rules` tables are accepted only at the TOML boundary,
""",
    "config internal semantics",
)
replace_once(
    "docs/CONFIG_SCHEMA.md",
    "| virtual desktops | enabled, `win_number_switching` true, number family `Win`, move families/previous/scratchpad unassigned |",
    "| virtual desktops | enabled, `win_number_switching` true, number family `Win`, move families/previous/Special Workspace hotkeys unassigned |",
    "config defaults",
)
replace_once(
    "docs/CONFIG_SCHEMA.md",
    """v3 defaults for the four Phase-1 hotkeys, v4 defaults for the Virtual Desktop
workflow fields, v5 defaults for the scratchpad hotkeys, v7 defaults for
""",
    """v3 defaults for the four Phase-1 hotkeys, v4 defaults for the Virtual Desktop
workflow fields, v5 defaults for the legacy-named `scratchpad_*` hotkeys (now Special
Workspace actions), v7 defaults for
""",
    "config migration wording",
)

replace_once(
    "docs/ARCHITECTURE.md",
    """Native Windows tray utility: audio hotkeys, status overlay, virtual desktop
workflow, and a runtime scratchpad window.
""",
    """Native Windows tray utility: audio hotkeys, status overlay, virtual desktop
workflow, and a runtime dedicated Special Workspace.
""",
    "architecture intro",
)
replace_once(
    "docs/ARCHITECTURE.md",
    "└── focus history + scratchpad",
    "└── focus history + special-workspace identity",
    "architecture diagram",
)
replace_once(
    "docs/ARCHITECTURE.md",
    "| Desktop | STA | build-pinned Shell COM, public VirtualDesktopManager, stable desktop focus history, runtime scratchpad HWND |",
    "| Desktop | STA | build-pinned Shell COM, public VirtualDesktopManager, stable normal-desktop focus history, runtime Special Workspace GUID + return GUID |",
    "architecture ownership table",
)
replace_once(
    "docs/ARCHITECTURE.md",
    """* Scratchpad assignment is runtime-only in the desktop STA worker. WinShort separately tracks
  whether it hid the assigned HWND, treats an off-desktop DWM cloak as valid runtime state, and
  reveals WinShort-hidden windows before reassignment, feature disable, or orderly shutdown.
  Showing moves the window to the current desktop first; visibility restoration does not force
  `SW_RESTORE`, so normal/maximized placement is not rewritten by Scratchpad toggling.

Scratchpad identity is an HWND plus its owning PID. The PID is rechecked before
each mutating Win32 call, so a handle recycled into a different process is
cleared instead of being shown, hidden, or moved. Win32 provides no handle
generation token: same-process HWND reuse and a destruction/recreation race
after the final check remain residual limitations.
""",
    """* The Special Workspace is runtime-only in the desktop STA worker and is a real Windows
  Virtual Desktop with its own GUID. Sending a foreground window uses the documented
  `IVirtualDesktopManager::MoveWindowToDesktop(HWND, GUID)` path; toggling uses the build-pinned
  native Shell backend to switch between that GUID and a remembered normal desktop. No window is
  hidden, shown, restored, or force-focused as part of Special Workspace ownership.
* Numbered Desktop 1–9 operations filter the Special Workspace GUID out of Shell ordering, so the
  dedicated desktop never consumes a user-facing ordinal. Once it exists, keyboard-arrow fallback
  is not used for numbered operations because it cannot safely skip the extra Shell desktop.
* Disabling the feature or orderly shutdown removes the dedicated desktop with Shell's
  `RemoveDesktop`, supplying a normal fallback so Windows relocates contained windows. If WinShort
  crashes before cleanup, an unlabeled orphan desktop can remain; the next process deliberately
  does not guess which pre-existing desktop was its old workspace.
""",
    "architecture special workspace rules",
)

replace_once(
    "docs/WIN32_LIFETIME.md",
    "| Scratchpad HWND + ownership | desktop thread | `DesktopController::scratchpad` | PID-checked before every mutation; WinShort-hidden windows revealed before reassignment, disable, and orderly shutdown; stale identities are cleared |",
    "| Special Workspace GUID + return GUID | desktop thread | `DesktopController::{special_workspace,special_return}` | runtime-only; external deletion clears stale identity; disable/orderly shutdown removes the dedicated VD with a normal Shell fallback; hard crashes may leave an orphan VD that is not guessed/reclaimed |",
    "lifetime handle table",
)

replace_once(
    "docs/TEST_PLAN.md",
    """- Scratchpad assign → hide → show remains runtime-only and moves the window to the current desktop
- A WinShort-hidden Scratchpad is revealed before reassignment, feature disable, or orderly shutdown
- Scratchpad toggling preserves normal/maximized placement and reports foreground rejection truthfully
- A valid Scratchpad on another Virtual Desktop remains assigned even when DWM cloaks that window
""",
    """- Send foreground window → Special Workspace moves it off the current normal desktop without hiding it
- Multiple sent windows coexist on the same dedicated Special Workspace
- Toggle from a normal desktop → Special Workspace → toggle again returns to that exact normal desktop
- Focus another application before entering the Special Workspace; the real desktop switch exposes usable workspace windows without hide/show focus hacks
- Numbered Desktop 1..9 ordinals exclude the Special Workspace, including missing-normal-desktop creation
- Previous Desktop history is not polluted by entering/leaving the Special Workspace
- Disable the feature or exit cleanly removes the Special Workspace and Shell relocates its windows to a normal fallback desktop
- External deletion of the Special Workspace clears stale runtime identity and the next use creates a fresh workspace
- Unsupported/native-failed builds report Special Workspace unavailable; keyboard fallback never simulates its create/move/toggle semantics
- Hard process termination may leave an orphan dedicated VD; this residual limitation is documented and must not be reported as graceful-cleanup success
""",
    "test plan special workspace matrix",
)

replace_once(
    "CHANGELOG.md",
    """- Hardened Scratchpad ownership so WinShort-hidden windows are revealed before reassignment,
  feature disable, or shutdown; off-desktop cloaking stays valid and show/hide no longer forces
  `SW_RESTORE` over the application's existing placement.
""",
    """- Replaced hidden-window Scratchpad ownership with a dedicated native Virtual Desktop:
  Special Workspace actions no longer hide/show or force-focus application HWNDs, numbered 1–9
  excludes the workspace, and graceful disable/shutdown removes it through Shell with a normal fallback.
""",
    "changelog fixed",
)
replace_once(
    "CHANGELOG.md",
    """- Schema v5 adds optional scratchpad assignment and toggle hotkeys. The assigned
  window handle remains runtime-only and is cleared when the window closes.
""",
    """- Schema v5's optional `scratchpad_assign` / `scratchpad_toggle` wire names remain compatible;
  their current behavior sends windows to and toggles a runtime-only dedicated Special Workspace.
""",
    "changelog schema v5",
)
replace_once(
    "CHANGELOG.md",
    """- Numbered Virtual Desktop switching now ensures missing desktops through the
  native Shell backend; move/follow, silent move, previous-desktop navigation,
  per-desktop foreground restoration, and runtime scratchpad show/hide are available
  as conservative configurable actions.
""",
    """- Numbered Virtual Desktop switching now ensures missing normal desktops through the
  native Shell backend; move/follow, silent move, previous-desktop navigation,
  per-desktop foreground restoration, and the dedicated Special Workspace are available
  as conservative configurable actions.
""",
    "changelog virtual desktop feature",
)

print("special workspace documentation updated")
