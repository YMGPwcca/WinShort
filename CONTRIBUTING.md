# Contributing

## Prerequisites

- Windows 10 (build 26100-era layouts supported) or Windows 11
- Rust toolchain (MSRV 1.85 — enforced via `rust-version` and the MSRV CI job)
- MSVC build tools (link.exe + Windows SDK for `winresource`)

## Local gates (all required before submitting)

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
cargo check --target i686-pc-windows-msvc
```

CI (`ci.yml`) runs these plus the x86_64 release build with an embedded-manifest check, an
aarch64 compile check, an MSRV 1.85 job, and `cargo deny check`.

## Commit discipline

- One logical change per commit where practical.
- Bug fixes must include a regression test.
- Any change to `unsafe` Win32/COM code needs SAFETY reasoning in comments:
  ownership, thread affinity, lifetime of handles/pointers.
- Changes to the keyboard hook, audio engine, or lifecycle/shutdown ordering
  deserve extra scrutiny — these areas have subtle reentrancy/generation
  semantics documented inline (see #42/#45/#47/#48 history).

## Areas requiring extra care

- `src/keyboard/hook.rs` — LL callback must stay lock-free and allocation-free.
- `src/desktop/` — undocumented Shell COM ABI, pinned to build families.
- Shutdown ordering in `src/app.rs` / `src/main.rs`.
