# ADR-0004: Coverage Policy

## Status

Accepted (Phase 0 of the testing-rebuild plan, 2026-05-20).

## Context

`linerule-rs` consists of three runtime crates:

- `linerule-core` — pure ADTs / reducers / parsers, `#![forbid(unsafe_code)]`, no platform dependencies.
- `linerule-app` — single-binary entry point (`linerule.exe`), CLI dispatch, logging boot, crash-dump install.
- `linerule-platform-windows` — Win32 / COM layer, gated by `#![cfg(windows)]` at the crate root (`crates/linerule-platform-windows/src/lib.rs:24`).

The `cfg(windows)` gate means the entire `linerule-platform-windows` crate is empty when built on a non-Windows host. That has two consequences for `cargo llvm-cov`:

1. Running `cargo llvm-cov` on a Linux runner cannot produce coverage for `linerule-platform-windows`. The crate's source files are not compiled at all under `cfg(unix)`.
2. The Win32 surface that *can* be tested in isolation (small `pub(crate)` helpers — `composition_renderer::decompose`, `messages::*` constants, `error::decode_last_error`, `overlay_state` counters) lives under that same gate. Their coverage shows up only when measured under `cargo llvm-cov` on a Windows runner.

We previously left `coverage` as an advisory job with no threshold. The plan moves it to a required check (P2-3), and that move only makes sense once the scope is explicit.

## Decision

The required `coverage` gate measures **`linerule-core` + `linerule-app` only**.

- **Linux job (`coverage`)**:
  - Command: `cargo llvm-cov nextest --workspace --exclude linerule-platform-windows --fail-under-lines <threshold>`
  - Threshold: starts at `80` after Phase 1 lands, raised to `85` once Phase 2 stabilizes.
- **Windows job (`coverage-windows`, future)**:
  - Command: `cargo llvm-cov nextest -p linerule-platform-windows`
  - Uploads an HTML / LCOV artifact for inspection.
  - **Does not gate merges.** The Windows-only surface is a thin FFI veneer over `windows` crate. Tests of those FFI calls require a live `HWND` / D3D11 device; we keep this surface honest with the `examples/overlay_smoke.rs` smoke and with the boundary unit tests in P1-6 (constants, atomic counters, error decode). Coverage of the COM call paths themselves is intentionally out of scope.

The same scope split applies to mutation testing: `cargo-mutants` runs only against `linerule-core`.

## Consequences

- The PR coverage check is enforceable and stable: it measures code that runs on every Linux runner with no Windows-host dependency.
- Windows-only modules show up in the artifact (so regressions in `decompose` / `overlay_state` are visible) but cannot block a PR. That is appropriate because they are second-order: the integration-grade signal for the Windows surface comes from `cargo xwin check` (build invariant), the `release build (win-x64, native)` job (link invariant), and the Windows native smoke (P2-4, `linerule.exe version` exit 0).
- The "pure helper exists in `linerule-core` and is unit-tested" pattern (P0-2 moved `chord_to_win32` / `key_to_vk` into core for exactly this reason) is the way to grow Windows-side coverage without growing the gate's blast radius. Any new Win32 helper that is purely data-mapping should be considered for promotion to `linerule-core` first.

## Links

- ADR-0002 §2 (crate invariants — core stays pure).
- ADR-0003 (unsafe isolation to `win32_ffi/`).
- Testing rebuild plan: `/home/yasunobu/.claude/plans/linerule-cs-c-linerule-rust-windows-c-cs-starry-taco.md`.
