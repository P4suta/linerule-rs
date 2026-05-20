# linerule-rs — task entry points. Routes through Docker unless INSIDE_CONTAINER=1.
#
# Conventions:
# - Every recipe is a thin wrapper. The intelligence lives in `cargo xtask`
#   subcommands (`lint`, `ci`, `strict-code`, `dep-graph`).
# - When INSIDE_CONTAINER=1, recipes run tools directly on $PATH. Outside,
#   they delegate to `docker compose run --rm dev` (or `exec dev` if the
#   dev service is already up — saves ≈1.5 s per invocation).
# - Windows-only operations are explicit (`publish-windows-cross` uses
#   `cargo-xwin` for iteration checks; shippable artifacts come from CI).

inside := env_var_or_default("INSIDE_CONTAINER", "0")

dev_running := `docker compose ps --status running --services 2>/dev/null | grep -c '^dev$' 2>/dev/null || true`
docker_run := if dev_running == "0" { "docker compose run --rm dev" } else { "docker compose exec dev" }

cargo := if inside == "1" { "cargo" } else { docker_run + " cargo" }
rustup := if inside == "1" { "rustup" } else { docker_run + " rustup" }
typos := if inside == "1" { "typos" } else { docker_run + " typos" }
actionlint := if inside == "1" { "actionlint" } else { docker_run + " actionlint" }
lefthook := if inside == "1" { "lefthook" } else { docker_run + " lefthook" }
taplo := if inside == "1" { "taplo" } else { docker_run + " taplo" }
biome := if inside == "1" { "biome" } else { docker_run + " biome" }
yamlfmt := if inside == "1" { "yamlfmt" } else { docker_run + " yamlfmt" }
sh := if inside == "1" { "bash -lc" } else { docker_run + " bash -lc" }
bun := if inside == "1" { "bun" } else { docker_run + " bun" }
bunx := if inside == "1" { "bunx" } else { docker_run + " bunx" }

dev_log := env_var_or_default("LINERULE_LOG", "debug,wnd_proc=info,heartbeat=info,cursor_tracker=info")

default:
    @just --list

# ----- first-run bootstrap -----

# One-shot setup for a fresh clone. Pulls the prebuilt dev image (or builds
# locally if absent), brings up the persistent dev container, installs git
# hooks, restores the commitlint bun packages, and runs `just doctor` to
# confirm every tool is reachable. Idempotent — re-run any time the
# environment feels off.
#
# The Windows cross-compile sysroot (MSVC CRT + Windows SDK, ~500 MB) is
# baked into the dev image, so the first `just cross-check` is instant.
bootstrap:
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

# ----- environment health check -----

# Verify every dev tool the recipes rely on is reachable inside the dev
# container. Run when joining the project or when something starts failing
# in a confusing way. Exits non-zero on the first missing tool so CI / scripts
# can fail loudly rather than silently.
doctor:
    @echo "==> linerule-rs doctor"
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

# ----- one-shot environment -----

docker-build:
    @echo "==> docker compose build (GITHUB_TOKEN auto-loaded from gh CLI if available)"
    GITHUB_TOKEN="${GITHUB_TOKEN:-$(gh auth token 2>/dev/null || true)}" docker compose build

shell:
    {{docker_run}} bash

clean-docker:
    @echo "==> docker compose down (volumes + local images)"
    docker compose down --volumes --rmi local

dev-up:
    @echo "==> docker compose up -d dev"
    docker compose up -d dev
    @echo "dev container is up — `just <recipe>` now uses docker exec (faster)."

dev-down:
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
    @echo "==> cargo nextest run --workspace --exclude linerule-platform-windows"
    {{cargo}} nextest run --workspace --exclude linerule-platform-windows
    @echo "==> cargo test --doc --workspace --exclude linerule-platform-windows"
    {{cargo}} test --doc --workspace --exclude linerule-platform-windows

# Inner-loop test alias (doctest を省くので速い)。
t:
    {{cargo}} nextest run --workspace --exclude linerule-platform-windows --no-fail-fast

# Doctest 単独実行（`just test` にも含まれるが個別に叩きたいとき用）。
doctest:
    @echo "==> cargo test --doc --workspace --exclude linerule-platform-windows"
    {{cargo}} test --doc --workspace --exclude linerule-platform-windows

test-windows:
    @echo "==> cargo nextest run --workspace --run-ignored all"
    {{cargo}} nextest run --workspace --run-ignored all
    @echo "==> cargo test --doc --workspace"
    {{cargo}} test --doc --workspace

# Coverage report (advisory threshold 80%).
coverage:
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
docs-dep-graph:
    {{sh}} "{{cargo}} depgraph --workspace-only | dot -Tsvg > docs/dep-graph.svg"

# Render module tree to ASCII for each in-house crate.
docs-modules:
    {{sh}} "{{cargo}} modules structure --package linerule-core > docs/modules/linerule-core.txt"
    {{sh}} "{{cargo}} modules structure --package linerule-platform-windows > docs/modules/linerule-platform-windows.txt 2>/dev/null || true"
    {{sh}} "{{cargo}} modules structure --package linerule-app > docs/modules/linerule-app.txt 2>/dev/null || true"
    {{sh}} "{{cargo}} modules structure --package xtask > docs/modules/xtask.txt"

# Sync `linerule-core` crate-level doc → README.md (marker block).
# cargo-rdme reads `[package.metadata.cargo-rdme]` in the crate's Cargo.toml
# to locate the README, so we just `cd` into the crate.
docs-readme:
    {{sh}} "cd crates/linerule-core && {{cargo}} rdme --force"

# Generate all the auto-docs in one go.
docs: docs-dep-graph docs-modules docs-readme

# Open generated rustdoc locally.
doc:
    {{cargo}} doc --workspace --no-deps --open

# Aggregated lint pipeline (everything that gates merges).
lint:
    @echo "==> cargo xtask lint"
    {{cargo}} xtask lint

# Local CI replica.
ci:
    @echo "==> cargo xtask ci"
    {{cargo}} xtask ci

# ----- cross-compile checks -----

# Compile-only check that Windows code still builds from Linux dev container.
# `--all-targets` でテスト・examples・benches も対象にし、Windows native CI
# (`cargo build --workspace --all-targets`) と検出範囲を揃える。
cross-check:
    @echo "==> cargo xwin check --workspace --all-targets --target x86_64-pc-windows-msvc"
    {{cargo}} xwin check --workspace --all-targets --target x86_64-pc-windows-msvc

# Iteration-quality cross build (NOT shippable — see ADR-0001 deployment notes).
publish-windows-cross:
    {{cargo}} xwin build --release --target x86_64-pc-windows-msvc -p linerule-app

# ----- distribution -----

# Native Windows build (run on a Windows host — produces the shippable binary).
publish-windows-native:
    {{cargo}} build --release -p linerule-app --target x86_64-pc-windows-msvc

# ----- diagnostics -----

# Tail today's events file with subsystem filter.
logs-tail subsystem="*":
    {{sh}} "tail -F \"$APPDATA/linerule\"/events.jsonl.* 2>/dev/null | jq -c 'select(.target | test(\"{{subsystem}}\"))'"

# Pretty-print today's events.
logs-pretty:
    {{sh}} "cat \"$APPDATA/linerule\"/events.jsonl.* | jq -C ."

logs-clear:
    {{sh}} "rm -f \"$APPDATA/linerule\"/events.jsonl.*"

crash-list:
    {{sh}} "ls -1t \"$APPDATA/linerule\"/crash-*.json 2>/dev/null"

crash-latest:
    {{sh}} "ls -1t \"$APPDATA/linerule\"/crash-*.json 2>/dev/null | head -1 | xargs -r cat | jq -C ."

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
    {{sh}} "git diff --quiet docs/ README.md || (echo 'docs drift detected — run \\`just docs\\` and commit' >&2; exit 1)"

_hook-commitlint msg_path:
    {{bunx}} commitlint --edit {{msg_path}}
