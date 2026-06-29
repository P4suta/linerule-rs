# 0009 — `linerule diagnostics` CLI extension and `debug_assertions` sentries

**Status:** Accepted (2026-05-20).

**See also:** [[0004-coverage-policy]], [[0007-debug-build-and-panic-strategy]], [[0008-error-class-and-app-aggregator]].

## Context

Viewing crash reports and event tails requires manually opening `%APPDATA%\linerule\` and running `jq`. The `just crash-latest` / `just logs-pretty` helpers are awkward to call from a Windows host (e.g. via WSL). Also, there is no use of `debug_assertions`, so debug builds have no invariant checks.

## Decision

### 1. Add flags to `linerule diagnostics`

```rust
Diagnostics {
    #[arg(long)] dry_run: bool,                          // existing (data_dir enumeration only)
    #[arg(long)] last_crash: bool,                       // new
    #[arg(long, value_name = "N")] recent_events: Option<usize>,  // new
    #[arg(long)] data_dir: bool,                         // new
}
```

- **`--data-dir`**: print the absolute path of `%APPDATA%\linerule\` as one line on stdout. For piping.
- **`--last-crash`**: pretty-print the newest `crash-*.json` (max mtime).
- **`--recent-events N`**: pretty-print the last `N` lines of `events.jsonl.<today>` as JSON.
- **`--dry-run`**: existing behavior (data dir enumeration only, no write).

Not mutually exclusive, but the CLI runs exactly one in the priority order `--data-dir → --last-crash → --recent-events → default` (for simplicity). For combined output, call multiple times from the shell.

### 2. Add `#[cfg(debug_assertions)]` sentries

| Location | Invariant | On violation |
|---|---|---|
| `OverlayWndState::record_hotkey` | same id not registered twice | `debug_assert!(prev.is_none(), ...)` |
| before `tick::step` returns | `next_world.frame_seq == prev.wrapping_add(1)` | `debug_assert!(...)` |
| same | `last_hud_refresh_at_ms` monotonically increasing (except first `i64::MIN`) | `debug_assert!(...)` |

Rejected: `RefCell` double-borrow already panics at runtime. Consistency between `id_to_action` and `registered_hotkey_ids` is preserved automatically by the record_hotkey invariant.

`debug_assert!` is compiled out in release builds. Under the `catch_unwind` path (overlay_wnd_proc), a panic is contained to a brief visual glitch (ADR-0007).

## Consequences

- `linerule-app/src/cli.rs`: add 3 flags to `Command::Diagnostics` (~30 LOC + 6 unit tests)
- `linerule-app/src/boot.rs`: `DiagnosticsArgs` + `print_last_crash` / `print_recent_events` (~130 LOC)
- `linerule-platform-windows/src/overlay_state.rs::record_hotkey`: `debug_assert!` (~5 LOC)
- `linerule-core/src/input/tick.rs::step`: 2 `debug_assert!` (~15 LOC)

## Alternatives considered

### A. Dedicated subcommand `linerule crash`

Rejected: extending `diagnostics` has higher discoverability (everything visible in `--help`), and grouping by flags is the clap idiom.

### B. Persist the ring buffer and tail it

Rejected: `event_ring` is in-memory and lost on restart. Persistence is a separate responsibility (room for a future `event_ring::flush_to_file`).

### C. Record only via `tracing::error!` instead of `debug_assert!`

Rejected: panicking immediately in debug builds on an invariant violation is more reliably caught in CI / on-device testing.

## Related

- ADR-0004 — `debug_assert!` in `tick::step` is covered via core
- ADR-0007 — `dist-dev` uses `panic = "unwind"`, so `debug_assert!` panics can be absorbed by catch_unwind
- ADR-0008 — mapping between `ErrorClass::ProgrammerError` and `debug_assert!` (detecting invalid input early in debug builds)
- `linerule-rs-version-bump-cautious` — this change is a patch bump under `fix(app):`
