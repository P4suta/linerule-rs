# Contributing

Contributions to linerule-rs are welcome. See [.github/CODE_OF_CONDUCT.md](.github/CODE_OF_CONDUCT.md) for the code of conduct.

Before structural changes, review the fixed design rules (one-way dependencies, RAII, exhaustive match, localized `unsafe` — all merge blockers) in [`docs/adr/0002-architecture-principles.md`](docs/adr/0002-architecture-principles.md).

## Setup

The toolchain is pinned via [mise](https://mise.jdx.dev/) (`mise.toml`); tasks run through [just](https://github.com/casey/just). Two paths exist — **Docker** and **native Windows** — and `just` auto-detects. See the README [Quickstart](README.md#quick-start) for prerequisites.

```
mise install        # install missing tools (native path)
just bootstrap      # one-shot setup (auto-detects docker / native)
just doctor         # check environment matches pins (native: just doctor-native)
```

Declare tools in `mise.toml` and install via `mise install`; do not add them ad hoc.

## Dev loop

```
just lint     # fmt + clippy + cargo-deny + typos + actionlint + cargo-machete + dep-graph
just test     # cargo nextest (falls back to test-threads=1 if absent)
just docs     # regenerate artifacts (cargo-rdme / cargo modules / cargo depgraph)
```

GUI verification works **only on native Windows** (overlay rendering is impossible in a Linux container).

```
just run                # launch the overlay for visual check
just verify             # GUI smoke: brief launch, health judged from events.jsonl
just verify-blur        # verify WinRT backdrop-blur with Horizontal + Blur
just verify-scenario    # inject Ctrl+Alt chord via SendInput, assert state transitions
```

`just verify` shares the same judgment logic (`cargo xtask verify`) as the CI release build.

## Commit / PR rules

- [Conventional Commits](https://www.conventionalcommits.org/) (`feat:` / `fix:` / `perf:` / `docs:` / `refactor:` / `test:` / `chore:` / `ci:` / `deps:`). commitlint (`commitlint.config.mjs`) lints every commit in a PR.
- **Squash-merge only.** Commits other than `Merge X into Y` form require Conventional Commits (a colon-prefixed first line like `Merge origin/main: <desc>` is parsed as conventional and fails).
- Releases are cut by [release-please](https://github.com/googleapis/release-please), not a bot: it bumps the CHANGELOG from conventional commits and tags (`.github/workflows/release-please.yml`). On tag push, `release-assets.yml` attaches `linerule-vX.Y.Z-win-x64.exe` and the SBOM to the GitHub Release.

## Before pushing

- `just lint` and `just test` green; run `just verify` too if you have hardware.
- After CLI / module-layout / dependency-graph changes, sync artifacts with `just docs` and commit. The `lefthook` pre-commit detects drift.
- **Do not hand-edit generated artifacts**: the README `cargo-rdme` block (source of truth is the crate-level doc in `crates/linerule-core/src/lib.rs`), `docs/modules/`, `docs/dep-graph.svg`. Edit the source (doc comments or code) and regenerate.
- Do not bypass the pre-push hook with `--no-verify`. If it fails, fix the cause.

## Scope

**Reading-ruler overlay only, Windows only.** Cross-platform support and features beyond a reading ruler are non-goals. Before proposing a feature, read out-of-scope in the [feature_request template](.github/ISSUE_TEMPLATE/feature_request.yml); before architecture changes, read the relevant ADR in [`docs/adr/`](docs/adr/).

## License

Contributions are accepted under the project's dual MIT / Apache-2.0 license ([`LICENSE-MIT`](LICENSE-MIT) / [`LICENSE-APACHE`](LICENSE-APACHE)).
