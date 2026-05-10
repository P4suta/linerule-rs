# 0012. Mode = Off | Mask(Orientation) — orientation-parametric typoscope

- Status: amended 2026-05-11 (originally proposed `Active(Shape, Orientation)`)
- Date: 2026-05-11
- Deciders: @P4suta
- Tags: architecture, types, render

> **Amendment.** The original decision below proposed
> `Mode = Off | Active(Shape × Orientation)` with `Shape ∈ {Bar, Mask}`.
> User feedback after live testing was that the bar variant added no
> reading-aid value the mask did not already provide ("マスクで周りを
> 暗くするのがマジでいいからそれだけでいいかも"), so `Shape` was
> retired. The remaining structure — orientation-parametric mask — is
> the natural collapse: `Mode = Off | Mask(Orientation)`. The
> *axis-symmetric render pipeline* (project → slit → lift → paint)
> survives unchanged; it now dispatches on `Orientation` only.

## Context

The first cut of `Mode` enumerated five flat variants:

```rust
pub enum Mode {
    Off,
    Bar,           // single horizontal bar at cursor Y
    Mask,          // typoscope, slit at cursor Y
    Vertical,      // single vertical bar at cursor X
    VerticalMask,  // 縦書き typoscope, slit at cursor X
}
```

`render` dispatched on this enum into four functions —
`render_bar`, `render_mask`, `render_vertical`,
`render_vertical_mask` — that were nearly mirror images of each
other on the X / Y axis. Adding a new shape (e.g. a `Frame`
vignette) or a new orientation (e.g. a 45° diagonal for diagrams)
would force a Cartesian-product expansion: one new variant per
combination, plus per-combination render impls.

The structure under the names was a pure direct product:

| axis           | values                       |
|----------------|------------------------------|
| `Shape`        | `Bar`, `Mask`                |
| `Orientation`  | `Horizontal`, `Vertical`     |

with `Off` standing apart as the explicit "draw nothing" cycle
position.

## Decision

Decompose `Mode` along its two intrinsic axes:

```rust
#[non_exhaustive] pub enum Shape       { Bar, Mask }
#[non_exhaustive] pub enum Orientation { Horizontal, Vertical }

#[non_exhaustive]
pub enum Mode {
    Off,
    Active(Shape, Orientation),
}

impl Mode {
    pub const BAR:           Self = Self::Active(Shape::Bar,  Orientation::Horizontal);
    pub const MASK:          Self = Self::Active(Shape::Mask, Orientation::Horizontal);
    pub const VERTICAL_BAR:  Self = Self::Active(Shape::Bar,  Orientation::Vertical);
    pub const VERTICAL_MASK: Self = Self::Active(Shape::Mask, Orientation::Vertical);
}
```

`render` dispatches once on `Mode`, then once on `(shape,
orientation)` through a single axis-symmetric pipeline:

```text
       project       slit_span        lift          paint
mode  ─────────►  (cursor', span)  ─────►  Span1D  ─────►  rect  ─────►  Layer
       on axis      on primary       complement
                      axis           on Mask
```

There is **one** `render_active` function that handles all four
`(Shape, Orientation)` pairs. Adding a third orientation (e.g.
`Diagonal`) means: one variant in `Orientation`, one match arm in
`project` and `lift`, no changes to `render_active`.

`cycle` is a 5-element permutation:
`Off → BAR → MASK → VERTICAL_BAR → VERTICAL_MASK → Off`.
Property test `cycle⁵ ≡ id` enforces injectivity / period.

## Consequences

**Becomes easier**
- The four mirror-image render impls collapse to one. The X / Y
  symmetry of the geometry is now stated *in code* (via
  `Orientation`) instead of duplicated in two near-identical
  function bodies.
- `Span1D::project` / `Span1D::lift` form a tiny composable
  vocabulary that can grow into `Affine` later if vello-style
  transforms come back in v0.2 (currently unused — see ADR-0009).
- The `BAR` / `MASK` / `VERTICAL_BAR` / `VERTICAL_MASK`
  associated constants give a cheap, descriptive call surface
  (`Mode::BAR` reads exactly like the old `Mode::Bar` did) without
  fixing the structure to those four points.

**Becomes harder**
- `Mode` no longer has a flat name per drawn variant. Tests that
  want to assert "is this the bar mode?" must say `Mode::BAR` or
  `Mode::Active(Shape::Bar, _)` rather than the unconditional
  `Mode::Bar` they used to. The associated constants make this a
  near-zero cost in practice.
- `serde` no longer round-trips through `"vertical_mask"`-style
  flat strings; the `Active` arm becomes a tuple variant. No user
  config consumed `Mode` directly (the binary's first-launch
  policy promotes `Off → MASK` programmatically), so this is an
  internal-only contract change.

## Alternatives considered

- **Keep flat `Mode`, factor out a private 1-D `slit` helper.**
  Rejected: the flat enum is what *forces* the duplication; lifting
  the helper without lifting the type still leaves four parallel
  arms in `render` and four parallel snapshot blocks in tests.
  The decomposition has to be in the type, not the helper.

- **Make `Mode` a struct `{ shape: Option<Shape>, orientation:
  Orientation }` with `None` shape == `Off`.** Rejected: the `Off`
  position has no orientation, so `Orientation` becomes vestigial in
  one variant; `enum` is the right shape for "either nothing or a
  pair" (= `Option<(Shape, Orientation)>` in essence, but the
  named-variant form reads better at every callsite).

- **Separate top-level enums for "horizontal modes" vs "vertical
  modes".** Rejected: the orientation axis is genuinely independent
  of the shape axis; encoding them as parallel sum types loses the
  symmetry that motivates the refactor in the first place.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- `crates/linerule-core/src/lib.rs` (Mode / Shape / Orientation /
  cycle / render / Span1D / project / lift / render_active)
- ADR-0002 (state model — five-mode cycle)
- `feedback_linerule_beauty_paramount` (auto-memory)
