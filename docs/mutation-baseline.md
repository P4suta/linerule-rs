# Mutation Testing Baseline (`cargo-mutants`)

## Scope

`cargo-mutants` is run only against `linerule-core`. Scope rationale and
gate policy live in [ADR-0004](adr/0004-coverage-policy.md).

## How to run

`cargo-mutants` is a **local-only** check. There is no CI workflow that runs it
automatically — the gate is "the developer touching `linerule-core` runs it
before they push". The wall time is ~12 minutes per full run, which is too
much per-PR cost when most PRs land 0 new mutations.

Locally (inside the dev container):

```sh
just shell    # opens the dev container
cargo mutants --package linerule-core --baseline skip --no-times \
  --output target/mutants --test-tool nextest
```

After the run, inspect `target/mutants/mutants.out/missed.txt`. Any line there
is a test gap to close (write a focused test) or, in rare equivalent-mutant
cases, to annotate with `// mutants: skip` plus a justification comment.

## Reading the report

After a run, `target/mutants/` contains:

- `mutants.out/missed.txt` — mutants the tests did **not** kill (the
  meaningful list). Anything new here is a test gap.
- `mutants.out/caught.txt` — mutants the tests killed (the good list).
- `mutants.out/unviable.txt` — mutants that failed to compile (not a gap,
  just code that cargo-mutants couldn't perturb).
- `mutants.out/timeout.txt` — tests that timed out under the mutant.

## Baseline (Phase ε — 2026-05-21)

First full local run on `feat/phase-epsilon-mutants-gate`:

| Outcome     | Count | Notes                                                          |
| ----------- | ----- | -------------------------------------------------------------- |
| **caught**  | 271   | killed by the test suite                                       |
| **missed**  | **0** | gate baseline (`mutants.yml` is now required, no `\|\| true`)  |
| **unviable**| 45    | compile-failure mutants (not a real test gap)                  |
| **timeout** | 0     | none                                                           |
| **total**   | 316   |                                                                |

Viable kill ratio: **271 / 271 = 100%** (well above the ≥ 85% target).

### How this baseline was established

The initial Phase ε run surfaced 47 missed mutants in 8 source files.
Rather than blanket-skip them (which would have been
"テストでコードの品質を保証・管理できていない" — exactly the situation
that triggered this work), each cluster was killed by a focused test:

| File                                | Missed → 0 via                                             |
| ----------------------------------- | ---------------------------------------------------------- |
| `render/hud_frame.rs` (22)          | row `origin_y` arithmetic pinned against `HudConfig::DEFAULT` |
| `color/perceptual.rs` (7)           | spot-value tests for `smooth(0.5)` / `l_star` segments     |
| `input/hud_fade.rs` (6)             | 1-px gap tests for `point_to_rect_distance`, fade-curve spot |
| `geometry.rs` (5)                   | `contains_rect` boundary cases (per-edge) + non-zero `top()` |
| `color/units.rs` (3)                | direct `get()` value assertions for `Opacity` / `DimLevel` |
| `input/chord.rs` (3)                | `parse_key("Left"/"Right")` + `Display::fmt` round-trip    |
| `input/tick.rs` (1)                 | `SetHudOpacity` emitted iff cursor position changed         |
| `state/reduce.rs` (1)               | `BumpOpacity` actually mutates the `opacity` field          |

### Maintenance rule

- New code that survives mutation testing **must be killed by a test**,
  not silenced with `// mutants: skip`. Use that annotation only when
  the mutant is provably equivalent (e.g. asserting a `1`-token offset
  in dead branches) — explain why in a comment on the annotation.
- The gate is **local, not CI-enforced**. When you change `linerule-core`,
  rerun `cargo mutants` before pushing. Update this file (and any
  `// mutants: skip` annotations) in the same PR that intentionally
  changes the baseline.
- Target kill ratio: **≥ 95%** of viable mutants. Current baseline is
  100%; allow a small buffer for unavoidable equivalences.
