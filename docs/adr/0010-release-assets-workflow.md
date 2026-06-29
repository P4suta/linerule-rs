# 0010 — Release artifact auto-attach via `release-assets.yml`

**Status:** Superseded by [[0014-immutable-release-asset-flow]] (2026-05-24). Accepted (2026-05-20). Dropped PDB / `-debug` attachment; release-profile binary only. Naming convention (`linerule-vX.Y.Z-win-x64.exe`) retained.

**See also:** [[0011-phase-j-slim-down]], [[0014-immutable-release-asset-flow]] (current design).

## Context

`release-please-action` automates version bump + CHANGELOG + tag + Release creation, but not binary asset attach. Users had to dig through per-run CI artifacts (expire in 90 days) to verify a release.

## Decision

Add new workflow `.github/workflows/release-assets.yml`. On `release: types: [published]`, build the release profile and attach via `gh release upload --clobber`.

### Trigger design

```yaml
on:
  release:
    types: [published]
  workflow_dispatch:
    inputs:
      tag:
        description: "Release tag to attach assets to (e.g. v0.2.13)"
        required: true
```

- `release: types: [published]` — release-please release PR merge → tag push + Release creation → triggers this workflow.
- `workflow_dispatch (inputs.tag)` — retroactive attach to past releases, and manual retry on build failure.
- `[published]` not `[created]`: excludes draft releases (insurance against a future draft→publish 2-phase flow).

### Build strategy

```yaml
- run: cargo build --release -p linerule-app
```

### Naming convention

```
linerule-vX.Y.Z-win-x64.exe          (release profile: stripped, panic=abort)
```

- Embed version in the filename so the version is known after download.
- Embed platform/arch so future linux builds etc. can sit alongside (this ADR: Windows x64 only).

### `--clobber` flag

Overwrites a same-named asset, making `workflow_dispatch` retry and the `release: published` race idempotent.

### Branch protection decision

Not added. It triggers on the `release: published` event and never runs during a PR, so making it a required check would leave PRs pending forever.

## Consequences

- New `.github/workflows/release-assets.yml` (~70 LOC).
- From the next tag push on, the binary asset is auto-attached to Releases/latest.

## Alternatives considered

- **A. release-please `extra-files`** — rejected: only bumps source versions, no asset upload.
- **B. Extend ci.yml build job to the release event** — rejected: branching gets complex. A dedicated release workflow has clearer responsibility.
- **C. Attach CI per-run artifact (no rebuild)** — rejected: requires mechanically identifying the latest successful run. Rebuild is simple at ~6 min with `rust-cache`.
- **D. crates.io publish** — rejected: this repo is an app crate, not a library.

## Related

- ADR-0007 — Debug build profile (`dist-dev`) and the panic-strategy asymmetry.
- ADR-0014 — asset flow redesign for immutable releases (current design).
