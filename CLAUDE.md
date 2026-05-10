# linerule — Claude Code project guide

Read this first when entering the repo.

## What this is

A digital reading ruler — frameless, transparent, click-through,
always-on-top desktop overlay that follows the cursor. Three modes:

- **Bar** — single horizontal translucent bar at the cursor's Y.
- **Mask** (typoscope) — top + bottom dimmed, slit at the cursor's Y.
- **Vertical** — rotated 90° for 縦書き / 青空文庫 reading.

v0.1 ships Windows 10/11 only; macOS / Linux are deferred to v0.2+
(the platform trait surface in `linerule-platform` is already shaped
so the addition is purely additive).

## Architecture (read the ADRs)

```
                      ┌──────────────────┐
                      │  linerule-core   │  pure logic, IO-free, #![forbid(unsafe_code)]
                      └────────┬─────────┘
                               │
                  ┌────────────┴────────────┐
                  ▼                         ▼
         ┌────────────────────┐   ┌──────────────────┐
         │ linerule-platform  │   │ linerule-config  │  serde + miette
         │  trait + windows   │   └────────┬─────────┘
         └─────────┬──────────┘            │
                   └─────────┬──────────────┘
                             ▼
                    ┌──────────────────┐
                    │ linerule(binary) │  clap + tracing + event loop
                    └──────────────────┘
```

ADRs in `docs/adr/`:

- `0001-tech-stack.md` — winit + vello + wgpu + peniko + windows crate + global-hotkey, why
- `0002-state-model.md` — runtime enum vs type-state, phantom coords, RAII tokens
- `0003-platform-trait.md` — `OverlaySurface` / `HotkeyHost` / `MouseTracker`
- `0004-windows-only-mvp.md` — v0.1 scope cut, v0.2 plan
- `0005-docker-only-execution.md` — every cargo invocation via `just`
- `0006-workspace-lints-single-source.md` — `[workspace.lints]` is the only place
- `0007-release-pipeline.md` — cargo-dist + release-plz role split

## North-star principle: architectural beauty

The user has stated explicitly: **the most important thing in this
project is architectural beauty.** Concretely:

- Prefer ADT decomposition (e.g. `Layer { Geometry × Brush }`) over
  flat product structs when the domain is even mildly composable.
- Match the vocabulary of the chosen rendering stack (peniko's
  `Brush` / `Geometry` / `Affine`) at the core layer too.
- Encode invariants in types: validating newtypes, phantom coords,
  RAII capability tokens.
- `#[non_exhaustive]` is the load-bearing decision, not boilerplate.
- When in doubt, fall back to (a) algorithmic vocabulary, (b)
  categorical structure, (c) explicit ADT decomposition.

## Day-1 tooling

Every cargo invocation goes through `just`, which wraps `docker compose
run --rm dev`. Host cargo is forbidden (ADR-0005).

```sh
just                 # lists all targets
just build           # cargo build --workspace --all-targets
just test            # nextest, all targets
just lint            # fmt-check + clippy + typos + strict-code + shear
just coverage        # cargo-llvm-cov, fail-under-branches 100
just build-windows   # cargo-xwin → x86_64-pc-windows-msvc
just watch [JOB]     # bacon
just hooks           # install lefthook (pre-commit + commit-msg + pre-push)
just ci              # local replica of GHA pipeline
```

## Defensive gates (`cargo run -p xtask -- strict-code`)

Reject patterns we have decided are bug-sources at the gate, not in
review (`feedback_defensive_gates_upfront`):

- `#[allow(...)]` and `cfg_attr(..., allow(...))` — use
  `#[expect(lint, reason = "...")]` instead (Rust 1.81+).
- `#![feature(...)]` — we're stable-only.
- `unsafe` in `linerule-core` / `linerule-config` (compile-time
  forbidden via `#![forbid(unsafe_code)]`).
- `unsafe` blocks in `linerule-platform` without `// SAFETY:` comment.
- bare `TODO/FIXME/XXX` without an issue / milestone / ADR reference.
- `println!` / `eprintln!` in library crates (use `tracing`).
- `continue-on-error: true` in `.github/workflows/`.
- `on.schedule:` triggers in workflows (Dependabot weekly only —
  feedback_no_cron_in_repos).

## DO NOT

- Modify `[lints]` per crate. Workspace `[workspace.lints]` is the
  single source of truth (ADR-0006 / cargo#12697).
- Use `#[allow(...)]` to silence lints. Refactor or use `#[expect]`.
- Add nightly toolchain features (`#![feature(...)]`).
- Add `unsafe` to `linerule-core` or `linerule-config`.
- Add an `on.schedule:` GHA workflow (Dependabot is the only allowed
  scheduled job).
- Pin tool versions from memory. Verify against crates.io / GitHub
  Releases at decision time (`feedback_verify_latest_versions`).
- Run cargo / clippy / nextest directly on the host. `just` only
  (ADR-0005).
- Pin individual lint exceptions in per-crate `[lints.clippy]`.
  Cargo silently overrides workspace carve-outs (ADR-0006).

## Where to find what

```text
crates/linerule-core/src/lib.rs        Mode / Action / reduce / render / newtypes / phantom coords
crates/linerule-core/tests/            unit_*.rs (cells), property_*.rs (invariants)
crates/linerule-config/src/lib.rs      Config schema + serde + miette
crates/linerule-config/tests/          golden_toml.rs round-trip + diagnostics
crates/linerule-platform/src/lib.rs    OverlaySurface / HotkeyHost / MouseTracker traits
crates/linerule-platform/src/mock.rs   in-memory mock impls (gated)
crates/linerule-platform/src/windows.rs Windows real impl (task #11)
crates/linerule/src/main.rs            CLI binary, clap subcommands
crates/xtask/src/main.rs               internal dev automation (replaces shell scripts)
crates/xtask/src/strict_code.rs        defensive grep gate, in Rust

docs/adr/                              ADR-0001..0008
.github/workflows/ci.yml               ubuntu-24.04 + windows-2022 matrix
.github/dependabot.yml                 weekly cargo + actions + docker bumps
```
