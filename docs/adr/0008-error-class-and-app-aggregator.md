# 0008 — `ErrorClass` classification and `AppError` aggregator

**Status:** Accepted (2026-05-20).

**See also:** [[0002-architecture-principles]] (closed sum / one-way dependency), [[0003-unsafe-isolation]], [[0007-debug-build-and-panic-strategy]].

## Context

Error types used `thiserror::Error` + `#[from]` to keep the `?` chain, but two things were missing:

1. Recoverability was not expressed in the type; callers `match`ed it case by case.
2. There was no `PlatformError → LineruleError` merge path, so the app layer relied on `anyhow` or manual matching.

Adding a `Platform` variant to `LineruleError` would invert the `core → platform-windows` dependency and break dependency-direction purity ([[0002-architecture-principles]] §1). The orphan rule also forbids writing `impl From<PlatformError> for LineruleError` on the platform side.

## Decision

### 1. Add `ErrorClass` to `linerule-core::diagnostics`

A separate enum, orthogonal to `Severity` (logging level):

```rust
pub enum ErrorClass {
    Recoverable,       // log + fallback, continue
    Fatal,             // terminate process + crash report
    ProgrammerError,   // static bug tag (room for debug_assert!)
}
```

Each error type gets a `class()`:

```rust
impl CoreError { pub const fn class(self) -> ErrorClass { ProgrammerError } }
impl ChordError { pub const fn class(&self) -> ErrorClass { Recoverable } }
impl LineruleError { pub const fn class(&self) -> ErrorClass { /* delegate */ } }
impl PlatformError { pub fn class(&self) -> ErrorClass { /* operation-aware */ } }
```

`PlatformError::class` branches on `operation: &'static str`, whitelisting known-recoverable APIs like `RegisterHotKey`; everything else is `Fatal`.

### 2. Add an `AppError` aggregator in `linerule-app/src/error.rs`

Put the merge point in the app layer; do not add a Platform variant to `LineruleError`:

```rust
// linerule-app/src/error.rs
#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error(transparent)] Core(#[from] LineruleError),
    #[cfg(target_os = "windows")]
    #[error(transparent)] Platform(#[from] PlatformError),
    #[error("I/O: {0}")] Io(#[from] std::io::Error),
    #[error("serde: {0}")] Serde(#[from] serde_json::Error),
}

impl AppError {
    pub(crate) fn class(&self) -> ErrorClass { /* delegate internally */ }
}
```

The `Platform` variant is under a cfg gate, so `#[cfg(target_os = "windows")]` limits it to Windows. Linux tests see only 3 variants. `main()` stays `anyhow::Result<()>` and rises into anyhow via the `?` chain through `#[from]`.

## Outcome

- `linerule-core/src/diagnostics.rs`: enum + 4 methods + 7 tests (~140 LOC)
- `linerule-platform-windows/src/error.rs`: `class()` + recoverable whitelist + 6 tests (~80 LOC)
- New `linerule-app/src/error.rs` (`AppError` + tests, ~125 LOC)
- Added `thiserror` dep to `linerule-app/Cargo.toml`, re-export `linerule-core::ErrorClass`

Dependency direction `app → platform-windows → core` purity is preserved (verified with `cargo xtask dep-graph`).

## Alternatives considered

- **A. Add `LineruleError::Platform(PlatformError)` to core** — rejected: dependency inversion.
- **B. `Platform(Box<dyn Error + Send + Sync>)`** — rejected: loses type info, requires downcasting, violates closed sum ([[0002]] §3).
- **C. `From<PlatformError> for LineruleError` on the platform side** — rejected: forbidden by orphan rule.
- **D. Fold `ErrorClass` into `Severity`** — rejected: orthogonal semantics. Every combination (`Recoverable + Warn`, etc.) is meaningful.

## Related

- ADR-0007 — sets `dist-dev` to `panic = "unwind"` to make the `catch_unwind` path live, so `ProgrammerError` is observable even in debug builds.
- ADR-0013 — consumes `AppError::class()` and pushes `Recoverable` to a HUD notification.
