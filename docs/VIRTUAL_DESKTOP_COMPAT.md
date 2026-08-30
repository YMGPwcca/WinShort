# Virtual Desktop Compatibility

**Status: Implemented** (native backend on whitelisted builds; bounded fallback elsewhere).
The public `IVirtualDesktopManager` cannot switch desktops (spec §19). Absolute switching and
cross-process window movement use undocumented Shell COM interfaces whose layouts can change
between builds. This file records exactly what WinShort supports and why.

## Supported layout — Windows 11 24H2 / 25H2 (builds 26100–262xx)

| Item | Value |
|---|---|
| CLSID ImmersiveShell | `C2F03A33-21F5-47FA-B4BB-156362A2F239` (`CLSCTX_LOCAL_SERVER`) |
| QueryService service | `C5E0CDCA-7B6E-41B2-9FC4-D93975CC467B` |
| IID IVirtualDesktopManagerInternal | `53F5CA0B-158F-4124-900C-057158060B27` |
| IID IApplicationViewCollection | `1841C6D7-4F9D-42C0-AF41-8747538F10E5` |
| IID IApplicationView | `372E1D3B-38D3-42E4-A15B-8AB2B178F513` |
| IID IVirtualDesktop | `3F07F4BE-B107-441A-AF0F-39D82529072C` |
| IID IObjectArray | `92CA9DCD-5622-4BBA-A805-5E9F541BD8C9` |

`IServiceProvider` from ImmersiveShell → `QueryService(service, IID_IVirtualDesktopManagerInternal)`.
`IApplicationViewCollection` is acquired from the same provider with
`QueryService(IID_IApplicationViewCollection, IID_IApplicationViewCollection)`.

### IVirtualDesktopManagerInternal vtable (slots after IUnknown)

```
3  GetCount(UINT*)
4  MoveViewToDesktop(IApplicationView*, IVirtualDesktop*)
5  CanViewMoveDesktops(IApplicationView*, INT32*)
6  GetCurrentDesktop(IVirtualDesktop**)          // returns desktop DIRECTLY on 22621+ / 26100+ / 26200
7  GetDesktops(IObjectArray**)                   // elements are IVirtualDesktop via GetAt(i, IID_IVirtualDesktop)
8  GetAdjacentDesktop(IVirtualDesktop*, UINT, IVirtualDesktop**)  // HRESULT-preserving (PreserveSig)
9  SwitchDesktop(IVirtualDesktop*)
10 SwitchDesktopAndMoveForegroundView(IVirtualDesktop*)   // INSERTED in 26100+ — shifts everything below
11 CreateDesktop(IVirtualDesktop**)
12 MoveDesktop(IVirtualDesktop*, UINT nIndex)
13 RemoveDesktop(IVirtualDesktop* destroy, IVirtualDesktop* fallback)
14 FindDesktop(GUID*, IVirtualDesktop**)
   ... trailing slots unused by WinShort
```

### IVirtualDesktop vtable

```
3 IsViewVisible(IApplicationView*, BOOL*)
4 GetId(GUID*)
5 GetName(HSTRING*)
6 GetWallpaperPath(HSTRING*)
7 IsRemote(BOOL*)
```

WinShort calls `GetCount`, `GetDesktops`, `GetCurrentDesktop`, `GetId`, `SwitchDesktop`,
`CreateDesktop`, `CanViewMoveDesktops`, and `MoveViewToDesktop`. The documented
`IVirtualDesktopManager::GetWindowDesktopId` remains the cross-process reader. The documented
`MoveWindowToDesktop(HWND, GUID)` is deliberately not used for WinShort move actions because it
returns `E_ACCESSDENIED` for HWNDs owned by another process on the tested Windows 25H2 build.
Instead, WinShort resolves the HWND to an `IApplicationView` with
`IApplicationViewCollection::GetViewForHwnd`, checks `CanViewMoveDesktops`, and passes that exact
view pointer to the build-pinned `MoveViewToDesktop` slot.

## Critical caveat: IID reuse across a vtable change

Microsoft **reused** IID `53F5CA0B…` when inserting `SwitchDesktopAndMoveForegroundView` in
24H2. Late-23H2 (`22631.3085+`) answers the same IID with an *older vtable shape*. Therefore:

> **IID equality does not prove layout equality. Selection is pinned by OS build number,
> never by a successful QueryService alone.**

Whitelist for the layout above: build ∈ {26100, 26200..=26299}. Anything else → backend
unavailable, fail closed, keyboard fallback used. No probing unknown layouts "to see".

## Historical layouts (documented, NOT implemented — listed to justify fail-closed design)

| Builds | ManagerInternal IID | Notes |
|---|---|---|
| Win10 1809–22H2 | `F31574D6-B682-4CDC-BD56-F1827619833A` / alt `0F3A72B0…` (+SetName) | no monitor param |
| 20348–21999 | `094AFE11-44F2-4BA0-976F-29A97E263EE0` | monitor/HWND param era begins |
| 22000–22482 (21H2) | `B2F925B9-5A0F-4D2E-9F4D-2B1507593C10` | every method takes monitor first param |
| 22621 pre-3085 | `A3175F2D-239C-4BD2-8AA0-EEBA8B0B138E` | direct IVirtualDesktop returns |
| 22631.3085+ (23H2) | `53F5CA0B…` (old shape, slot 10 = CreateDesktop) | same IID as 24H2, different vtable |
| 26100+ (24H2/25H2) | `53F5CA0B…` + `SwitchDesktopAndMoveForegroundView` at slot 10 | implemented here |

## Sources & corroboration

1. **Ciantic/VirtualDesktopAccessor** (Rust, branch `rust`, pushed 2026-04): README states
   "requires at least 24H2 26100.2605, tested with 25H2 OS Build 26200.8117".
2. **ramensoftware/windhawk-mods `virtual-desktop-helper`** (u2x1): full per-era GUID table +
   raw vtable indices; tested on build 26200.8875.
3. **MScholtes/VirtualDesktop** v1.21 (2025-08): `VirtualDesktop11-24H2.cs` targeting 26100+.
4. **limyiheng gist** (plain C, 2024-10): same 53F5CA0B layout.
5. **WillyGarage/roost** engineering spike on build 26200.8875: independently observed
   cross-process `MoveWindowToDesktop` returning `E_ACCESSDENIED` and verified
   `GetViewForHwnd` → `CanViewMoveDesktops` → `MoveViewToDesktop` for cross-process moves.
6. **LGUG2Z/komorebi** Rust COM declarations: corroborates the stable
   `IApplicationViewCollection` IID and `GetViewForHwnd` vtable position.

Sources 1–2 independently confirm the exact build family this utility targets; sources 5–6
corroborate the cross-process application-view move path used by WinShort.

## Runtime behavior

* `desktop/detect.rs` reads the build number from registry (`CurrentBuildNumber`; UBR is read
  for display only and plays no role in allow/deny).
* Whitelisted build → native backend available; `SwitchDesktop(n)` resolves index n−1 through
  `GetDesktops` ordering and calls `SwitchDesktop`. A request equal to the current desktop is
  an early no-op.
* Native window move → `GetViewForHwnd` resolves the foreign HWND, `CanViewMoveDesktops` rejects
  pinned/unmovable views, then `MoveViewToDesktop` performs the move. No synthetic-input fallback
  exists for moving a window.
* Unknown build or native setup failure → status `UnsupportedBuild { build }`/`Failed`,
  diagnostics logged. Creation, window movement, and identity navigation refuse without
  synthetic input; switching may use the keyboard fallback only for a previously enumerated
  existing target after a transient Shell/RPC failure.
* Explorer restart kills the STA proxies (RPC-class error); the controller performs one inline
  proxy rebuild + retry, and if that fails it re-probes lazily once per subsequent command —
  never permanently disabled, never infinitely retried within one command.

## Error taxonomy and fallback policy

`DesktopError` variants (`desktop/backend.rs`), partitioned by `permits_fallback()`:

| Transient — may retry/fallback | Semantic — never fall back |
|---|---|
| `RpcDisconnected` (Explorer RPC gone) | `TargetOutOfRange { requested, count }` — target doesn't exist |
| `BackendUnavailable` (not activated / safety limit) | `UnsupportedBuild` — fail closed |
| | `AbiMismatch` — retained for typed matching; currently surfaced via BackendUnavailable |
| | `SwitchFailed(hr)` — Shell rejected a switch operation |
| | `CreationUnavailable`, `MoveUnavailable`, `WindowUnavailable`, `FocusFailed` |
| | `Partial` — a later step failed after an earlier step completed |

Move-operation HRESULTs use a move-specific classifier: Shell/RPC disconnects remain retryable,
while ordinary failures retain their API context inside `MoveUnavailable` rather than being
misreported as `SwitchDesktop failed`.

The exact partition is proptested (`policy_props.rs`, #37). `index >= count` refuses **before**
any input injection (#20): a too-large target never triggers SendInput keys.

## Fallback backend guarantees

`KeyboardFallback::switch_to` (src/desktop/keyboard_fallback.rs):

* Saturate-left then move-right: exactly **32 `Ctrl+Win+Left` chords**, then `index`
  `Ctrl+Win+Right` chords. No current-index tracking — the walk self-corrects every invocation.
* Per-chord order: Ctrl down → LWin down (only if Win isn't already physically held) → extended
  Arrow down/up → LWin up (if injected) → Ctrl up. Every event carries a `dwExtraInfo` tag so
  WinShort can identify its own injections downstream.
* Timing: ~18 ms pause per Left chord, ~28 ms per Right chord.
* Guard: targets above index 31 are refused (`TargetOutOfRange`) without injecting anything.
* Partial `SendInput`: remaining chords abort immediately, best-effort key-up cleanup is sent
  for any key possibly left down, error reports "target not verified". An elevated foreground
  window blocks synthesized input entirely (UIPI); the log names this possibility.
* The fallback cannot enumerate: `desktop_count()`/`current_desktop()` report unavailable.

## Status surface

Settings → Advanced shows one composed read-only row ("Virtual desktop engine"): active
backend label, desktop count when known, native availability/reason (`Unsupported build N`),
and last served. `last_served` (#21/#22) records the backend that actually completed the most
recent switch — native success updates it even after a rebuild; a failed fallback walk leaves
it unchanged.

## Tested results (this machine)

| Check | Result |
|---|---|
| Build | 26200.9168 (25H2), whitelisted |
| `GetDesktops` count | 9 desktops enumerated |
| `GetCurrentDesktop` | index resolves via GUID match |
| `SwitchDesktop` 1↔2 | registry `CurrentVirtualDesktop` GUID changed both ways |
| Startup log | `virtual desktop backend: Native Shell`, `count=9, current=1` |
| Win+1..9 routing | binding table → `SwitchDesktop(n)` → Native Shell |
| Cross-process Special Workspace move | implementation corrected to application-view path; real-Windows re-test pending |
| Fallback path | compiled in; active only when native setup fails (fail closed) |

Original switching checks were tested 2026-08-24. Cross-process Special Workspace movement was
corrected after manual `E_ACCESSDENIED` acceptance failure on 2026-08-31 and remains pending
real-Windows re-verification after this fix.
