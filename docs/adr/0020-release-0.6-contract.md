# ADR 0020 — linerule 0.6 release contract

- Date: 2026-07-25
- Status: accepted

## Decision

- Support Windows 11 x64 and ARM64.
- Start Off and restore only ruler preferences and nine shortcuts.
- Keep one hidden controller for single-instance, tray, settings, and shutdown;
  the click-through overlay only renders.
- Persist schema v1 atomically in MSIX LocalState or portable `data/`; preserve
  unknown future schemas and quarantine corrupt files.
- Expose only the core contracts and the Windows runtime facade. Keep Win32,
  WndProc, renderer, pointer, and FFI details private.
- Permit `unsafe` only below `platform-windows/src/win32_ffi/`, enforced by
  `cargo xtask policy`.
- Use demand-driven rendering. Recover through hardware recreation, WARP, then
  drawing-only degradation; keep tray and settings alive.
- Make pinned cargo-nextest the primary test runner. Keep ordinary parallel
  `cargo test --workspace --all-targets` as the isolation compatibility gate.
- Ship an x64/ARM64 MSIX bundle, App Installer file, per-architecture portable
  ZIPs, CycloneDX SBOM, source SPDX, checksums, signatures, and provenance.
- Gate release with `cargo xtask release-check`, native Windows jobs, coverage,
  mutation testing, REUSE 3.3, dependency review, and supply-chain checks.

## Documentation

The maintained user/developer documentation is limited to `README.md`,
`CONTRIBUTING.md`, `docs/TROUBLESHOOTING.md`, and `docs/RELEASE.md`. ADRs retain
decision history; generated diagrams and duplicate runbooks are not committed.
