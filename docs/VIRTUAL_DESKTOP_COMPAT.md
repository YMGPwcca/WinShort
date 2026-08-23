# Virtual Desktop Compatibility

The public `IVirtualDesktopManager` cannot switch desktops (spec §19). Absolute switching uses
undocumented Shell COM interfaces whose layout changes between builds. This file records exactly
what WinShort supports and why.

## Supported layout — Windows 11 24H2 / 25H2 (builds 26100–262xx)

| Item | Value |
|---|---|
| CLSID ImmersiveShell | `C2F03A33-21F5-47FA-B4BB-156362A2F239` (`CLSCTX_LOCAL_SERVER`) |
| QueryService service | `C5E0CDCA-7B6E-41B2-9FC4-D93975CC467B` |
| IID IVirtualDesktopManagerInternal | `53F5CA0B-158F-4124-900C-057158060B27` |
| IID IVirtualDesktop | `3F07F4BE-B107-441A-AF0F-39D82529072C` |
| IID IObjectArray | `92CA9DCD-5622-4BBA-A805-5E9F541BD8C9` |

`IServiceProvider` from ImmersiveShell → `QueryService(service, IID_IVirtualDesktopManagerInternal)`.

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

WinShort calls only: `GetCount`, `GetDesktops`, `GetCurrentDesktop`, `SwitchDesktop`,
`GetId`. Nothing else — smaller surface, fewer breakage points.

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

Sources 1–2 independently confirm the exact build this utility targets (26200).

## Runtime behavior

* `desktop/internal_api/detect.rs` reads the build number (registry `CurrentBuildNumber`).
* Whitelisted build → internal backend available; `SwitchDesktop(n)` resolves index n−1 through
  `GetDesktops` ordering and calls `SwitchDesktop`.
* Unknown build or any COM failure during setup → status becomes `Unsupported { build }`,
  diagnostics logged with reason, engine transparently routes switches to the keyboard fallback.
* Explorer restart kills the STA proxies (`RPC_S_SERVER_UNAVAILABLE`); the desktop thread rebuilds
  the provider chain lazily on next command instead of failing permanently.

## Fallback backend guarantees

The SendInput fallback synthesizes `Ctrl+Win+Left/Right` repeatedly from the current desktop to
reach the target index. It requires knowing the current index, which it tracks from observed
switch results — it is **best effort**: if another app or the user moves desktops without our
knowledge, relative tracking can drift until corrected by a successful native-backend read or a
user-visible correction. The settings Advanced page shows which backend is active and why
(spec §21 wording).

## Tested results (this machine)

| Check | Result |
|---|---|
| Build | 26200.9168 (25H2), whitelisted |
| `GetDesktops` count | 9 desktops enumerated |
| `GetCurrentDesktop` | index resolves via GUID match |
| `SwitchDesktop` 1↔2 | registry `CurrentVirtualDesktop` GUID changed both ways |
| Startup log | `virtual desktop backend: Native Shell`, `count=9, current=1` |
| Win+1..9 routing | binding table → `SwitchDesktop(n)` → Native Shell |
| Fallback path | compiled in; active only when native setup fails (fail closed) |

Tested 2026-08-24 against the layout above; re-verify after any Windows update.
