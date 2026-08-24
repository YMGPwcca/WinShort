# Security Policy

## Supported versions

Only the latest tagged release receives security fixes. Older releases should
be upgraded.

## Reporting a vulnerability

Do **not** open a public GitHub issue for anything that describes an
exploitable behavior (e.g. global-hook input handling, privilege/boundary
issues in the audio or virtual-desktop integration).

Use GitHub's **Private vulnerability reporting** (Security tab → Report a
vulnerability) if it is enabled on this repository. If it is not available,
contact the repository owner through your GitHub account and reference
"WinShort security" — do not include exploit details publicly.

When reporting, please include:

- WinShort version (or commit SHA)
- Windows version/build + UBR
- Architecture
- Which subsystem: keyboard hook / audio / virtual desktop / tray / overlay
- Minimal description of the impact

Do **not** include raw keystroke captures or personal data.

## Response expectations

This is a personal project; there are no guaranteed SLAs. Reports are reviewed
on a best-effort basis and fixed in the next release where feasible.
