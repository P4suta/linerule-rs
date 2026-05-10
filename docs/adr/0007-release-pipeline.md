# 0007. Release pipeline: cargo-dist + release-plz with separated responsibilities

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: infra, release

## Context

linerule needs a reproducible release pipeline that:

1. Generates Win/Mac/Linux artifacts (Windows-only for v0.1; macOS /
   Linux land in v0.2 alongside ADR-0004's expansion).
2. Maintains a `CHANGELOG.md` from Conventional Commits.
3. Bumps SemVer based on commit type/scope without manual edits.
4. Publishes to GitHub Releases with checksums + installer scripts.

Two mainstream 2026 tools cover these concerns:

- [`cargo-dist`](https://opensource.axo.dev/cargo-dist/) generates the
  GHA release workflow + artifact / installer / checksum pipeline.
- [`release-plz`](https://release-plz.dev/) opens a Release PR each
  time a commit lands on the main branch, bumping versions and
  updating `CHANGELOG.md` from the Conventional Commits history.

## Decision

- **cargo-dist** owns artifact generation, installer scripts, GitHub
  Release creation, and checksums. Configured via
  `[workspace.metadata.dist]` in `Cargo.toml`. Tag push triggers it.
- **release-plz** owns version bumping (SemVer inferred from commit
  type) and `CHANGELOG.md` updates. It opens a Release PR — merging
  the PR triggers cargo-dist via the resulting tag.
- `cargo semver-checks` runs in CI to detect breaking-API drift before
  the bump lands.
- `git-cliff` is pulled in as release-plz's internal dependency (no
  separate `cliff.toml`).
- Conventional Commits are enforced at commit time by `committed` via
  the `lefthook` commit-msg hook, and again on PR titles via CI.

## Consequences

**Becomes easier**
- A normal contributor never edits `CHANGELOG.md` by hand. They write
  Conventional Commits; release-plz formats and ships them.
- The release workflow is generated, not hand-maintained — bumping
  cargo-dist regenerates it.
- CI can verify SemVer drift before the bump is merged.

**Becomes harder**
- Two tools to learn (cargo-dist, release-plz). Mitigated by the fact
  that both are 2026-mainstream and well-documented.
- `cargo-dist` and `release-plz` versions are pinned in Cargo.toml /
  release-plz.toml respectively; bumps are Dependabot-driven.

## Alternatives considered

- **Hand-maintained `CHANGELOG.md` + manual `git tag` + manual
  `gh release create`** — works for one or two releases; rotting
  thereafter.
- **`cargo-release` (legacy)** — older, less integrated with GHA.
- **Dependabot version bumps + manual release** — Dependabot is for
  dependencies, not the project's own version.
- **Pure git-cliff + manual release** — git-cliff produces the
  changelog body but doesn't open Release PRs or interpret SemVer
  intent. release-plz wraps git-cliff and adds those concerns.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- `release-plz.toml`
- `Cargo.toml` `[workspace.metadata.dist]`
- [cargo-dist](https://opensource.axo.dev/cargo-dist/)
- [release-plz](https://release-plz.dev/)
