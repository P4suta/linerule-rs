# ADR-0004: Coverage Policy

## Status

Accepted (2026-05-20).

## Context

`linerule-platform-windows` is gated by `#![cfg(windows)]` at its crate root, so the entire crate is empty (uncompiled) on a non-Windows host. `cargo llvm-cov` on a Linux runner therefore cannot cover it, including the small `pub(crate)` helpers (`composition_renderer::decompose`, `messages::*`, `error::decode_last_error`, `overlay_state`) living under the same gate.

## Decision

The required `coverage` gate measures **`linerule-core` + `linerule-app` only**.

- **Linux job (`coverage`)**:
  - `cargo llvm-cov nextest --workspace --exclude linerule-platform-windows --exclude xtask --fail-under-lines <threshold>`
  - Threshold `80`, raised to `85` once stable.
  - `xtask` excluded: build-time helper run via `cargo xtask`, not exercised by `cargo nextest`.
- **Windows job (`coverage-windows`, future)**:
  - `cargo llvm-cov nextest -p linerule-platform-windows`, uploads HTML/LCOV artifact.
  - **Does not gate merges.** Thin FFI veneer over `windows` crate; testing COM call paths needs a live `HWND`/D3D11 device, kept honest by `examples/overlay_smoke.rs` and boundary unit tests (constants, atomic counters, error decode). COM call-path coverage is out of scope.

Mutation testing (`cargo-mutants`) runs only against `linerule-core`.

## Consequences

- The PR coverage check is stable: only Linux-runnable code, no Windows-host dependency.
- Windows-only modules appear in the artifact (regressions in `decompose`/`overlay_state` visible) but cannot block a PR. Windows-surface signal comes instead from `cargo xwin check`, the `release build (win-x64, native)` job, and the Windows native smoke.
- Prefer promoting pure data-mapping Win32 helpers into `linerule-core` (as done for `chord_to_win32`/`key_to_vk`) to grow Windows-side coverage without widening the gate.

## Links

- ADR-0002 §2 (core stays pure).
- ADR-0003 (unsafe isolation to `win32_ffi/`).
