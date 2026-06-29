# 0007 — Debug Build profile (`dist-dev`) and panic strategy asymmetry

**Status:** **Superseded by [[0011-phase-j-slim-down]]** (2026-05-20). The `[profile.dist-dev]` / PDB artifact / `panic = "unwind"` asymmetry has been removed; no `dist-dev` profile exists in the current codebase.

**See also:** [[0003-unsafe-isolation]], [[0004-coverage-policy]], [[0011-phase-j-slim-down]].

## Context

`[profile.release]` is `panic = "abort"` + `lto = "fat"` + `strip = "symbols"` + `overflow-checks = false`. Correct for shipping, but deep debugging gets no PDB symbols, no `catch_unwind` rescue path (`overlay_wnd_proc`), no overflow detection. Under `abort`, `catch_unwind` is effectively dead.

## Decision

**Add `[profile.dist-dev]` and ship a Debug Build artifact from CI. The Release Build profile is unchanged.**

```toml
[profile.dist-dev]
inherits = "release"
debug = "full"
strip = "none"
lto = "thin"             # `fat` + `debug=full` is a known PDB symbol mapping mismatch
panic = "unwind"         # makes catch_unwind path verifiable on real hardware
overflow-checks = true
incremental = false
```

Add a `debug-build (win-x64, native, PDB)` CI job that uploads `linerule-win-x64-debug` (`linerule.exe` + `.pdb`), retention 14 days.

Why Release stays non-`unwind`: binary +10-15%, unwinding tables resident, longer compile times. Shipping keeps the simple, predictable "unexpected panic = immediate death". Real-hardware `catch_unwind` verification is done via the Debug Build artifact instead. Switching Release to unwind is evaluated in a separate Issue/PR.

## Consequences

The `release` vs `dist-dev` asymmetry is intentional:

| Item | `release` | `dist-dev` |
|---|---|---|
| panic | abort | **unwind** |
| catch_unwind | effectively dead | **live** |
| PDB | not uploaded | **uploaded** |
| strip | symbols | **none** |
| lto | fat | **thin** |
| overflow-checks | false | **true** |

## Alternatives considered

- **A. Bundle just the PDB into Release** — rejected. With `abort` + `fat` kept, catch_unwind is dead and the PDB's value is halved.
- **B. Single profile (release with unwind + strip=none)** — rejected. Binary +25-30%, and changing shipping characteristics is a different-scale decision.
- **C. `dist-dev` with `inherits = "dev"`** — rejected. `opt-level=0` is too slow for real-hardware QA.

## Related

- ADR-0002 §4, §7 — the `catch_unwind` rescue path becomes live, making wndproc dispatch invariants verifiable on real hardware
- ADR-0003 — the policy of confining unsafe to `win32_ffi/` is unchanged
