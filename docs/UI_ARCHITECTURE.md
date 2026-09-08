# UI boundaries and ownership

This refactor is based on PR #103 at `43777bce4f05837b4aa1a6def44563a4357ba0d8`.
It preserves the existing native controls, layout, overlay material architecture,
interaction semantics and accessibility IDs. It does not introduce a new UI
framework; selectable blur treatments remain inside the existing Composition renderer.

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

`ElementId` remains the stable flat geometry, focus and UI Automation identity.
`ElementId::domain()` is the single exhaustive family classifier. Availability,
activation, visible values, page painting and accessibility enrichment route
through `ElementDomain` and then match only the narrower domain enum. This keeps
stable IDs without copying giant element-family lists into every consumer; adding
a new domain variant makes the affected domain handlers fail to compile until its
policy and presentation are considered.

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

`DisplaySession` owns the display editor phase, dirty flag and optional selected
route as one transaction state. A closed editor cannot be dirty, and an empty
profile has no selected route rather than a synthetic route zero. Output or
topology changes made through the existing picker enter the Review phase when
needed so a risky display draft always has an explicit Test/Keep/Discard path.

`InteractionState` owns short-lived pointer, close, confirmation and hotkey
capture state. `HotkeyCaptureState` makes the recording target and modifier mask
one transition instead of independent recording/armed booleans. Destructive
confirmation has one typed pending target, so Reset and Delete cannot both be
armed. First-run layout uses the finite `OnboardingStep` enum rather than numeric
step values.

Picker results cross the native/application boundary as one exhaustive
`PickerCommit`; picker kind and result value cannot disagree. Popup rows use an
internal `PickerChoiceValue` only while the native list is open. `PickerModel`
validates that every row belongs to the requested picker domain and represents
current selection as `Option<usize>`, so a missing configured value is not
silently displayed as row zero.

Device-cycle allowlists use `DeviceCycleSelection::{All, Disabled, Selected}` in
UI code. `Selected` can only be constructed with at least one endpoint. The
persisted `Option<Vec<String>>` encoding is translated at the configuration
boundary: `None` means All, an empty vector means Disabled, and a non-empty
vector means Selected. Presentation, layout and UI mutation consume the typed
state rather than interpreting those sentinels independently.

Display rollback presentation is derived from one semantic
`DisplayRollbackStatus` projection of the runtime snapshot. The application
boundary still supplies its existing transport fields; Control Center policy and
painting do not copy independent active/keep flags into `SettingsUi`.

`FocusState` atomically observes only the picker host/list HWNDs. The logical
picker owner is the existing focus target, while `ControlCenterWindow` alone owns
the `PickerPopup`. This prevents a second copied owner field from drifting out of
sync. Logical focus remains distinct from native ownership and the
keyboard-visible focus indicator. `SearchCaret` has either no timer deadline or
an active blink state. An empty or entirely disabled focus order produces `None`,
not an invalid index.

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
construction and report construction have instead been separated by purpose.
Geometry/rendering constants and accessibility identifiers retain their existing
values.

See [UI_REFACTOR_REPORT.md](UI_REFACTOR_REPORT.md) for the measured before/after
structure and the measurement's limitations. Windows regression and ABI tests
remain necessary; source metrics are not a substitute for runtime validation.
