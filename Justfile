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
npx := if inside == "1" { "npx --no" } else { docker_run + " npx --no" }

dev_log := env_var_or_default("LINERULE_LOG", "debug,wnd_proc=info,heartbeat=info,cursor_tracker=info")

default:
    @just --list

# ----- one-shot environment -----

docker-build:
    docker compose build

shell:
    {{docker_run}} bash

clean-docker:
    docker compose down --volumes --rmi local

dev-up:
    docker compose up -d dev
    @echo "dev container is up — `just <recipe>` now uses docker exec (faster)."

dev-down:
    docker compose stop dev

# ----- Rust workflow -----

build:
    {{cargo}} build --workspace --all-targets

build-release:
    {{cargo}} build --release --workspace

# Inner-loop alias: skips dependency resolution checks.
b:
    {{cargo}} build --workspace

test:
    {{cargo}} nextest run --workspace --exclude linerule-platform-windows

# Inner-loop test alias.
t:
    {{cargo}} nextest run --workspace --exclude linerule-platform-windows --no-fail-fast

test-windows:
    {{cargo}} nextest run --workspace --run-ignored all

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
    {{cargo}} xtask lint

# Local CI replica.
ci:
    {{cargo}} xtask ci

# ----- cross-compile checks -----

# Compile-only check that Windows code still builds from Linux dev container.
cross-check:
    {{cargo}} xwin check --workspace --target x86_64-pc-windows-msvc

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
    {{sh}} "npm install --no-audit --no-fund"

# ----- lefthook delegated recipes (do not run directly) -----

_hook-fmt files:
    {{cargo}} fmt -- {{files}}

_hook-typos-fix files:
    {{typos}} --write-changes {{files}}

_hook-taplo-fmt files:
    {{taplo}} fmt {{files}}

_hook-cargo-sort:
    {{cargo}} sort --workspace

_hook-biome-format files:
    {{biome}} format --write {{files}}

_hook-yamlfmt files:
    {{yamlfmt}} {{files}}

_hook-actionlint files:
    {{actionlint}} {{files}}

_hook-xtask-dep-graph:
    {{cargo}} xtask dep-graph

_hook-docs-drift:
    just docs
    {{sh}} "git diff --quiet docs/ README.md || (echo 'docs drift detected — run \\`just docs\\` and commit' >&2; exit 1)"

_hook-commitlint msg_path:
    {{npx}} -- commitlint --edit {{msg_path}}
