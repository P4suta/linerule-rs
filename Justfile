# Minimal, shell-independent developer entry points for the mise environment.

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

export RUSTDOCFLAGS := "-D warnings"

default:
    mise exec just --command "just --list"

bootstrap:
    mise install
    mise --env coverage install
    mise exec lefthook --command "lefthook install"
    mise exec just --command "just doctor"

doctor:
    mise exec rust --command "rustc --version"
    mise exec rust --command "cargo --version"
    mise exec cargo:cargo-nextest --command "cargo nextest --version"
    mise exec cargo:cargo-deny --command "cargo deny --version"
    mise exec cargo:cargo-audit --command "cargo audit --version"
    mise exec cargo:cargo-llvm-cov --command "cargo llvm-cov --version"
    mise exec cargo:cargo-machete --command "cargo machete --version"
    mise exec cargo:cargo-mutants --command "cargo mutants --version"
    mise exec cargo:cargo-sort --command "cargo sort --version"
    mise exec cargo:cargo-sbom --command "cargo sbom --version"
    mise exec dotnet --command "dotnet --version"
    mise exec dotnet:CycloneDX --command "dotnet-CycloneDX --version"
    mise exec cyclonedx --command "cyclonedx --version"
    mise exec just --command "just --version"
    mise exec lefthook --command "lefthook version"
    mise exec taplo --command "taplo --version"
    mise exec biome --command "biome --version"
    mise exec yamlfmt --command "yamlfmt --version"
    mise exec actionlint --command "actionlint -version"
    mise exec typos --command "typos --version"
    mise exec pipx:reuse --command "reuse --version"

build:
    mise exec rust --command "cargo build --workspace --all-targets"

build-release:
    mise exec rust --command "cargo build --release --workspace"

b:
    mise exec rust --command "cargo build --workspace"

# cargo-nextest is the primary runner. Doctests stay separate because nextest
# intentionally does not execute them.
test:
    mise exec cargo:cargo-nextest --command "cargo nextest run --workspace --all-targets --no-fail-fast"
    mise exec rust --command "cargo test --doc --workspace"

t:
    mise exec cargo:cargo-nextest --command "cargo nextest run --workspace --all-targets --no-fail-fast"

doctest:
    mise exec rust --command "cargo test --doc --workspace"

# Deliberately parallel stock-runner gate: test isolation must not depend on
# nextest's process-per-test model.
test-cargo:
    mise exec rust --command "cargo test --workspace --all-targets"

test-windows:
    mise exec cargo:cargo-nextest --command "cargo nextest run --workspace --all-targets --run-ignored all --no-fail-fast"
    mise exec rust --command "cargo test --doc --workspace"
    mise exec rust --command "cargo test --workspace --all-targets"

mutants:
    mise exec cargo:cargo-mutants cargo:cargo-nextest --command "cargo mutants -p linerule-core --test-tool nextest --no-shuffle"
    mise exec python --command "python tools/check_mutants.py mutants.out/outcomes.json"

build-settings:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File ui/linerule-settings/BuildAndRun.ps1 -SkipRun

run-settings:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File ui/linerule-settings/BuildAndRun.ps1

test-settings-ui:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File ui/linerule-settings/UiTests.ps1

coverage:
    mise exec rust cargo:cargo-llvm-cov cargo:cargo-nextest --command "cargo llvm-cov nextest --workspace --exclude linerule-platform-windows --exclude xtask --json --summary-only --output-path target/coverage-lines.json --fail-under-lines 90 --fail-under-functions 90 --fail-under-regions 90"
    mise --env coverage exec cargo:cargo-llvm-cov cargo:cargo-nextest --command "cargo llvm-cov nextest --workspace --exclude linerule-platform-windows --exclude xtask --branch --no-cfg-coverage-nightly --json --summary-only --output-path target/coverage-branch.json"
    mise exec python --command "python tools/check_coverage.py target/coverage-branch.json branches 85"

run *args:
    mise exec rust --command "cargo run -p linerule-app -- {{args}}"

run-release *args:
    mise exec rust --command "cargo run --release -p linerule-app -- {{args}}"

fmt:
    mise exec rust --command "cargo fmt --all"
    mise exec cargo:cargo-sort --command "cargo sort --workspace"
    mise exec taplo --command "taplo fmt"
    mise exec biome --command "biome format --write ."
    mise exec yamlfmt --command "yamlfmt -gitignore_excludes ."

fmt-check:
    mise exec rust --command "cargo fmt --all -- --check"
    mise exec cargo:cargo-sort --command "cargo sort --workspace --check"
    mise exec taplo --command "taplo fmt --check"
    mise exec biome --command "biome format ."
    mise exec yamlfmt --command "yamlfmt --lint -gitignore_excludes ."

clippy:
    mise exec rust --command "cargo clippy --workspace --all-targets -- -D warnings"

rustdoc-check:
    mise exec rust --command "cargo doc --workspace --no-deps"

deny:
    mise exec cargo:cargo-deny --command "cargo deny check advisories bans licenses sources"

audit:
    mise exec cargo:cargo-audit --command "cargo audit --deny warnings"

reuse:
    mise exec pipx:reuse --command "reuse lint"
    mise exec pipx:reuse --command "reuse spdx --output linerule-source.spdx"

source-spdx output="linerule-source.spdx":
    mise exec pipx:reuse --command "reuse spdx --output {{output}}"

sbom output="dist/linerule-sbom.cdx.json":
    mise exec rust cargo:cargo-sbom dotnet dotnet:CycloneDX cyclonedx --command "powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/New-Sbom.ps1 -OutputFile {{output}}"

typos:
    mise exec typos --command "typos"

typos-fix:
    mise exec typos --command "typos --write-changes"

actionlint:
    mise exec actionlint --command "actionlint"

machete:
    mise exec cargo:cargo-machete --command "cargo machete"

dep-graph:
    mise exec rust --command "cargo xtask dep-graph"

policy:
    mise exec rust --command "cargo xtask policy"

release-check *args:
    mise exec rust --command "cargo xtask release-check {{args}}"

lint:
    mise exec rust cargo:cargo-sort cargo:cargo-deny cargo:cargo-machete taplo biome yamlfmt typos actionlint pipx:reuse --command "cargo xtask lint"

ci:
    mise exec rust cargo:cargo-nextest cargo:cargo-sort cargo:cargo-deny cargo:cargo-machete taplo biome yamlfmt typos actionlint pipx:reuse --command "cargo xtask ci"

version channel date="":
    mise exec rust --command "cargo xtask version --channel {{channel}} {{ if date == "" { "" } else { "--date " + date } }}"

cross-check:
    mise exec cargo:cargo-xwin --command "cargo xwin check --workspace --all-targets --target x86_64-pc-windows-msvc"

publish-windows-native:
    mise exec rust --command "cargo build --release -p linerule-app --target x86_64-pc-windows-msvc"

doc:
    mise exec rust --command "cargo doc --workspace --no-deps --open"

hooks:
    mise exec lefthook --command "lefthook install"

_hook-fmt *files:
    mise exec rust --command "cargo fmt --all"

_hook-cargo-sort:
    mise exec cargo:cargo-sort --command "cargo sort --workspace"

_hook-taplo-fmt *files:
    mise exec taplo --command "taplo fmt {{files}}"

_hook-biome-format *files:
    mise exec biome --command "biome format --write {{files}}"

_hook-yamlfmt *files:
    mise exec yamlfmt --command "yamlfmt {{files}}"

_hook-typos-fix *files:
    mise exec typos --command "typos --write-changes {{files}}"

_hook-actionlint *files:
    mise exec actionlint --command "actionlint {{files}}"

_hook-xtask-dep-graph:
    mise exec rust --command "cargo xtask dep-graph"
