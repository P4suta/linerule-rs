# 0002. State model: runtime enum + phantom-typed coordinate spaces + RAII capability tokens

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: architecture, types

## Context

The overlay supports five user-visible modes — `Off`, `Bar`, `Mask`,
`Vertical`, `VerticalMask` — cycled in a closed permutation by the
`CycleMode` action. A natural question: encode the modes as a
type-state machine (`Overlay<Off> → Overlay<Bar> → ...`) or as a
runtime enum?

Separately, the codebase deals with two distinct coordinate systems —
*logical* (DPI-independent) and *physical* (raw pixel) — that the
compiler should refuse to mix. And it deals with OS-level hotkey
registrations whose lifetime must be paired to a Rust value to avoid
leaks.

A third concern: orthogonal to *which* mode the overlay is in, the
user wants to *temporarily silence the entire overlay* without
forgetting their mode / config. We model this as a single `enabled:
bool` on `State` rather than carrying both `visible` and `paused`
flags; the previous two-flag design produced four states only two of
which were meaningful (`enabled & shown`, `disabled` — anything
else was indistinguishable in render output) and forced the platform
layer to consult both before rendering an empty frame.

## Decision

- **Mode**: runtime enum `{Off, Bar, Mask, Vertical, VerticalMask}`
  with exhaustive `match` in `cycle` and `render`. NOT type-state.
- **Pause / resume**: a single `State.enabled: bool`. While `false`,
  the platform layer renders `OverlayFrame::empty()` regardless of
  mode; the rest of `State` (mode, config, last-known cursor) is
  preserved so re-enabling snaps back to the exact prior visual.
  `Action::TogglePause` is the only verb that touches it.
- **Coordinate spaces**: phantom-typed `Point<S>` / `ScreenRect<S>`
  where `S = Logical | Physical`. The compiler refuses to pass a
  physical point where a logical one is expected (and vice versa).
- **Hotkey registrations**: RAII `HotkeyToken` type. Cannot be cloned
  or copied. Dropping it calls into the OS unregister path through an
  inner `Arc<dyn HotkeyRelease>`.

## Consequences

**Becomes easier**
- `render` and `cycle` exhaustively `match` five small variants —
  read in one screen, no type-state plumbing.
- Mixed-space bugs are a compile error.
- Hotkey leaks are structurally impossible — unregister-on-drop.
- Pause / resume preserves the rest of `State` for free; the only
  consumer is `repaint`, which short-circuits to an empty frame.

**Becomes harder**
- Mode transitions are not encoded in the type system. We rely on the
  `#[non_exhaustive]` attribute + exhaustive match to catch new
  variants, plus property tests (`cycle⁵ ≡ id`) to verify the cycle
  invariant.
- A `(enabled = false, mode = Bar)` state is representable but
  visually indistinguishable from `(enabled = false, mode = Mask)`
  — the test suite pins this collapse explicitly so the design
  invariant is documented in code.

## Alternatives considered

- **Type-state Mode** — `Overlay<Off> → Overlay<Bar> → ...`. Type-state
  earns its keep when each state offers a *different* operation set
  (`TcpStream<Connected>` vs `TcpStream<Listening>`). Here, every mode
  produces the same `OverlayFrame { rects: SmallVec<[_; 4]> }` shape and
  every action is uniformly applicable. Type-state would degrade into a
  thin wrapper around the same enum + 16 redundant `Transition<From, To>`
  variants with no genuine compile-time invariants.
- **Tagged union over `*const ()`** — `repr(C)` enum. No advantage; we
  don't ship across an FFI boundary in core.
- **Untyped i32 coordinates** — relies on review discipline; not
  acceptable per `feedback_competitive_rust_rigor`.
- **Hotkey ID as `u32`** — what `global-hotkey` itself uses internally.
  We wrap it in `HotkeyToken` so consumers cannot accidentally
  double-register or forget to unregister.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- `crates/linerule-core/src/lib.rs`
