# Contributing

## Prerequisites

- Windows 10 (build 26100-era layouts supported) or Windows 11
- Rust toolchain (MSRV 1.85 — enforced via `rust-version` and the local final-validation gate)
- MSVC build tools (link.exe + Windows SDK for `winresource`)
- `cargo-deny` for dependency/security policy validation
- Installed Rust targets `i686-pc-windows-msvc` and `aarch64-pc-windows-msvc` for the full local gate

## Local gates (all required before merge/release)

The canonical full gate is:

```powershell
.\tools\final_validation.ps1
```

It verifies formatting, all-target/all-feature compilation, strict Clippy, tests, the x86_64
release build, embedded manifest/icon resources, the nullable UIA provider ABI regression,
i686/aarch64 compile checks, Rust 1.85 MSRV compatibility, `cargo deny check`, Windows UI
acceptance, and the visual-sanity capture. The generated visual sheet still requires human review.

For a compiler/security-only rerun while diagnosing a failure:

```powershell
.\tools\final_validation.ps1 -SkipUiReview
```

GitHub-hosted push/PR CI is intentionally not used. Release automation remains tag/manual only;
a final local gate must be completed before merge/release rather than delegated to hosted runners.

## Commit discipline

- One logical change per commit where practical.
- Bug fixes must include a regression test when the behavior can be exercised deterministically.
- Any change to `unsafe` Win32/COM code needs SAFETY reasoning in comments:
  ownership, thread affinity, lifetime of handles/pointers.
- Changes to the keyboard hook, audio engine, or lifecycle/shutdown ordering
  deserve extra scrutiny — these areas have subtle reentrancy/generation
  semantics documented inline (see #42/#45/#47/#48 history).

## Areas requiring extra care

- `src/keyboard/hook.rs` — LL callback must stay lock-free and allocation-free.
- `src/desktop/` — undocumented Shell COM ABI, pinned to build families.
- Shutdown ordering in `src/app.rs` / `src/main.rs`.