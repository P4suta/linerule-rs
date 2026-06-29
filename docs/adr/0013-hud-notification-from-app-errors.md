# 0013 — Consume AppError::class() to surface Recoverable as HUD notification

**Status:** Accepted.

**See also:** [[0008-error-class-and-app-aggregator]], [[0012-foreground-hook-and-hud-telemetry]].

## Context

ADR-0008 introduced `ErrorClass`, `class()` on each error type, and the `AppError` aggregator, but `AppError` and `class()` stayed `#[allow(dead_code)]` with no consumer. This ADR wires Recoverable errors into a HUD notification path and removes the dead_code allows.

## Decision

Consume `AppError::class()` in `boot::run_overlay`'s error path; `Recoverable` early errors are pushed as a HUD notification toast. Removes 4 dead_code allows.

### 1. `classify_and_log(err: &AppError) -> RunDecision`

Add to `linerule-app/src/error.rs`:

```rust
pub(crate) fn classify_and_log(err: &AppError) -> RunDecision {
    match err.class() {
        ErrorClass::Recoverable => {
            tracing::warn!(...);
            RunDecision::Continue
        }
        ErrorClass::Fatal => {
            tracing::error!(...);
            RunDecision::Stop
        }
        ErrorClass::ProgrammerError => {
            tracing::error!(...);
            debug_assert!(false, ...);
            RunDecision::Stop
        }
    }
}
```

- `Recoverable`: caller pushes HUD and continues.
- `Fatal`: bubble up to main via `?`, hits the crash-dump path.
- `ProgrammerError`: `debug_assert!` fires in debug builds, equivalent to Fatal in release (consistent with ADR-0009).

### 2. Consumer path in `boot::run_overlay`

Early errors such as `SetProcessDpiAwarenessContext` failure:

```rust
if let Err(e) = set_dpi_aware() {
    let app_err: AppError = e.into();
    if classify_and_log(&app_err) == RunDecision::Continue {
        early_recoverable.push(format!("DPI awareness: {app_err}"));
    }
}
// ...
overlay.attach_dcomp()?;
overlay.register_hotkeys(...)?;
for msg in early_recoverable.drain(..) {
    overlay.state().push_notification(NotificationClass::Warn, msg, 10_000);
}
```

Push to the HUD only after the `OverlayWindow` handle exists (after dcomp attach). The existing `HudNotification` / `push_notification` / `wndproc::build_notifications` path suffices; no new platform API needed.

### 3. Cleaning up the dead_code allows

| File | Resolution |
|---|---|
| `linerule-app/src/error.rs:32` | remove allow, now consumed by `classify_and_log` |
| `linerule-app/src/error.rs:57` | same |
| `linerule-app/src/logging.rs:83` | drop the future-use `Subscriber` import |
| `linerule-platform-windows/src/overlay_state.rs:318` | drop `ChordSpec` import (HUD display extension is a separate issue) |

## Consequences

- `crates/linerule-app/src/error.rs` — remove 2 dead_code allows, add `classify_and_log` + `RunDecision`
- `crates/linerule-app/src/boot.rs` — accumulate recoverable errors when `set_dpi_aware()` fails, push them as HUD notifications in one batch after `attach_dcomp()`
- `crates/linerule-app/src/logging.rs` — remove `Subscriber` import + dead_code allow
- `crates/linerule-platform-windows/src/overlay_state.rs` — remove `ChordSpec` import + dead_code allow

## Alternatives considered

- **A. Don't consume AppError, just mix the class string into the log** — rejected: `class()` stays dead code.
- **B. Bundle the HUD push into ADR-0008** — rejected: violates separation between introducing the types and consuming them.
- **C. Store ChordSpec parsed into `HotkeyConflict`** — rejected: orthogonal to the main thread, and the separate issue is small.

## Related

- [[0008-error-class-and-app-aggregator]]: prior ADR.
- [[0009-diagnostics-cli-and-debug-assertions]]: consistent with `ProgrammerError`'s `debug_assert!` behavior.
- [[0011-phase-j-slim-down]]: does not violate the portable doctrine.
- [[0012-foreground-hook-and-hud-telemetry]]: independent of the HUD frame extension.
