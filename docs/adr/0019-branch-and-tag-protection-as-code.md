# 0019 — Branch/tag protection as rulesets and a `ci-required` aggregate gate

**Status:** Accepted (2026-06-30).

**See also:** [[0014-immutable-release-asset-flow]] / [[0017-release-signing-and-attestation]] /
[[0018-build-channel-separation]]. Runbook: `.github/rulesets/README.md`, docs/SUPPLY_CHAIN.md.

## Context

Branch protection lives only in the GitHub UI — outside review, history, and diffs, with no restore template. Listing
required status checks by individual job name in the UI desyncs every time a CI job is added or renamed. Port the
find-my-files approach (`.github/rulesets/*.json` as source of truth, required checks behind one aggregate job).

## Decision

**Encode branch/tag protection in `.github/rulesets/*.json` and collapse required status checks into a single
`ci-required` job.**

### `ci-required` aggregate gate (`ci.yml`)

- Aggregate job with every job in `needs:`. Scans `join(needs.*.result)` and fails on `failure`/`cancelled`.
- `if: always()` so it runs even when upstream fails. **`skipped` counts as pass** — `dependency-review` and
  `conventional-commits` are PR-only (skipped on `merge_group`/`push`), so without skip=pass the merge queue waits
  forever.
- The ruleset requires only the single `ci-required` context, so branch protection never desyncs as jobs come and go.
  No third-party action needed.

### `.github/rulesets/` (3 files)

| File | target | Main rules |
|---|---|---|
| `protect-default-branch.json` | `main` | no deletion / non_fast_forward / required_linear_history / pull_request (0 approvals, thread resolution required, **squash only**) / required_status_checks (strict, `ci-required`) |
| `require-signed-commits.json` | `~ALL` (except gh-pages) | required_signatures |
| `protect-release-tags.json` | `refs/tags/v*` | deletion / non_fast_forward / update (immutable published tags) |

Field rationale:

- **squash only**: mechanizes CONTRIBUTING's squash-merge-only rule.
- **0 approvals**: solo maintainer (CODEOWNERS `* @P4suta`). `required_review_thread_resolution: true` blocks merge
  with unresolved comments.
- **signatures required**: the commit side of supply-chain hardening ([[0017-release-signing-and-attestation]]).
  `gh-pages` is excluded for template parity (this repo deploys Pages via artifact, so there is no gh-pages branch).
- **immutable tags**: makes published `v*` undeletable/unmovable, securing [[0014-immutable-release-asset-flow]] from
  the ref side too.
- **tag creation is policy, not a rule**: `v*` is pushed by release-please via `GITHUB_TOKEN`, so a hard `creation`
  rule would block release-please. Leave it out and state the policy in prose instead.

### Source of truth and DR

GitHub does not auto-apply in-tree repo rulesets (only org-level can be imported). These JSON files are both source of
truth and DR template, imported via `gh api`. After an incident, re-running the import restores them.

### Migration (drop classic last)

1. Merge the `ci-required` job and 3 JSON files first.
2. On a throwaway PR, confirm the `ci-required` context is actually reported.
3. Import the 3 files via `gh api --method POST .../rulesets --input <file>`.
4. On a test PR, confirm squash merge passes and unsigned push / tag deletion are rejected (rulesets and classic are
   additive — strictest wins).
5. **Only after confirming**, delete the classic UI branch protection
   (`gh api --method DELETE .../branches/main/protection`). Never leave `main` unprotected for a moment.

**Prerequisite**: `required_signatures` on `~ALL` blocks unsigned pushes. Before import, set up signing (GPG/SSH/gitsign)
or GitHub UI merges (web-flow signature).

## Impact

| Item | Before | After (this ADR) |
|---|---|---|
| branch protection | UI only, out of history | `.github/rulesets/*.json` (reviewable, DR template) |
| required checks | per-job list in UI (desync source) | single `ci-required` |
| tag protection | none | `v*` immutable (no delete/move) |
| signed commits | optional | required on `~ALL` |
| merge queue | (skip handling undefined with per-job checks) | stable via `ci-required` skip=pass |

## Verification

- Static: `actionlint` checks `ci-required`. The 3 JSON files parse under the `gh api` schema (`jq -e .`), biome JSON
  format green.
- Dynamic (migration steps 2–5): throwaway PR shows `ci-required` → import → `gh api repos/P4suta/linerule-rs/rulesets`
  shows 3 active rulesets → test PR: squash green, unsigned push rejected, `v*` tag deletion rejected → delete classic
  protection.
- `ci-required` goes green even on a docs-only PR (some jobs skipped).

## Open questions / Followup

- **Tag `update` rule**: if import rejects `{ "type": "update" }`, reduce to `deletion` + `non_fast_forward`.
- If CodeQL (`analyze`) is added later, add its context to required_status_checks in `protect-default-branch.json`.
- bypass_actors is empty (no exceptions, admins included). If emergency temporary bypass is ever needed, document it
  separately.
