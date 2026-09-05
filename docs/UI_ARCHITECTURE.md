# UI boundaries and ownership

This refactor is based on PR #103 at `43777bce4f05837b4aa1a6def44563a4357ba0d8`.
It preserves the existing native controls, layout, overlay material, interaction
semantics and accessibility IDs. It does not introduce a new UI framework or
change the blur style.

## Direction of dependencies

The application supplies configuration access and runtime snapshots to the
Control Center. Input adapters translate native messages into operations. Pure
layout and presentation code supply values to stateless painters and immutable
accessibility snapshots. COM providers read those snapshots and queue actions;
they do not reach through to window-owned `SettingsUi`.

- `ui/control_center.rs`: the feature facade. Its private modules separate local
  state, draft transactions, commands, domain presentation and native dispatch.
- `ui/layout.rs`: geometry, element identifiers and page builders. A layout is
  shared by rendering, hit testing, keyboard traversal and accessibility; there
  is not a second independently maintained accessibility layout.
- `ui/controls.rs`: stateless control families. Painters borrow their input and
  do not load configuration or perform application commands.
- `ui/renderer.rs`: target ownership, complete brush sets and text formats.
- `ui/overlay.rs`: status model, pure timing, placement, window lifetime,
  foreground drawing and rendering backends. The Graphics Effects ABI remains
  isolated from the status model and timing policy.
- `ui/control_center_automation.rs`: snapshot model, semantic capability
  classification, lifetime/queues, COM providers and the nullable ABI adapter.
- `ui/picker.rs`, `ui/prompt.rs`, `ui/diagnostics.rs`: separate native windows.
  Picker selection, diagnostic report sections and native lifetime are not
  combined with the Control Center's command implementation.
- `tray`: native notification-area registration, typed menu selection and icon
  rasterization. The GDI conversion boundary owns its intermediate resources.

Small cohesive modules such as animation, theme, navigation and first-run policy
remain together. The final anti-fragmentation pass also folded search-caret state
into search, display-inventory state into display inventory, close-state helpers
into Control Center state, and navigation actions into navigation. A facade
aggregates a feature's public entry points; it does not make implementation
modules public. Production imports are explicit. Test fixtures are feature-local
and grouped by the behavior under test.

## Invariants in the data model

`DisplayInventory` distinguishes an unqueried inventory, a complete successful
query and a failed query. Stale outputs and a failure cannot coexist. A profile's
`ProfileReadiness` is computed once from that inventory and its saved routes.
Named card presentation values feed both painters and accessibility labels;
positional tuples of strings and unrelated flags are no longer the contract.

`FocusState` installs or removes a picker owner/window/list binding atomically.
Logical focus is distinct from native ownership and the keyboard-visible focus
indicator. `SearchCaret` has either no timer deadline or an active blink state.
An empty or entirely disabled focus order produces `None`, not an invalid index.

`DisplayRouteEdit` is the validated result of parsing editor text. Its dimensions
and rational refresh components are nonzero, and its rotation is an enum. Only
`apply_to` converts it back to the existing persisted DisplayConfig DTO. Unknown
choice indices and native tray command values do not silently select an option.

Config-backed toggles use one classification for visible values, mutations and
animation; the deliberately inverted Pause Shortcuts label is represented
explicitly. `ConfigAccess` supplies only snapshot/read and commit capabilities.
The owning application wires those operations. Window construction and fixtures
pass initial configuration explicitly instead of having the state constructor
read an application singleton or the filesystem.

## Native lifetime and reentrancy

`WindowCreation<T>` owns state until the synchronous `WM_NCCREATE` callback takes
it. Failure before that callback drops the state normally. A
`WindowConstructionGuard` owns the new HWND during the rest of a fallible
constructor and destroys the partially created window and its children on error.
Successful construction transfers ownership to the existing window owner.
`GWLP_USERDATA` is interpreted as its actual `WindowState<T>` allocation, not an
assumed layout-compatible field pointer.

`PaintSession` pairs BeginPaint/EndPaint even on early returns. Picker fonts,
tray menus, temporary GDI bitmaps and acquired screen DCs have owners whose Drop
implementations release only their own handles. A failed version negotiation
rolls back notification-icon registration, both initially and after Explorer
restarts. Fallible class/brush/provider construction reports errors rather than
panicking inside a native callback.

Reentrancy still matters on one thread. Calls such as ShowWindow, SetWindowPos,
SetFocus and DestroyWindow can synchronously cause more window messages. The
existing prepare-plan/apply pattern and picker snapshot handoff are retained:
release a state borrow before performing native operations that can reenter.
The `RefCell` boundary is retained where this models native callback ownership;
it is not replaced by a blocking mutex. UIA retains Arc and short-lived mutexes
where COM consumers genuinely run on other threads. No async executor or new
background service is introduced.

The nullable UIA bridge preserves successful null COM returns rather than
converting them into Rust non-null interfaces. Provider initialization now caches
a complete identity or an HRESULT; it never publishes a partially initialized
provider. Existing debug and release ABI regressions remain part of verification.

## Scope and tradeoffs

This is not a rewrite of the audio, display, keyboard or app event subsystem.
Their process-level coordination remains at the volatile application/native
edge. Introducing traits for every function or replacing every config value
with a new type would broaden scope without clarifying a boundary.

Exhaustive element-to-label, element-to-command, picker-choice, vector-icon and
SDK property-ID tables remain explicit. They naturally have many arms; replacing
them with indirect registries just to lower a complexity number would obscure
behavior. Large operation bodies, repeated configuration mutations, native
dispatch, accessibility enrichment, Composition setup, page painting, wizard
construction and report construction have instead been separated by purpose. Geometry/rendering constants
and accessibility identifiers retain their existing values.

See [UI_REFACTOR_REPORT.md](UI_REFACTOR_REPORT.md) for the measured before/after
structure and the measurement's limitations. Windows regression and ABI tests
remain necessary; source metrics are not a substitute for runtime validation.
