# WinShort Roadmap

**Status:** canonical project work queue.  
**Last audited:** 2026-09-01 against `main` at `a2fb594` (`chore(deps): bump toml to 1.1.4 (#51)`).

This file answers **what should be worked on next**. `docs/TEST_PLAN.md` remains the source of truth for how behavior is verified; architecture/design documents remain authoritative for their own invariants.

## Ground rules

- Do not describe automated/unit coverage as live Windows, accessibility, or hardware evidence.
- Preserve the native Rust + Win32/COM + Direct2D/DirectWrite architecture. No Electron/WebView/framework rewrite.
- Keep thread/COM ownership and UIA deferred-delivery invariants intact.
- Do not resurrect removed Scratchpad hidden-window behavior. The supported model is the dedicated Special Workspace.
- `DeviceSelection::Default` / `"default"` is a binding mode that follows the real Windows default endpoint, not a fake endpoint.
- Rust **1.85 is the intentional MSRV** until the project explicitly decides to raise it.
- Do not merge stale/WIP branches merely because they contain commits not reachable from `main`; compare behavior/tree first.

## Current implemented baseline

The following is implemented on `main` and is maintenance/release-validation work, not an unfinished feature list:

- Native Control Center: Home, Shortcuts, Audio, Workspaces, Displays, Overlay, System/Advanced, search, onboarding, theme/high-contrast handling, keyboard focus, and custom UI Automation provider.
- Global hotkeys with capture/conflict validation, AltGr handling, Win-number desktop switching, and event-driven runtime updates.
- Audio mute controls, foreground-app mute/volume, actual Windows default input/output switching, and independent cycling allowlists.
- Numbered virtual desktops, move/follow, silent move, Previous Desktop, and the dedicated recoverable Special Workspace.
- Display profiles with stable route/profile identity, capture/edit/rename/duplicate/delete, per-profile hotkeys, Test Apply, Keep/Revert, and 15-second rollback.
- Monitor-aware status overlay with draft preview, appearance/position/scale/opacity/duration controls, including the full 3x3 position set.
- Diagnostics/logging/support bundle, startup control, single-instance/tray lifecycle, and sanitized support export.
- Hosted CI gates for formatting, clippy, tests, x86_64 release build + manifest/icon + release UIA ABI regression, i686/aarch64 checks, Rust 1.85 MSRV, and cargo-deny.
- GitHub Actions checkout/artifact pipeline updated to the Node 24-era majors and validated end-to-end through release packaging/checksum aggregation.
- `toml` 1.1.4 is the current config parser/serializer dependency and passes the full config-heavy CI suite at Rust 1.85.

There are currently **no open product issues**. The only remaining open dependency PR is the deferred tag-publish action upgrade (#53).

## P0 — Release acceptance on real Windows

Goal: turn implemented behavior into current release evidence. Use the exact procedures in `docs/TEST_PLAN.md`; record Windows build/hardware and do not substitute unit tests for manual results.

1. **Control Center / accessibility / DPI**
   - Narrator or Accessibility Insights navigation and semantics.
   - Tab/Shift-Tab, picker keyboard behavior, recorder states, focus repair, disabled-help behavior.
   - 100/125/150/200% DPI, negative coordinates, monitor removal/reconnect.
   - System/Light/Dark and High Contrast, visible focus, disconnected-device presentation.

2. **Audio hardware/runtime**
   - Default input/output changes while running.
   - Device unplug/replug and Windows Audio service restart.
   - Next microphone / Next speaker changes all three Windows default roles and respects allowlists/offline IDs.
   - Foreground-app mute/volume across real multi-session applications and no-session cases.

3. **Virtual Desktop / Special Workspace**
   - Native backend validation on supported Windows build families documented in `VIRTUAL_DESKTOP_COMPAT.md`.
   - Numbered desktop creation/reorder/delete behavior, Previous Desktop, move/follow and silent move.
   - Special Workspace multi-window use, exact return desktop, hard-kill/reboot GUID recovery, external deletion, clean removal.
   - Unsupported build must fail closed for native-only operations.

4. **Display profiles**
   - Single- and multi-monitor capture, clone/extend/custom routes, same-panel/different-connector cases.
   - Test Apply → Keep, explicit Revert, timeout rollback, and rollback-failure blocker.
   - Hotkeys, rename/duplicate/delete identity behavior, monitor unplug/replug, and unresolved-route refusal.

5. **Overlay / tray / lifecycle**
   - Fullscreen focus safety, monitor/DPI movement, rapid coalesced updates, animation/reduced-motion settings.
   - Explorer restart/tray recreation, second-instance activation, repeated start/exit, lock/unlock, sleep/resume, idle CPU.

**P0 exit condition:** release-target hardware/manual matrix is recorded with no unresolved blocker. Items not exercised stay explicitly `NOT VERIFIED`.

## P1 — Dependency and CI maintenance

Major upgrades are accepted only with evidence for the paths they actually affect.

| PR | Change | Disposition / evidence |
|---|---|---|
| #52 / #99 | `dtolnay/rust-toolchain` MSRV pin | **Resolved.** #52 was closed because bumping the CI pin would remove the Rust 1.85 MSRV gate. #99 makes Dependabot ignore this intentional pin. |
| #55 / #56 / #54 → #101 | checkout 4→7, upload-artifact 4→7, download-artifact 4→8 | **Merged as #101.** Temporary #100 exercised the real release workflow: both Windows matrix builds/tests/packages passed; upload v7 passed; download v8 passed with digest mismatch = error; checksums were generated; downloaded release ZIPs contained exactly `winshort.exe`, `README.md`, and `LICENSE`. Normal #101 CI also passed 8/8. |
| #51 | `toml` 0.9 → 1.1.4 | **Merged.** Fresh post-#101 CI passed 8/8, including Rust 1.85 MSRV, the config-heavy test suite, cross-target checks, release build, and UIA ABI regression. |
| #94 | `zip` 6 → 8.6 | **Rejected / major ignored.** Fresh CI proved `zip@8.6.0 requires rustc 1.88`, incompatible with the intentional Rust 1.85 MSRV. |
| #53 | `softprops/action-gh-release` 2 → 3 | **Deferred.** This action runs only on the tag-publish path. Normal CI and safe release dry-runs do not execute it, so v3 needs a deliberately disposable publish test or controlled real release before merge. |

**P1 remaining work:** #53 only. Do not treat a normal green PR CI run as proof of tag-publish compatibility.

## P2 — Release/CI hardening

- Keep the validated release dry-run path working for both x86_64 and i686 packaging/checksums after future workflow changes.
- Screenshot-driven UI QA remains **planned, not implemented**; do not claim a screenshot automation gate exists.
- Authenticode signing remains an explicit future hook unless signing secrets/certificates are actually configured and validated.
- aarch64 remains compile-check-only until a real artifact/support decision is made.
- Consider protecting `main` with required CI checks after the maintenance flow is stable; repository policy should prevent accidental direct history mutation without blocking intentional maintainer recovery.
- Reconcile the `workflow_dispatch.publish` input before relying on it for manual publishing: the current publish step is intentionally/actually tag-gated, so a dispatch dry-run does not exercise release creation.

## P3 — Repository hygiene

Existing non-Dependabot branches must be audited before deletion:

- merged/fix branches such as `fix/ci-onboarding-test-fixture`, `fix/scratchpad-scope-cleanup`, and `fix/display-profile-ux`;
- obsolete Scratchpad WIP branches;
- `wip/special-workspace-build` intermediate history;
- temporary maintenance/documentation branches after their PRs merge.

Deletion is a separate maintenance action: compare each branch against `main`, preserve anything uniquely useful, then delete only with explicit authorization.

## How to update this roadmap

When a task is completed:

1. merge the implementation/evidence into `main`;
2. update the relevant design/test document if its contract changed;
3. move or remove the roadmap item in the same cleanup cycle;
4. keep evidence labels honest: **AUTOMATED / HOSTED CI**, **AUTOMATED / LOCAL**, **MANUAL / WINDOWS**, **MANUAL / HARDWARE**, or **NOT VERIFIED**.
