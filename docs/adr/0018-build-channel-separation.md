# 0018 — Build channel separation (dev / nightly / stable) and version embedding

**Status:** Accepted (2026-06-30). Adds a layer on top of [[0017-release-signing-and-attestation]] stable distribution
to identify non-release builds. Branch/tag protection split out to [[0019-branch-and-tag-protection-as-code]].

**See also:** [[0011-phase-j-slim-down]] / [[0014-immutable-release-asset-flow]] /
[[0017-release-signing-and-attestation]]. Runbooks: docs/SUPPLY_CHAIN.md, CONTRIBUTING.md.

## Context

The stable pipeline is complete, but non-release builds can't identify themselves: `cargo build`, CI, and the shipped
EXE all report `linerule 0.4.1` and can be mistaken for stable. There's also no official home for nightly-style trial
builds. Port the dev / nightly / stable 3-channel layout from sibling find-my-files.

## Decision

**Identify the channel via the version string, embedded into the binary at build time.** Don't use GitHub's prerelease flag.

### Version format (`cargo xtask version --channel <dev|nightly|stable> [--date YYYYMMDD]`)

| channel | format | use |
|---|---|---|
| stable | `X.Y.Z` | the release tag itself (clean) |
| dev | `X.Y.Z-dev+g<sha>` (no git → `-dev`, dirty → `.dirty`) | default for everyday `cargo build` |
| nightly | `X.Y.Z-nightly.<date>+g<sha>` (date strictly validated as 8 digits) | daily unsigned build |

- base `X.Y.Z` is `env!("CARGO_PKG_VERSION")` (= `[workspace.package].version`, bumped by release-please).
  Every crate inherits `version.workspace = true`, so no TOML parse needed.
- Single source of the format is the pure function `compute()` in `xtask/src/version.rs` (unit-tested, no git/FS access).

### Embed via a `linerule-app/build.rs` extension, not a separate crate

- `linerule-app` is the leaf of the dependency graph (`xtask dep-graph` enforces app → platform-windows → core), so
  a `.git/HEAD` change only rebuilds app — no reason to isolate it in a separate crate (find-my-files uses a separate crate).
- `build.rs` emits `LINERULE_VERSION` via `cargo:rustc-env`, received by `const VERSION = env!("LINERULE_VERSION")`
  in `src/version.rs`. Doesn't pollute `xtask dep-graph`, `docs/modules/`, or `docs/dep-graph.svg`.

### `LINERULE_VERSION` precedence (build.rs)

1. env `LINERULE_VERSION` (non-empty) → used as-is (CI sets it for stable/nightly).
2. `{CARGO_PKG_VERSION}-dev+g{short_sha}[.dirty]` (git reachable).
3. `{CARGO_PKG_VERSION}-dev` (git / `.git` absent).

Always emit one so `env!` compiles.

### Output paths

The `version` subcommand, clap-native `--version`/`-V`, and the boot banner all read `crate::version::VERSION`.
The custom `version` subcommand stays for backward compatibility.

### nightly = unsigned Actions artifact (`.github/workflows/nightly.yml`)

- Daily cron + `workflow_dispatch`. Skips if `git log --since='24 hours ago'` shows no change on main (dispatch bypasses).
- Release build, nightly stamp, the same Horizontal+Blur GUI smoke as CI, then uploads the unsigned exe + SHA256SUMS to a
  14-day Actions artifact (`linerule-nightly`). Fetch via `gh run download --name linerule-nightly`.
- Deliberately no SBOM/signing/attestation/tag/Release, to keep it clearly distinct from stable.

### `on: schedule` policy exception

`ci.yml` follows a "No `on: schedule`" policy. nightly.yml is the sole intentional schedule (dedicated to the daily
product-binary build). The CI body itself has no schedule.

## Consequences

| item | Before | After (this ADR) |
|---|---|---|
| dev/CI/shipped version string | all `0.4.1` | dev=`-dev+g<sha>` / nightly=`-nightly.<date>+g<sha>` / stable=`0.4.1` |
| embedding mechanism | `env!("CARGO_PKG_VERSION")` only | `build.rs` emits `LINERULE_VERSION`, via `src/version.rs` |
| `--version` flag | none (custom `version` only) | adds clap-native `--version`/`-V` (custom `version` kept too) |
| nightly distribution | none | unsigned Actions artifact (14-day, with checksum) |
| `release-assets.yml` | no version env | stamps `--channel stable` before build (see risk below) |
| schedule policy | no schedule in CI | nightly.yml is the sole exception |

## Validation

- `cargo test -p xtask`: 7 `compute()` unit tests green. Manually checked `cargo xtask version --channel {dev,nightly,stable}`
  (nightly requires `--date` with 8-digit validation; clap rejects unknown channels).
- After `cargo build -p linerule-app`, `linerule version` / `--version` / `-V` output `0.4.1-dev+g<sha>[.dirty]`.
  `cli_smoke.rs`, `boot.rs` green.
- nightly: `gh workflow run nightly.yml` → `gh run download --name linerule-nightly` → exe reports
  `-nightly.<date>+g<sha>` and `sha256sum -c SHA256SUMS.txt` matches.
- stable regression: the next release (or `workflow_dispatch tag=main publish=false`) EXE reports a clean `X.Y.Z`
  with no `-dev`.

## Open questions / Followup

- **Top risk**: since `build.rs` stamps `-dev` by default, `release-assets.yml` must include a stamp step
  `LINERULE_VERSION=$(cargo xtask version --channel stable)`. Otherwise the shipped EXE reports `-dev` and the supply-chain
  invariant (self-reported version = tag) breaks. Add it before the first release.
- release-please version sync needs no extra wiring (workspace version bump → all crates inherit).
- nightly cron time (04:00 UTC) and 14-day retention are provisional.
