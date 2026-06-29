# ADR 0002 — Architecture Principles

- Date: 2026-05-20
- Status: accepted
- Proposer: P4suta

## Context

Architectural beauty is the top priority. This ADR decomposes "beauty" into mechanically verifiable principles.

## Decision

### Principle 1 — One-way dependency

Dependencies flow one way only: `linerule-app → linerule-platform-windows → linerule-core`. Reverse and peer-to-peer dependencies are forbidden.

Verification: `cargo xtask dep-graph` parses `cargo metadata` to detect violations. Required CI gate.

### Principle 2 — Per-crate invariants

| Crate | Role | Crate attributes | Must not have |
|---|---|---|---|
| `linerule-core` | Pure logic, ADTs, reducer, parser | `#![forbid(unsafe_code)]` | Side effects, the `windows` crate, `std::env`/`std::fs`, global state |
| `linerule-platform-windows` | Win32 / COM implementation | `#![cfg(windows)] #![deny(unsafe_op_in_unsafe_fn)]` | Domain logic (reducer/render only called from core) |
| `linerule-app` | Wiring and entry point | `#![forbid(unsafe_code)]` (local exception for `console` only) | Domain logic |
| `xtask` | Build automation | `#![forbid(unsafe_code)]` | Impact on production builds |

Non-determinism such as `std::time::Instant` is passed into `linerule-core` functions as arguments from outside; core performs no global access whatsoever.

### Principle 3 — Defer abstraction

Do not create traits / generics preemptively. Promote to an abstraction only **after at least 2 implementations appear**. Do not build port-and-adapter traits; write test mocks as a separate `cfg(test)` implementation or via closure injection.

### Principle 4 — Strict RAII

COM objects / HWND / Hook / Hotkey / JoinHandle are all released reliably via `Drop`. Production use of manual `ComLifetime` Release, `ManuallyDrop`, `std::mem::forget`, and `Box::leak` is forbidden.

### Principle 5 — Exhaustive match

State transitions and command handling rely on `match` exhaustiveness. Do not use `_ => …` in production code (allowed only in test fixtures). Keep a state where the compiler detects non-exhaustiveness when a new case is added.

Verification: the `no-wildcard-match` rule of `xtask strict-code`.

### Principle 6 — Natural `Result + ?` flow

Do not introduce custom monad / Kleisli machinery such as `BootDag<Phase<TIn,TOut>>`. Compiler-enforced ordering (line order in function bodies) is sufficient. Structure error types with `thiserror::Error`.

### Principle 7 — Localized `unsafe`

Keep `unsafe` blocks as narrow as possible and state the invariants in a comment immediately before the block. `#![deny(unsafe_op_in_unsafe_fn)]` requires this even inside an `unsafe fn`.

A file-wide `#![allow(unsafe_code)]` at the top of a file is forbidden (detected by `no-file-wide-unsafe-allow` of `xtask strict-code`). Only `linerule-platform-windows` may allow `#![allow(unsafe_code)]` as a crate attribute, and only directly below `#![cfg(windows)]`.

### Principle 8 — Data-driven + unidirectional

`OverlayAction → State + StateDelta → OverlayFrame → render` is unidirectional. State mutation is concentrated at the single point `StateDelta::apply`. Do not scatter direct `state.field = x` assignments.

### Principle 9 — WndProc instance binding

Bind HWND ↔ struct pointer via `SetWindowLongPtr(GWLP_USERDATA)`. `static mut` and `OnceLock<Mutex<Option<Box<dyn Fn>>>>`-style static dispatchers are forbidden (do not create a path where multiple HWNDs interfere during tests).

Verification: the `no-static-mut` rule of `xtask strict-code`, plus design review.

### Principle 10 — Minimal, localized `#[allow]`

Write `#[allow]` immediately before the target item, not at the top of the file. Broad `#[allow(clippy::all)]` and `#[allow(warnings)]` are forbidden.

Verification: the `no-broad-allow-clippy` / `no-broad-allow-warnings` rules of `xtask strict-code`.

### Principle 11 — Boundary-only panic

`unwrap()` / `expect()` are boundary-only (`main.rs`, the `xtask` crate, `#[cfg(test)]`). Elsewhere, return structured errors via `?` + `thiserror`. Enforce clippy `unwrap_used = deny` and `expect_used = warn` in workspace lints, complemented by the `no-unwrap-outside-boundary` rule of `xtask strict-code`.

### Principle 12 — No `mod.rs`

Do not use `mod.rs`; standardize on the 2018+ form of `module_name.rs` + `module_name/`. Enforce clippy `mod_module_files = deny` in workspace lints.

### Principle 13 — No wildcard imports

Forbid `use foo::*`. Enforce clippy `wildcard_imports = deny` in workspace lints. Do not create prelude modules (exception: update this ADR if creating `linerule_core::prelude`).

## Consequences

- `cargo xtask strict-code` and `cargo xtask dep-graph` become required CI gates
- Revisit the principles when adding a new module
- Adding, removing, or changing a principle requires an ADR revision (maintaining this ADR is the source of truth for the principle list)
