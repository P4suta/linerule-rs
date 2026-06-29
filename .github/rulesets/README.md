# Branch & tag rulesets

linerule-rs protects `main` and release tags with **version-controlled GitHub
rulesets** rather than the classic, UI-only branch-protection settings. These
JSON files are the canonical record and the disaster-recovery template.

> **GitHub does _not_ auto-apply repository rulesets from files in the tree**
> (only org-level rulesets can be imported automatically). The files here are
> source-of-truth; you apply them with `gh api` (below). To restore protection
> after any incident, re-run the import commands.

## Files

| File | Target | Effect |
| --- | --- | --- |
| `protect-default-branch.json` | `main` | No deletion / force-push, linear history, PR required (0 approvals, threads must resolve, **squash-only**), one required check: `ci-required`. |
| `require-signed-commits.json` | all branches except `gh-pages` | Every commit must be cryptographically signed. |
| `protect-release-tags.json` | `refs/tags/v*` | Published release tags are immutable (no delete / move / force-update). |

The single required status check is **`ci-required`** — the aggregation job in
`ci.yml` that gates every other job. Referencing one context (instead of
enumerating all jobs) means adding or renaming a CI job never desyncs the
ruleset.

## Prerequisite — signed commits

`require-signed-commits.json` rejects unsigned pushes to `main`. Before applying
it, set up commit signing (GPG / SSH / gitsign) or merge via the GitHub UI
(web-flow commits are signed by GitHub). Otherwise your first post-enable push
is blocked.

## Apply / verify / remove (migration runbook)

`gh` has no `ruleset import`; use the REST API. Full rationale and sequencing
are in [`docs/adr/0019-branch-and-tag-protection-as-code.md`](../../docs/adr/0019-branch-and-tag-protection-as-code.md).

```sh
# 1. ci-required must already report on a PR (open a throwaway PR to confirm the
#    context name exists) before the ruleset can require it.

# 2. Import (idempotent create) — one POST per file.
gh api --method POST -H "Accept: application/vnd.github+json" \
  repos/P4suta/linerule-rs/rulesets --input .github/rulesets/protect-default-branch.json
gh api --method POST -H "Accept: application/vnd.github+json" \
  repos/P4suta/linerule-rs/rulesets --input .github/rulesets/require-signed-commits.json
gh api --method POST -H "Accept: application/vnd.github+json" \
  repos/P4suta/linerule-rs/rulesets --input .github/rulesets/protect-release-tags.json

# 3. List / inspect.
gh api repos/P4suta/linerule-rs/rulesets

# 4. Update an existing ruleset after editing a file (need its id from step 3).
gh api --method PUT repos/P4suta/linerule-rs/rulesets/<id> \
  --input .github/rulesets/protect-default-branch.json

# 5. ONLY after the rulesets are confirmed active and ci-required is green,
#    remove the legacy classic protection so main is never momentarily exposed.
gh api --method DELETE repos/P4suta/linerule-rs/branches/main/protection
```

To refresh a file from the live ruleset (e.g. after a UI tweak), re-dump and
strip server-only metadata:

```sh
gh api repos/P4suta/linerule-rs/rulesets/<id> \
  --jq 'del(.id,.node_id,.created_at,.updated_at,._links,.current_user_can_bypass,.source,.source_type)'
```
