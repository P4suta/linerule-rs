# Contributing to linerule

Thanks for considering a contribution. The bar this project sets is
intentionally high; please read this in full before opening a PR.

## Quickstart

```sh
git clone https://github.com/P4suta/linerule
cd linerule
just hooks            # install lefthook (pre-commit / commit-msg / pre-push)
just build            # build the workspace inside Docker
just test             # run the test suite
just ci               # local replica of GHA pipeline
```

The host needs Docker + `just` + git. No host cargo required (and no
host cargo invocation is permitted in CI / hooks — see
[ADR-0005](docs/adr/0005-docker-only-execution.md)).

## North-star principle: architectural beauty

The most important goal of this project is the architectural shape of
the code. Concretely:

- Prefer ADT decomposition over flat product structs when the domain
  is even mildly composable.
- Match the vocabulary of the chosen rendering stack at the core
  layer too.
- Encode invariants in types (validating newtypes, phantom coords,
  RAII tokens).
- `#[non_exhaustive]` is a load-bearing decision, not boilerplate.

Pragmatic shortcuts ("we'll generalize later") are *explicitly* a
non-goal.

## Hard rules

These are enforced by `cargo run -p xtask -- strict-code` (part of
`just lint` and the lefthook pre-commit hook). The rule list lives in
`crates/xtask/src/strict_code.rs`; this prose is the explanation.

- **No `#[allow(...)]` attributes.** Use `#[expect(lint, reason = "...")]`
  (Rust 1.81+) when a lint genuinely needs silencing — `expect`
  self-removes when the issue is fixed. See
  [ADR-0006](docs/adr/0006-workspace-lints-single-source.md).
- **No nightly toolchain features (`#![feature(...)]`).** Stable only.
- **No `unsafe` in `linerule-core` / `linerule-config`.** They are
  `#![forbid(unsafe_code)]`. The `linerule-platform` crate may use
  `unsafe` at the FFI boundary but every block requires a preceding
  `// SAFETY:` comment.
- **No bare `TODO` / `FIXME` / `XXX`.** Each must reference an issue
  (`#N`), milestone (`M1..M4`), or ADR (`ADR-NNNN`).
- **No `println!` / `eprintln!` in library crates.** Use `tracing`.
- **No `on.schedule:` workflow triggers.** Dependabot is the only
  scheduled job allowed in this repo (see
  [feedback_no_cron_in_repos](https://github.com/P4suta) project rule).
- **No `continue-on-error: true` in CI.**

## Per-crate `[lints]` overrides are forbidden

`[workspace.lints]` in the workspace `Cargo.toml` is the single source
of truth (ADR-0006). Cargo silently overrides workspace carve-outs
when you mix per-crate `[lints.clippy]` (cargo#12697); per-crate
exceptions go through the strict-code grep gate.

## Commit messages

Conventional Commits, enforced by `committed` via the lefthook
commit-msg hook:

```
type(scope?): subject

body (optional, wrapped at 72)
```

`type` is one of `feat / fix / docs / style / refactor / perf / test
/ build / ci / chore / revert`. Subject is lowercase, imperative,
under 50 chars, no trailing period.

Releases are automated via release-plz: merge a feat/fix/perf/refactor
commit on main → release-plz opens a Release PR with the version bump
and `CHANGELOG.md` update; merging that PR cuts the tag and triggers
cargo-dist.

## Testing

```sh
just test            # all targets, nextest
just test-doc        # doctests
just prop            # bolero in proptest mode
just snap            # cargo-insta snapshot review
just coverage        # branch coverage, fail-under 100% on pure crates
```

C1 100% branch coverage is enforced on `linerule-core` and
`linerule-config`. The platform layer's bare Win32 FFI lines are
explicitly excluded (boundary). When you add a public API in a pure
crate, add unit + property tests covering every Mode×Action cell or
input partition.

## ADRs

Substantial design decisions land an ADR in `docs/adr/` *before* the
code that implements them. The MADR template at
`docs/adr/0000-template.md` is the starting point. ADRs are
append-only — outdated ones are marked `superseded by ADR-XXXX` rather
than deleted.

## License

By contributing you agree your contribution is dual-licensed under
[Apache-2.0](LICENSE-APACHE) and [MIT](LICENSE-MIT) at the user's
choice.
