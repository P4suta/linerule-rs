# Mutation Testing Baseline (`cargo-mutants`)

## Scope

`cargo-mutants` is run only against `linerule-core`. Scope rationale and
gate policy live in [ADR-0004](adr/0004-coverage-policy.md).

## How to run

Locally (inside the dev container):

```sh
just shell    # opens the dev container
cargo mutants --package linerule-core --baseline skip --no-times \
  --output target/mutants --test-tool nextest
```

In CI: `.github/workflows/mutants.yml` runs the same command on every PR
that touches `crates/linerule-core/**`, plus on manual `workflow_dispatch`.
Reports are uploaded as the `mutants-report` artifact.

## Reading the report

After a run, `target/mutants/` contains:

- `mutants.out/missed.txt` — mutants the tests did **not** kill (the
  meaningful list). Anything new here is a test gap.
- `mutants.out/caught.txt` — mutants the tests killed (the good list).
- `mutants.out/unviable.txt` — mutants that failed to compile (not a gap,
  just code that cargo-mutants couldn't perturb).
- `mutants.out/timeout.txt` — tests that timed out under the mutant.

## Baseline (TBD)

A first full run will land here once the workflow has been exercised at
least once. Until then, treat `missed.txt` as advisory: review every
entry on PRs that intentionally widen the missed set, and update this
file to record the new baseline.

Target kill ratio: **≥ 85%** of viable mutants. Anything lower indicates
the test suite leans on incidental coverage rather than asserting
behavioral invariants.
