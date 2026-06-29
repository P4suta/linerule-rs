# 0011 — Drop AppData logs / dist-dev / PDB; return to a portable thin reading tool

**Status:** Accepted (2026-05-20).

**Supersedes:** [[0007-debug-build-and-panic-strategy]] (`[profile.dist-dev]` + `panic = "unwind"` + PDB distribution).

**Amends:** [[0010-release-assets-workflow]] (Release asset reduced to a single release-profile binary).

**See also:** [[0009-diagnostics-cli-and-debug-assertions]] (`linerule diagnostics` only changes path, keeps function).

## Context

We had accumulated `%APPDATA%` logs, a `dist-dev` profile, PDB distribution, and a `panic = "unwind"` asymmetry for crash analysis — all excessive for a portable reading aid. The user asked for "JSON logs next to the exe, and plain JSON for debug too, without PDBs."

## Decision

**Distribute a single `linerule.exe`, log next to the exe. Drop the `dist-dev` profile, PDB distribution, and the panic asymmetry.**

### 1. Log location: `%APPDATA%\linerule\` → same dir as exe

Change `logging.rs::data_dir()` from `ProjectDirs` to `current_exe().parent()`, dropping the `directories` crate dependency.

- Keep daily rolling (`tracing_appender::rolling::daily`) — standard, useful for separating logs across runs
- Crash dump JSON also colocates (`crash_dump::crash_path` follows via `data_dir()`)
- If the dir is not writable (under Program Files), `init()` returns `Err` → startup fails. Intentional fail under the portable assumption

### 2. Drop `[profile.dist-dev]` / unify on `panic = "abort"`

The distributed binary uses the release profile only (`panic = "abort"` + `strip = "symbols"` + `lto = "fat"`).

- The `catch_unwind` path in `overlay_wnd_proc` is dead under abort but kept — a harmless guard at the `unsafe` boundary and a hedge for a future return to unwind
- Crash dump still works under `panic = "abort"` because the panic hook fires before abort, so it is kept

### 3. Reducing CI / Release artifacts

- Remove the `debug-build` job from `ci.yml`
- Remove the `dist-dev` build and `-debug.exe` / `-debug.pdb` from `release-assets.yml`, attaching only `linerule-vX.Y.Z-win-x64.exe`
- Remove the `build-debug` recipe from `Justfile`

### 4. What we keep

| Item | Reason |
|---|---|
| daily rolling (`events.jsonl.YYYY-MM-DD`) | standard, separates logs across runs |
| crash dump JSON | one panic hook, fires even under abort |
| `linerule diagnostics` | only path follows, function kept |
| `event_ring` | supplier of `recent_events` for the crash dump |
| `catch_unwind` in `overlay_wnd_proc` | dead under abort but a harmless guard |
| `LINERULE_LOG` env var | one `EnvFilter` line, standard |

## Consequences

- Made `logging.rs::data_dir()` exe-relative, dropped the `directories` dependency
- Removed `directories` and `[profile.dist-dev]` from the workspace `Cargo.toml`
- Removed `build-debug` from `Justfile` and the `debug-build` job from `ci.yml`; reduced `release-assets.yml`
- Marked ADR-0007 Superseded, updated ADR-0010's build strategy / naming table, updated the log path in `README.md`

## Alternatives considered

- **A. Keep AppData, drop only PDB** — rejected: the user explicitly called AppData itself "fancy"; OS integration conflicts with portability.
- **B. Keep dist-dev, drop only PDB** — rejected: what remains is "an almost-release second profile," not worth the dual maintenance.
- **C. Also drop crash dump JSON** — rejected: a single JSON file write is the minimum viable diagnostic; kept, to be discussed in a separate task.

## Related

- ADR-0007 (supersede source), ADR-0010 (amend source), ADR-0009 (diagnostics)
- memory `feedback-enforce-in-code-not-docs` — enforcement is achieved by code change; this ADR is the history record
