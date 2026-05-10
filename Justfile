# linerule task runner — the ONE entry point for every dev / CI operation.
# Every cargo / clippy / nextest invocation runs inside the dev container
# (ADR-0005 Docker-only). Recipes are grouped via `[group("...")]` so
# `just --list` reads top-down by intended user task.

set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load := false

# --- internal helpers ---------------------------------------------------------

# Interactive dev container (TTY attached). Quiet variant suppresses the
# "Container ... Created" chatter so tight edit loops scroll less.
_dev   := "docker compose run --rm dev"
_quiet := "docker compose run --rm --quiet-pull dev"
# Non-interactive variant for CI-like invocations
_ci    := "docker compose run --rm --no-TTY ci"

_BUILD_TIMEOUT := env_var_or_default("LINERULE_BUILD_TIMEOUT_S", "1800")

# --- meta ---------------------------------------------------------------------

# Show the categorised recipe list (default).
[private]
default:
    @just --list --unsorted --list-heading $'linerule task runner — `just <recipe>`\n\n'

# --- bootstrap ----------------------------------------------------------------

# Build the dev container image. Wraps `docker compose build` with
# progress=plain visibility and a hard timeout (ADR-0008 fail-fast).
[group('bootstrap')]
build-image:
    @echo "==> build-image: timeout={{_BUILD_TIMEOUT}}s, progress=plain"
    timeout --signal=KILL --kill-after=10s {{_BUILD_TIMEOUT}} \
        docker compose build --progress=plain dev || \
        (echo "ERROR: image build failed or exceeded {{_BUILD_TIMEOUT}}s" >&2 ; exit 1)

# Internal: bail with an actionable message if the dev image isn't built yet.
[private]
image-ready:
    @docker image inspect linerule-dev:local >/dev/null 2>&1 \
        || (echo "ERROR: linerule-dev:local image not found. Run 'just build-image' first." >&2 ; exit 2)

# Install lefthook git hooks (pre-commit + commit-msg + pre-push).
[group('bootstrap')]
hooks:
    {{_dev}} lefthook install

# Remove lefthook git hooks.
[group('bootstrap')]
hooks-uninstall:
    {{_dev}} lefthook uninstall

# --- developer dev loop -------------------------------------------------------

# bacon file-watcher inside the dev container (default job: check).
# Keybindings: c clippy / t test / d doc / f failing-only / q quit.
[group('dev loop')]
watch JOB="":
    {{_dev}} bacon {{JOB}}

# Headless bacon — pipe-friendly, no TUI.
[group('dev loop')]
watch-headless JOB="check":
    {{_ci}} bacon --headless --job {{JOB}}

# Fast incremental syntax/type check — sub-second after warm cache.
# The right "did I just break something" loop, much faster than `build`.
[group('dev loop')]
check: image-ready
    {{_dev}} cargo check --workspace --all-targets

# Auto-fix everything that is fixable: fmt, clippy --fix, shear --fix.
# One container session, one cargo invocation chain — no per-tool startup.
[group('dev loop')]
fix: image-ready
    {{_dev}} bash -c 'set -e; \
        cargo fmt --all; \
        cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged -- -D warnings; \
        cargo shear --fix || true'

# Drop into an interactive dev shell.
[group('dev loop')]
shell: image-ready
    {{_dev}} bash

# Run the linerule CLI with arbitrary args.
[group('dev loop')]
run *ARGS: image-ready
    {{_dev}} cargo run --package linerule --quiet -- {{ARGS}}

# --- build / test -------------------------------------------------------------

# Debug build of the whole workspace.
[group('build / test')]
build: image-ready
    {{_dev}} cargo build --workspace --all-targets

# Optimised release build.
[group('build / test')]
build-release: image-ready
    {{_dev}} cargo build --release --workspace

# Cross-compile to Windows from WSL via cargo-xwin (vendors MSVC SDK).
[group('build / test')]
build-windows: image-ready
    {{_dev}} cargo xwin build --release --target x86_64-pc-windows-msvc

# nextest run, all targets.
[group('build / test')]
test *ARGS: image-ready
    {{_dev}} cargo nextest run --workspace --all-targets {{ARGS}}

# Doctests (nextest skips these by design).
[group('build / test')]
test-doc: image-ready
    {{_dev}} cargo test --workspace --doc

# Property-test sweep (bolero in proptest mode).
[group('build / test')]
prop: image-ready
    {{_dev}} cargo nextest run --workspace -E 'test(property_)'

# Snapshot tests (cargo-insta with `--review` interactivity).
[group('build / test')]
snap: image-ready
    {{_dev}} cargo insta test --workspace --review

# --- lint / static analysis ---------------------------------------------------
#
# Two flavours, picked by the use case:
#   `lint-quick` — sub-second after warm cache; fmt-check + typos + strict-code.
#                  Right for the inner loop / pre-commit hook.
#   `lint`       — adds clippy + shear; ~1 minute warm. Right before push / CI.
#
# Both run as a single `bash -c` inside ONE container session so we pay the
# docker startup cost once instead of N times.

# Cheap fast lint pass — fmt-check + typos + strict-code in one shell.
[group('lint / analysis')]
lint-quick: image-ready
    {{_dev}} bash -c 'set -e; \
        cargo fmt --all -- --check; \
        typos; \
        cargo run --quiet --release --package xtask -- strict-code'

# Full lint pass — adds clippy + shear; what `just ci` runs.
[group('lint / analysis')]
lint: image-ready
    {{_dev}} bash -c 'set -e; \
        cargo fmt --all -- --check; \
        typos; \
        cargo run --quiet --release --package xtask -- strict-code; \
        cargo shear; \
        cargo clippy --workspace --all-targets --all-features -- -D warnings'

# Standalone recipes — useful when iterating on one specific gate.
[group('lint / analysis')]
fmt: image-ready
    {{_dev}} cargo fmt --all

[group('lint / analysis')]
fmt-check: image-ready
    {{_dev}} cargo fmt --all -- --check

[group('lint / analysis')]
clippy: image-ready
    {{_dev}} cargo clippy --workspace --all-targets --all-features -- -D warnings

[group('lint / analysis')]
typos: image-ready
    {{_dev}} typos

[group('lint / analysis')]
strict-code: image-ready
    {{_dev}} cargo run --quiet --release --package xtask -- strict-code

[group('lint / analysis')]
deny: image-ready
    {{_dev}} cargo deny check

[group('lint / analysis')]
audit: image-ready
    {{_dev}} cargo audit

[group('lint / analysis')]
shear: image-ready
    {{_dev}} cargo shear

[group('lint / analysis')]
semver: image-ready
    {{_dev}} cargo semver-checks

# --- coverage -----------------------------------------------------------------

# C1 / branch coverage gate. Floor 100% on linerule-core / linerule-config;
# linerule-platform Win32 FFI is excluded (boundary).
_COV_FLOOR := "100"
_COV_IGNORE := "(target/|/main\\.rs$|crates/linerule-platform/src/windows.rs|crates/xtask/)"

[group('coverage')]
coverage: image-ready
    {{_dev}} cargo llvm-cov nextest \
        --workspace \
        --branch \
        --ignore-filename-regex '{{_COV_IGNORE}}' \
        --fail-under-branches {{_COV_FLOOR}}

[group('coverage')]
coverage-html: image-ready
    {{_dev}} cargo llvm-cov nextest \
        --workspace \
        --branch \
        --ignore-filename-regex '{{_COV_IGNORE}}' \
        --html --output-dir coverage/html

# --- release ------------------------------------------------------------------

[group('release')]
release-dry: image-ready
    {{_dev}} release-plz update --dry-run

[group('release')]
dist-plan: image-ready
    {{_dev}} cargo dist plan

# --- aggregate ----------------------------------------------------------------

# Local replica of the full CI pipeline. Run before push.
[group('aggregate')]
ci: lint build test test-doc deny audit coverage

# --- cleanup ------------------------------------------------------------------

[group('cleanup')]
clean: image-ready
    {{_dev}} cargo clean --workspace

# Tear down all compose state (destroys cached registry/target/sccache volumes)
[group('cleanup')]
nuke:
    docker compose down -v --remove-orphans
