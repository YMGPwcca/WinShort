# UI refactor: structural report

Baseline: PR #103, `43777bce4f05837b4aa1a6def44563a4357ba0d8`.

These measurements describe `src/ui`, not third-party code or generated Windows bindings.
Line counts include comments, whitespace and explicit imports. Test files and `#[cfg(test)]` blocks
are excluded from production-function counts; moved tests are not removed functionality.

| Measure | Before | Final candidate |
|---|---:|---:|
| Rust source files (including grouped tests) | 15 | 178 |
| Largest production module, lines | 6251 | 550 |
| Largest production function, lines | 504 | 257 |
| Production functions over 200 lines | 8 | 3 |

The first extraction pass produced 182 Rust files. A final anti-fragmentation review merged caret
state into search, display-inventory state into display inventory, Control Center close-state helpers
into local state, and navigation actions into navigation. Tiny modules that still remain represent a
real boundary (for example native message families, COM ABI/provider concerns, or renderer resource
ownership), rather than a target file-size quota.

This is not a source-line reduction exercise. Explicit imports, invariant-bearing types, RAII owners,
unit-test grouping and safety/error contracts intentionally add some lines where they shorten the
reasoning needed to understand a change.

## Selected operation boundaries

| Operation | Lines before / final entry point | Decision proxy before / final entry point |
|---|---:|---:|
| `settings_wndproc` | 504 / 103 | 94 / 33 |
| `activate` | 395 / 13 | 106 / 3 |
| activation routing (`dispatch_activation`) | n/a / 97 | n/a / 10 |
| `draw_page` | 323 / 13 | 51 / 6 |
| element paint routing (`draw_element`) | n/a / 75 | n/a / 12 |
| `publish_automation_snapshot` | 203 / 15 | n/a / 2 |
| `add_display_wizard` | 211 / 23 | n/a / 5 |
| Composition host `create` | 195 / 27 | n/a / 8 |
| diagnostics `lines` | 503 / 12 | 34 / 1 |
| `is_disabled` | 181 / 158 | 70 / 56 |
| `value_for` | 225 / 257 | 79 / 91 |

Decision proxy is a source-structural aid: one plus `if`, loop, match-arm, `?`, `&&` and `||`
occurrences in the function body. It is **not** a certified McCabe score or proof of an optimal
design. Match arms count even in simple total lookup tables. The refactor therefore reduces
complexity where a function mixed operations, I/O or ownership, but does not hide declarative
complexity behind registries or dynamic dispatch.

The three remaining production functions over 200 lines are explicit projections/drawing tables:
`value_for`, vector icon drawing, and picker-choice construction. `value_for` also grew when partial
hotkey conversion was replaced by typed, exhaustive mappings. Splitting those tables into indirect
registries solely to improve a number would make behavior harder to audit and weaken exhaustiveness.

## Concrete design changes

- Atomic picker/focus binding, caret timing state and display-inventory outcome.
- Shared named display presentation and readiness used by painting and accessibility.
- Explicit configuration I/O and constructor inputs, without a service/container framework.
- Validated route-edit model, nonzero dimensions/rates, typed rotation and tray commands.
- Activation, page painting, accessibility enrichment, display-wizard construction and Composition
  setup are routed through small domain operations instead of monolithic functions.
- Total renderer resource sets instead of maps whose callers must `expect` an entry.
- Constructor rollback, paint pairing and GDI/font/menu ownership on error paths.
- Nullable COM ABI/provider lifetime kept isolated and initialization made fallible.
- Event handlers and report sections extracted without extending state borrow scopes.
- Final anti-overengineering pass removed four one-purpose split modules that did not improve a
  domain boundary.

## Verification boundary

The final source tree must pass the repository's Windows CI: formatting, strict Clippy, existing
regression tests, release build and nullable-provider ABI regression, x86/ARM64 type checks, declared
MSRV and dependency checks. Hosted build/test results are not a claim that a human interactive desktop
visual review was performed. Layout and overlay-material parameters were not retuned. No runtime crate
or Cargo feature was added. Temporary refactor transport/workflow files are not part of the handoff.
