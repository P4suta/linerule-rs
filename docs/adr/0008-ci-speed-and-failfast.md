# 0008. CI speed strategy + fail-fast wrappers

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: infra, ci, performance

## Context

Rust builds are (in)famously slow. Two distinct concerns dominate this
project's iteration cycle:

1. **Image build time.** The dev container needs ~16 cargo plugins
   (`cargo-nextest`, `cargo-llvm-cov`, `cargo-deny`, `cargo-audit`,
   `cargo-shear`, `cargo-semver-checks`, `cargo-insta`, `cargo-dist`,
   `cargo-xwin`, `cargo-edit`, `bacon`, `release-plz`, `git-cliff`,
   `committed`, `typos-cli`, `cargo-bolero`). Compiling every one of
   these from source via `cargo install` took ~15-20 min of cold image
   build — far too slow for every contributor / CI run.

2. **Stalled / hung builds.** A `cargo install` deep in a transitive
   dep tree (rustls, h2, hyper, etc.) can give zero stdout for several
   minutes, indistinguishable from a hang. Without a fail-fast wrapper
   the developer wastes terminal time, and CI burns its budget.

3. **CI cache thrash.** GHA runners get a fresh filesystem per job;
   without explicit caching every job rebuilds dependencies from zero.

## Decision

### 1. cargo-binstall in the Dockerfile

Replace `cargo install --locked` with `cargo binstall --no-confirm
--locked --no-symlinks` for every dev tool that has prebuilt
GitHub-Release binaries (which is all 16 today). Falls back to source
build per-tool when no binary exists, so the policy is robust.

Expected impact: cargo-tools layer drops from ~15-20 min to ~2-3 min.

### 2. Fail-fast wrapper around `docker compose build`

`just build-image` (inline shell wrapper around `docker compose build`):
- `--progress=plain` for grep-able real-time output (vs the JSON-bake
  format that hides individual steps).
- Hard `timeout` (default 30 min, override via
  `LINERULE_BUILD_TIMEOUT_S`).
- Stall watchdog: every 30 s, check whether the log file grew; emit a
  `WARN: build log has not grown for Ns` to stderr after 3 min of
  silence (override via `LINERULE_BUILD_STALL_S`).
- Tail the last 50 log lines to stderr on any non-zero exit.

The plain `just build` recipe gates on `image-ready`, which fails
with an actionable message if `linerule-dev:local` does not exist —
no silent re-pull / re-build. The watchdog stall detection from the
prior `scripts/build-image.sh` was dropped when the script was
removed; `--progress=plain` provides equivalent visibility, and the
30-min hard timeout prevents indefinite hangs.

### 3. CI caching

`.github/workflows/ci.yml`:
- `actions/cache` for `~/.cargo/registry`, `~/.cargo/git`,
  and the project `target/` dir, keyed by `Cargo.lock` hash.
- `Swatinem/rust-cache@v2` on every Rust job (Linux + Windows).
- `docker/build-push-action@v6` with `cache-from: type=gha,scope=...`
  and `cache-to: type=gha,scope=...,mode=max` for the dev image build.
- `mozilla-actions/sccache-action@v0.0.6` to wire `RUSTC_WRAPPER=sccache`
  with a cache backend (GitHub Actions cache).
- `taiki-e/install-action@v2` to install dev tools as precompiled
  binaries on the host runner (when not running through Docker).
- `concurrency: cancel-in-progress: true` so successive pushes cancel
  earlier in-flight runs.

### 4. Push the dev image to GHCR on main update

`.github/workflows/build-image.yml` triggers on push to `main` whenever
`Dockerfile` or `Cargo.lock` changes; builds + pushes
`ghcr.io/p4suta/linerule-dev:latest`. CI runs pull this image instead
of rebuilding from scratch (with a fallback to local build for forks).

### 5. Per-job timeout in CI

Every job in `ci.yml` declares `timeout-minutes: 30`. A genuinely-stuck
job is killed by GHA before it eats the workflow budget.

## Consequences

**Becomes easier**
- Cold dev container build: ~3 min (down from ~20).
- A stalled build is visibly stalled within 3 min, killed within 30.
- CI runs share state across jobs / commits via three independent
  caches (cargo registry / target / sccache).
- New contributors get a working dev shell quickly; the image build is
  the slowest path and even that is bounded.

**Becomes harder**
- The `build-image.sh` script is one more piece of bash to maintain.
  Mitigated by keeping it small (~80 lines) and well-commented.
- GHCR token + permissions setup. Mitigated by GitHub-provided
  `GITHUB_TOKEN`.

## Alternatives considered

- **`cargo install --locked` everywhere** — was the original setup;
  rejected because of #1 above.
- **Pre-bake the entire workspace into the image** — defeats the
  purpose of the bind-mounted source tree; would invalidate the image
  on every code change.
- **Self-hosted runners** — overkill for a hobby-scale repo; cost +
  ops burden not justified.
- **Skip CI entirely and rely on `just ci` locally** — defeats the
  purpose of CI; rejected.
- **`cargo-chef` for project-side dep caching** — useful when baking
  the workspace into an image, but our workspace is bind-mounted.
- **`progress=tty`** — colourful but harder to parse; `plain` works
  better with the watchdog log scraping.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- `Justfile` `build-image` recipe
- `Dockerfile` (cargo-binstall layer)
- `crates/xtask/src/strict_code.rs` (Rust replacement for the
  former `scripts/strict-code.sh`)
- `.github/workflows/ci.yml` (caching jobs)
- [cargo-binstall](https://github.com/cargo-bins/cargo-binstall)
- [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache)
- [mozilla-actions/sccache-action](https://github.com/Mozilla-Actions/sccache-action)
- [taiki-e/install-action](https://github.com/taiki-e/install-action)
