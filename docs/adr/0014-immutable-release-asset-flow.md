# 0014 — Immutable release asset flow (draft → upload → publish)

**Status:** Accepted (2026-05-24). Supersedes the trigger design of [[0010-release-assets-workflow]]. Naming convention, SBOM attachment, and build strategy inherited from ADR-0010.

**Note:** `release-please-config.json`'s `"skip-github-release": true` skips not only the Release object but also the tag push (`release-please-action@v5`). Without a tag, release-please gets stuck with `untagged, merged release PRs outstanding - aborting`, so `release-please.yml` needs a helper step that runs `git tag` + `git push` itself when no tag was pushed.

## Context

Immutable releases (GA 2025-10-28) are ON in this repo (owner policy, 2026-05-24). Adding, changing, or deleting assets on a published release is forbidden with 422. Old ADR-0010 added assets after publish via `gh release upload`, so under immutable that upload always fails with 422.

## Decision

Keep immutable ON, switch release-assets.yml to a `push: tags: ["v*"]` trigger, and assemble the release + assets in 3 steps: draft create → upload → publish. The draft is mutable; the immutable lock applies the moment publish (`draft=false`) happens, so staging all assets before publish is the only safe way to attach.

### Workflow coordination

```
release-please.yml                 release-assets.yml
─────────────────────              ─────────────────────────────────────
on: push: branches: [main]         on: push: tags: ["v*"]
                                       workflow_dispatch (inputs.tag)
↓                                  ↓
release-please-action              gh release create $tag --draft --generate-notes
  └ skip-github-release: true      gh release upload $tag <files> --clobber
  └ push the tag (no release)      gh release edit $tag --draft=false --latest
```

Add `"skip-github-release": true` to `release-please-config.json`: release-please handles CHANGELOG / version bump / tag push, and the Release page generation is delegated to release-assets.yml.

### Release notes

`gh release create --generate-notes` auto-generates PR titles from the commit range. Precise CHANGELOG section extraction is not adopted (avoids maintaining an awk script, [[0011-phase-j-slim-down]]).

### Idempotency / manual retry

Probe `gh release view $tag --json isDraft` at the start of the job:

- absent → `gh release create $tag --draft`
- present + draft → reuse (idempotent via `--clobber` upload)
- present + published → stop with error (immutable lock already applied; needs a different tag or delete + retag)

`workflow_dispatch (inputs.tag)` is used for manual retry on build failure.

### Relation to the token problem

This workflow uses `push: tags` (an event that always fires), so it avoids the problem where release-please's `secrets.GITHUB_TOKEN` does not trigger other workflows. Migrating the token to a PAT is out of scope for this ADR.

### Retroactive attach to existing releases (v0.2.0–v0.4.0)

Not done. Assets cannot be added to published releases, so past releases stay without assets. Distribution with assets starts from v0.4.1.

### Branch protection

release-assets is not a required check (a tag push event is not a PR check). On failure, manually delete the draft + tag, or wait for the next release-please PR.

### Naming convention (inherited from ADR-0010)

```
linerule-vX.Y.Z-win-x64.exe          (release profile: stripped, panic=abort)
linerule-vX.Y.Z-sbom.cdx.json        (CycloneDX 1.6 JSON)
```

## Consequences

| Item | Before (ADR-0010) | After (this ADR) |
|---|---|---|
| trigger | `release: [published]` | `push: tags: ["v*"]` |
| release creator | release-please-action | release-assets.yml's `gh release create --draft` |
| asset attach | after publish (fails, 422) | upload during draft (succeeds) |
| release-please-config | `skip-github-release` unset | add `skip-github-release: true` |
| immutable compatibility | ❌ not possible | ✅ spec-compliant |
| release notes source | release-please's CHANGELOG | `gh release ... --generate-notes` |

## Verification

Live verification on a release cycle merge: release-please PR merge → tag push → release-assets.yml creates draft → EXE + SBOM upload → publish → confirm 2 assets via `gh release view <tag>`. Rollback is `gh release delete <tag> --cleanup-tag`.

## Open questions / Followup

- The PR-check problem caused by release-please's `secrets.GITHUB_TOKEN` is not resolved by this ADR. Continue the interim practice of firing the release PR's `ci.yml` by close+reopen.
- If `--generate-notes` output is insufficient, switch to section extraction from CHANGELOG.md.
- Reconsider the `--latest` flag when running multiple main lines (`v0.4.x` and `v0.5.x` in parallel). Currently a single line.
