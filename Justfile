# linerule-rs — task entry points. Three execution modes, auto-detected:
#   inside  : in the dev container (INSIDE_CONTAINER=1)   → tools on $PATH.
#   native  : Windows host without Docker                 → tools on $PATH.
#   docker  : host with Docker                            → `docker compose run/exec dev`.
#
# Conventions:
# - Every recipe is a thin wrapper. The intelligence lives in `cargo xtask`
#   subcommands (`lint`, `ci`, `dep-graph`, `verify`).
# - In `inside`/`native` modes, recipes run tools directly on $PATH. In
#   `docker` mode they delegate to `docker compose run --rm dev` (or `exec dev`
#   if the dev service is already up — saves ≈1.5 s per invocation).
# - Mode selection: INSIDE_CONTAINER=1 → inside; else LINERULE_NATIVE=1 →
#   native; else Docker present → docker, Docker absent → native. Force native
#   on a Docker host with `LINERULE_NATIVE=1 just <recipe>`.
# - Windows-host-only operations are explicit (`publish-windows-native`,
#   `verify`/`verify-blur`). `cross-check`/`publish-windows-cross` use
#   `cargo-xwin` from Linux; shippable artifacts come from CI.

inside := env_var_or_default("INSIDE_CONTAINER", "0")
want_native := env_var_or_default("LINERULE_NATIVE", "0")

# Cheap, side-effect-free docker probe that never errors at parse time, so a
# Docker-less host doesn't emit a command-not-found on every `just` call.
# Short-circuited to "0" inside the container (docker isn't reachable there).
docker_present := if inside == "1" { "0" } else { `command -v docker >/dev/null 2>&1 && echo 1 || echo 0` }

# `inside` and `native` both run tools straight off $PATH; only `docker` prefixes.
mode := if inside == "1" { "inside" } else if want_native == "1" { "native" } else if docker_present == "0" { "native" } else { "docker" }

# Only probe the running dev service in docker mode. In inside/native modes
# this backtick must NOT run — it would spawn docker on a Docker-less host.
dev_running := if mode != "docker" { "0" } else { `docker compose ps --status running --services 2>/dev/null | grep -c '^dev$' 2>/dev/null || true` }
docker_run := if dev_running == "0" { "docker compose run --rm dev" } else { "docker compose exec dev" }

cargo := if mode == "docker" { docker_run + " cargo" } else { "cargo" }
rustup := if mode == "docker" { docker_run + " rustup" } else { "rustup" }
typos := if mode == "docker" { docker_run + " typos" } else { "typos" }
actionlint := if mode == "docker" { docker_run + " actionlint" } else { "actionlint" }
lefthook := if mode == "docker" { docker_run + " lefthook" } else { "lefthook" }
taplo := if mode == "docker" { docker_run + " taplo" } else { "taplo" }
biome := if mode == "docker" { docker_run + " biome" } else { "biome" }
yamlfmt := if mode == "docker" { docker_run + " yamlfmt" } else { "yamlfmt" }
# Non-login shell: a login shell (`-lc`) re-inits PATH and drops cargo.
sh := if mode == "docker" { docker_run + " bash -c" } else { "bash -c" }
bun := if mode == "docker" { docker_run + " bun" } else { "bun" }
bunx := if mode == "docker" { docker_run + " bunx" } else { "bunx" }

# The docker image bakes in nextest; a native host may not have it, so the
# test recipes fall back to `cargo test --test-threads=1` (which also
# serializes the linerule-app event_ring tests that share process state —
# nextest's process-per-test isolates them the same way CI does).
nextest_present := if mode == "docker" { "1" } else { `command -v cargo-nextest >/dev/null 2>&1 && echo 1 || echo 0` }

dev_log := env_var_or_default("LINERULE_LOG", "debug,wnd_proc=info,heartbeat=info,cursor_tracker=info")

default:
    @just --list

# ----- first-run bootstrap -----

# One-shot setup for a fresh clone. Dispatches on the active mode: a Docker
# host gets the container bootstrap; a native (Docker-less) Windows host gets
# the native one. Idempotent — re-run any time the environment feels off.
bootstrap:
    @if [ "{{mode}}" = "docker" ]; then just bootstrap-docker; else just bootstrap-native; fi

# Container bootstrap: pull the prebuilt dev image (or build locally if
# absent), bring up the persistent dev container, install git hooks, restore
# the commitlint bun packages, and run `just doctor`. The Windows
# cross-compile sysroot (MSVC CRT + Windows SDK, ~500 MB) is baked into the
# dev image, so the first `just cross-check` is instant.
bootstrap-docker:
    @echo "==> 1/4 fetch dev image (try ghcr.io, fall back to local build)"
    @docker compose pull 2>/dev/null && echo "  (pulled prebuilt image from ghcr.io)" \
        || (echo "  (no published image, building locally with GITHUB_TOKEN if available)" && \
            GITHUB_TOKEN="${GITHUB_TOKEN:-$(gh auth token 2>/dev/null || true)}" docker compose build)
    @echo "==> 2/4 docker compose up -d dev (persistent dev container)"
    docker compose up -d dev
    @echo "==> 3/4 lefthook install (pre-commit / commit-msg / pre-push hooks)"
    {{lefthook}} install
    @echo "==> 4/4 bun install (commitlint, used by commit-msg hook)"
    {{bun}} install
    @just doctor
    @echo
    @echo "🎉 bootstrap done. Try: just build / just test / just cross-check / just lint"

# Native bootstrap (Docker-less Windows host). Adds the rustup components and
# msvc target, installs the host toolchain via mise (see mise.toml), wires git
# hooks, restores commitlint packages, and confirms the environment. The
# post-`mise install` steps run under `mise exec` so freshly installed tools
# are on PATH within this same run. Idempotent.
bootstrap-native:
    @echo "==> 1/5 rustup components + msvc target"
    rustup component add rustfmt clippy rust-src llvm-tools-preview
    rustup target add x86_64-pc-windows-msvc
    @echo "==> 2/5 mise install (cargo-nextest, biome, yamlfmt, ... — see mise.toml)"
    mise install
    @echo "==> 3/5 lefthook install (pre-commit / commit-msg / pre-push hooks)"
    mise exec -- lefthook install
    @echo "==> 4/5 bun install (commitlint, used by commit-msg hook)"
    mise exec -- bun install
    @echo "==> 5/5 doctor"
    @mise exec -- just doctor-native
    @echo
    @echo "🎉 native bootstrap done. Native superpowers: just run / just verify / just publish-windows-native"
    @echo "   (optional: 'winget install Graphviz.Graphviz' enables docs-dep-graph locally)"

# ----- environment health check -----

# Verify every dev tool the recipes rely on is reachable. Run when joining the
# project or when something starts failing in a confusing way. Dispatches on
# mode: container tool set vs native host tool set.
doctor:
    @if [ "{{mode}}" = "docker" ]; then just doctor-docker; else just doctor-native; fi

# Container health check. Exits non-zero on the first missing tool so CI /
# scripts can fail loudly rather than silently.
doctor-docker:
    @echo "==> linerule-rs doctor (container)"
    @{{docker_run}} bash -c 'set -e; \
        check() { printf "  %-18s " "$1"; out=$($2 2>&1 | head -1) && printf "ok    %s\n" "$out" || { printf "MISSING\n"; exit 1; }; }; \
        check rustc          "rustc --version"; \
        check cargo          "cargo --version"; \
        check cargo-nextest  "cargo nextest --version"; \
        check cargo-xwin     "cargo xwin --version"; \
        check cargo-deny     "cargo deny --version"; \
        check cargo-audit    "cargo audit --version"; \
        check cargo-llvm-cov "cargo llvm-cov --version"; \
        check cargo-machete  "cargo machete --version"; \
        check cargo-sort     "cargo sort --version"; \
        check cargo-rdme     "cargo rdme --version"; \
        check cargo-modules  "cargo modules --version"; \
        check cargo-depgraph "cargo depgraph --version"; \
        check typos          "typos --version"; \
        check taplo          "taplo --version"; \
        check biome          "biome --version"; \
        check yamlfmt        "yamlfmt --version"; \
        check actionlint     "actionlint -version"; \
        check lefthook       "lefthook version"; \
        check just           "just --version"; \
        check mold           "mold --version"; \
        check clang          "clang --version"; \
        check bun            "bun --version"; \
    '
    @echo "==> doctor: ok"

# Native host health check. Same check() helper, but the required set drops the
# Linux-only linker accelerators (mold, clang) and the cross-compile-only
# cargo-xwin, and the optional tools (graphviz dot, cargo-llvm-cov for
# coverage) are soft-checked — a warning, not a hard failure.
doctor-native:
    @echo "==> linerule-rs doctor (native)"
    @{{sh}} 'set -e; \
        check() { printf "  %-18s " "$1"; out=$($2 2>&1 | head -1) && printf "ok    %s\n" "$out" || { printf "MISSING\n"; exit 1; }; }; \
        soft()  { printf "  %-18s " "$1"; out=$($2 2>&1 | head -1) && printf "ok    %s\n" "$out" || printf "warn  (optional) not found\n"; }; \
        check rustc          "rustc --version"; \
        check cargo          "cargo --version"; \
        check rustup         "rustup --version"; \
        check cargo-nextest  "cargo nextest --version"; \
        check cargo-deny     "cargo deny --version"; \
        check cargo-audit    "cargo audit --version"; \
        check cargo-machete  "cargo machete --version"; \
        check cargo-sort     "cargo sort --version"; \
        check cargo-rdme     "cargo rdme --version"; \
        check cargo-modules  "cargo modules --version"; \
        check cargo-depgraph "cargo depgraph --version"; \
        check typos          "typos --version"; \
        check taplo          "taplo --version"; \
        check biome          "biome --version"; \
        check yamlfmt        "yamlfmt --version"; \
        check actionlint     "actionlint -version"; \
        check lefthook       "lefthook version"; \
        check just           "just --version"; \
        check bun            "bun --version"; \
        check jq             "jq --version"; \
        soft  cargo-llvm-cov "cargo llvm-cov --version"; \
        soft  dot            "dot -V"; \
    '
    @echo "==> doctor: ok"

# ----- one-shot environment -----

# Internal guard: docker-management recipes are meaningless in native/inside
# mode. Fail fast with a clear message instead of a raw `docker: not found`.
_require-docker:
    @[ "{{mode}}" = "docker" ] || { echo "This recipe needs Docker (current mode: {{mode}}); it manages the dev container. Native/inside modes don't use it." >&2; exit 1; }

docker-build: _require-docker
    @echo "==> docker compose build (GITHUB_TOKEN auto-loaded from gh CLI if available)"
    GITHUB_TOKEN="${GITHUB_TOKEN:-$(gh auth token 2>/dev/null || true)}" docker compose build

shell: _require-docker
    {{docker_run}} bash

clean-docker: _require-docker
    @echo "==> docker compose down (volumes + local images)"
    docker compose down --volumes --rmi local

dev-up: _require-docker
    @echo "==> docker compose up -d dev"
    docker compose up -d dev
    @echo "dev container is up — `just <recipe>` now uses docker exec (faster)."

dev-down: _require-docker
    docker compose stop dev

# ----- Rust workflow -----

build:
    @echo "==> cargo build --workspace --all-targets"
    {{cargo}} build --workspace --all-targets

build-release:
    @echo "==> cargo build --release --workspace"
    {{cargo}} build --release --workspace

# Inner-loop alias: skips dependency resolution checks.
b:
    @echo "==> cargo build --workspace"
    {{cargo}} build --workspace

test:
    @if [ "{{nextest_present}}" = "1" ]; then \
        echo "==> cargo nextest run --workspace --exclude linerule-platform-windows"; \
        {{cargo}} nextest run --workspace --exclude linerule-platform-windows; \
    else \
        echo "==> nextest absent → cargo test --workspace --exclude linerule-platform-windows -- --test-threads=1"; \
        {{cargo}} test --workspace --exclude linerule-platform-windows -- --test-threads=1; \
    fi
    @echo "==> cargo test --doc --workspace --exclude linerule-platform-windows"
    {{cargo}} test --doc --workspace --exclude linerule-platform-windows

# Inner-loop test alias (doctest を省くので速い)。
t:
    @if [ "{{nextest_present}}" = "1" ]; then \
        {{cargo}} nextest run --workspace --exclude linerule-platform-windows --no-fail-fast; \
    else \
        {{cargo}} test --workspace --exclude linerule-platform-windows -- --test-threads=1; \
    fi

# Doctest 単独実行（`just test` にも含まれるが個別に叩きたいとき用）。
doctest:
    @echo "==> cargo test --doc --workspace --exclude linerule-platform-windows"
    {{cargo}} test --doc --workspace --exclude linerule-platform-windows

test-windows:
    @if [ "{{nextest_present}}" = "1" ]; then \
        echo "==> cargo nextest run --workspace --run-ignored all"; \
        {{cargo}} nextest run --workspace --run-ignored all; \
    else \
        echo "==> nextest absent → cargo test --workspace -- --include-ignored --test-threads=1"; \
        {{cargo}} test --workspace -- --include-ignored --test-threads=1; \
    fi
    @echo "==> cargo test --doc --workspace"
    {{cargo}} test --doc --workspace

# Coverage report (advisory threshold 80%).
coverage:
    @if [ "{{mode}}" != "docker" ] && ! command -v cargo-llvm-cov >/dev/null 2>&1; then \
        echo "coverage: cargo-llvm-cov not found. Run 'mise install' (see mise.toml) and 'rustup component add llvm-tools-preview'." >&2; \
        exit 1; \
    fi
    {{cargo}} llvm-cov --workspace --branch --html --output-dir artifacts/coverage

# Run the overlay locally (Windows host required for actual rendering).
run *args:
    LINERULE_LOG={{dev_log}} {{cargo}} run -p linerule-app -- {{args}}

run-release *args:
    LINERULE_LOG={{dev_log}} {{cargo}} run --release -p linerule-app -- {{args}}

# ----- lint / quality gates -----

fmt:
    {{cargo}} fmt --all
    {{cargo}} sort --workspace
    {{taplo}} fmt
    {{biome}} format --write .
    {{yamlfmt}} .

fmt-check:
    {{cargo}} fmt --all -- --check
    {{cargo}} sort --workspace --check
    {{taplo}} fmt --check
    {{biome}} format .
    {{yamlfmt}} --lint .

clippy:
    {{cargo}} clippy --workspace --all-targets -- -D warnings

deny:
    {{cargo}} deny check advisories bans licenses sources

audit:
    {{cargo}} audit --deny warnings

typos:
    {{typos}}

typos-fix:
    {{typos}} --write-changes

actionlint:
    {{actionlint}} .github/workflows/*.yml

xtask-dep-graph:
    {{cargo}} xtask dep-graph

machete:
    {{cargo}} machete

# ----- auto-generated docs (commit the output; lefthook checks drift) -----

# Render dependency graph SVG (requires graphviz `dot`).
# Plain `cargo`/`dot` inside `{{sh}}` — the sh wrapper already enters the
# container; nesting `{{cargo}}` would double-exec.
docs-dep-graph:
    {{sh}} "command -v dot >/dev/null 2>&1 || { echo 'docs-dep-graph: graphviz dot not found — skipping docs/dep-graph.svg (CI/container regenerates it; install graphviz to render locally).' >&2; exit 0; }; cargo depgraph --workspace-only | dot -Tsvg > docs/dep-graph.svg"

# Render module tree to ASCII for each in-house crate. NO_COLOR keeps the
# committed files free of ANSI escapes (deterministic for the drift check).
# linerule-platform-windows is cfg(windows) so it yields a minimal tree on Linux.
docs-modules:
    {{sh}} "NO_COLOR=1 cargo modules structure --package linerule-core > docs/modules/linerule-core.txt"
    @if [ "{{mode}}" = "native" ] && [ "{{os()}}" = "windows" ]; then \
        echo "docs-modules: skip linerule-platform-windows.txt on native Windows (cfg(windows) full tree drifts from the committed Linux-canonical tree; CI/container regenerates it)"; \
    else \
        {{sh}} "NO_COLOR=1 cargo modules structure --package linerule-platform-windows > docs/modules/linerule-platform-windows.txt 2>/dev/null || true"; \
    fi
    {{sh}} "NO_COLOR=1 cargo modules structure --package linerule-app > docs/modules/linerule-app.txt 2>/dev/null || true"
    {{sh}} "NO_COLOR=1 cargo modules structure --package xtask > docs/modules/xtask.txt"

# Sync `linerule-core` crate-level doc → README.md (marker block). The
# `-r` path is passed explicitly; cargo-rdme 1.5 does not honor the
# `readme-path` metadata key reliably.
docs-readme:
    {{sh}} "cd crates/linerule-core && cargo rdme --force -r ../../README.md"

# Generate all the auto-docs in one go.
docs: docs-dep-graph docs-modules docs-readme

# Open generated rustdoc locally.
doc:
    {{cargo}} doc --workspace --no-deps --open

# `RUSTDOCFLAGS=-D warnings` 下で rustdoc を build。.github/workflows/docs.yml
# (main push 時に GitHub Pages へ publish するジョブ) と同じ厳しさで warning を
# error 扱いし、`pre-push` で push 前に検出する。
#
# `docker compose exec/run` の `-e` flag を使うため `{{docker_run}}` 展開を手で
# 書き分ける（テンプレートは末尾に `dev` を含むため -e を直接挟めない）。
# `dev` service が起動済みなら `exec` で速い、停止中なら `run --rm` で起動する。
# 後者を fallback として持たないと、pre-push hook が dev サービス停止時に必ず
# `service "dev" is not running` で失敗する。
rustdoc-check:
    @echo "==> cargo doc --workspace --no-deps --exclude linerule-platform-windows (RUSTDOCFLAGS=-D warnings)"
    @if [ "{{mode}}" != "docker" ]; then \
        RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --exclude linerule-platform-windows; \
    elif [ "{{dev_running}}" = "0" ]; then \
        docker compose run --rm -e RUSTDOCFLAGS="-D warnings" dev cargo doc --workspace --no-deps --exclude linerule-platform-windows; \
    else \
        docker compose exec -e RUSTDOCFLAGS="-D warnings" dev cargo doc --workspace --no-deps --exclude linerule-platform-windows; \
    fi

# Aggregated lint pipeline (everything that gates merges). LINERULE_MODE lets
# xtask adapt the native steps (drop xwin, serialize tests); in docker mode it
# stays on the host shell and never reaches the container, so xtask defaults.
lint:
    @echo "==> cargo xtask lint"
    LINERULE_MODE={{mode}} {{cargo}} xtask lint

# Local CI replica.
ci:
    @echo "==> cargo xtask ci"
    LINERULE_MODE={{mode}} {{cargo}} xtask ci

# ----- cross-compile checks -----

# Compile-only check that Windows code still builds from Linux dev container.
# `--all-targets` でテスト・examples・benches も対象にし、Windows native CI
# (`cargo build --workspace --all-targets`) と検出範囲を揃える。
cross-check:
    @if [ "{{mode}}" = "native" ]; then \
        echo "==> native host: cargo check --workspace --all-targets (xwin cross-check is a Linux-only concern; the host already targets msvc)"; \
        {{cargo}} check --workspace --all-targets; \
    else \
        echo "==> cargo xwin check --workspace --all-targets --target x86_64-pc-windows-msvc"; \
        {{cargo}} xwin check --workspace --all-targets --target x86_64-pc-windows-msvc; \
    fi

# Iteration-quality cross build (NOT shippable; native artifacts come from CI).
publish-windows-cross:
    @if [ "{{mode}}" = "native" ]; then \
        echo "==> native host: redirecting to publish-windows-native (xwin is Linux-only)"; \
        just publish-windows-native; \
    else \
        {{cargo}} xwin build --release --target x86_64-pc-windows-msvc -p linerule-app; \
    fi

# ----- distribution -----

# Native Windows build (run on a Windows host — produces the shippable binary).
publish-windows-native:
    {{cargo}} build --release -p linerule-app --target x86_64-pc-windows-msvc

# ----- GUI smoke (Windows host only) -----
#
# These drive the real overlay window, which the Linux dev container can't do,
# so they call `cargo` directly (NOT the docker `{{cargo}}` wrapper) — the exe
# must be the native Windows build. `cargo xtask verify` reuses the exact CI
# release-build verdict: launch via --duration-ms, then judge events.jsonl
# (no ERROR / tick-failed / crash dump; clean message-loop exit).

# Build linerule.exe and run the GUI smoke (default profile: debug).
verify profile="debug":
    @if [ "{{profile}}" = "release" ]; then cargo build --release -p linerule-app; else cargo build -p linerule-app; fi
    cargo xtask verify --profile {{profile}}

# Same smoke seeded into Horizontal + Blur from startup, exercising the WinRT
# backdrop-blur COM path immediately (matches the CI release-build smoke).
verify-blur profile="debug":
    @if [ "{{profile}}" = "release" ]; then cargo build --release -p linerule-app; else cargo build -p linerule-app; fi
    cargo xtask verify --profile {{profile}} --mode horizontal --effect blur

# ----- diagnostics -----
#
# Phase J (ADR-0011) 以降、ログは `linerule.exe` と同じディレクトリに出る
# portable 運用。これらの recipes は dev profile (`target/debug/linerule.exe`)
# を起動した場合の出力先 `target/debug/` を assume する。`target/release/` の
# log/crash を見たいときは LOG_DIR=target/release just <recipe> で上書きできる。

log_dir := env_var_or_default("LOG_DIR", "target/debug")

# Tail today's events file with subsystem filter.
logs-tail subsystem="*":
    {{sh}} "tail -F {{log_dir}}/events.jsonl.* 2>/dev/null | jq -c 'select(.target | test(\"{{subsystem}}\"))'"

# Pretty-print today's events.
logs-pretty:
    {{sh}} "cat {{log_dir}}/events.jsonl.* | jq -C ."

logs-clear:
    {{sh}} "rm -f {{log_dir}}/events.jsonl.*"

crash-list:
    {{sh}} "ls -1t {{log_dir}}/crash-*.json 2>/dev/null"

crash-latest:
    {{sh}} "ls -1t {{log_dir}}/crash-*.json 2>/dev/null | head -1 | xargs -r cat | jq -C ."

# ----- git hooks -----

hooks:
    {{lefthook}} install
    {{bun}} install

# ----- lefthook delegated recipes (do not run directly) -----

_hook-fmt +files:
    {{cargo}} fmt -- {{files}}

_hook-typos-fix +files:
    {{typos}} --write-changes {{files}}

_hook-taplo-fmt +files:
    {{taplo}} fmt {{files}}

_hook-cargo-sort:
    {{cargo}} sort --workspace

_hook-biome-format +files:
    {{biome}} format --write {{files}}

_hook-yamlfmt +files:
    {{yamlfmt}} {{files}}

_hook-actionlint +files:
    {{actionlint}} {{files}}

_hook-xtask-dep-graph:
    {{cargo}} xtask dep-graph

_hook-docs-drift:
    just docs
    {{sh}} "git diff --quiet docs/ README.md || (echo 'docs drift detected — run: just docs, then stage docs/ and README.md' >&2; exit 1)"

_hook-commitlint msg_path:
    {{bunx}} commitlint --edit {{msg_path}}
