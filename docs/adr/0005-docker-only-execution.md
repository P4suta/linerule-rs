# 0005. Docker-only execution

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: infra, build

## Context

Per the user's project-wide rule (`feedback_docker_only_execution`),
every cargo / clippy / nextest / etc. invocation must run inside a
container so the toolchain is reproducible across machines and CI.
Host-side cargo invocation is forbidden in automation.

For linerule there is one extra wrinkle: cross-compiling to
`x86_64-pc-windows-msvc` from a Linux dev container requires the MSVC
SDK. `cargo-xwin` vendors the SDK transparently.

## Decision

- All cargo invocations go through `docker compose run --rm dev cargo
  ...`, wrapped in `Justfile` recipes.
- Two compose services: `dev` (TTY-attached interactive) and `ci`
  (non-interactive). Both build from the same `Dockerfile` (single
  `dev` stage; `ci` is a thin reuse target).
- The `dev` image installs `cargo-nextest`, `cargo-llvm-cov`,
  `cargo-deny`, `cargo-audit`, `cargo-shear`, `cargo-semver-checks`,
  `cargo-insta`, `cargo-dist`, `cargo-xwin`, `cargo-bolero`,
  `bacon`, `release-plz`, `git-cliff`, `committed`, `typos-cli`,
  `lefthook`, `just`.
- `mold` is the linker for Linux builds (`/root/.cargo/config.toml`
  inside the image; mirrored at `/workspace/.cargo/config.toml` for
  host visibility).
- The Windows MSVC target spec is installed via `rustup target add
  x86_64-pc-windows-msvc` so `cargo xwin build` finds it.
- Cargo registry / target / xwin caches are persisted via named
  Docker volumes for fast re-runs.

## Consequences

**Becomes easier**
- One `just build` produces identical artifacts on any developer's
  machine.
- CI runs the same image as the dev loop — no CI-only drift.
- The Windows cross-compile is `just build-windows`; no MSVC license
  song-and-dance for contributors.

**Becomes harder**
- First-time image build is slow (15+ min for the full cargo-tools
  install). Mitigated by layer caching: only the changed tool's
  layer rebuilds on bumps.
- Bind-mount permissions on a non-root host UID need the
  `safe.directory` git config triplet to keep git commands working
  inside the container.

## Alternatives considered

- **Host cargo + lock toolchain via rust-toolchain.toml** — relies on
  every contributor having rustup. CI also needs explicit version
  installation. Rejected per project-wide rule.
- **`act` for local CI emulation** — overlapping concern; `just ci`
  is enough.
- **Dev-container only (skip the bare Dockerfile compose)** — Dev
  Containers pin a VS Code dependency we don't want to enforce.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- `Dockerfile`, `compose.yaml`, `Justfile`
- `feedback_docker_only_execution`
