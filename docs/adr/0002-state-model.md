# 0002. State model: runtime enum + phantom-typed coordinate spaces + RAII capability tokens

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: architecture, types

## Context

The overlay supports five user-visible modes — `Off` plus the four
elements of the (Shape × Orientation) lattice — cycled in a closed
permutation by the `CycleMode` action. A natural question: encode the
modes as a type-state machine (`Overlay<Off> → Overlay<Bar> → ...`)
or as a runtime enum?

The two-axis structure of the modes itself is decided in **ADR-0012**;
this ADR pins the runtime-enum-vs-type-state choice and the orthogonal
*lifecycle* (active vs paused) shape decided in **ADR-0011**.

The codebase also deals with two distinct coordinate systems —
*logical* (DPI-independent) and *physical* (raw pixel) — that the
compiler should refuse to mix. And it deals with OS-level hotkey
registrations whose lifetime must be paired to a Rust value to avoid
leaks (originally via the `HotkeyToken` capability — see
**ADR-0010** for why that surface was scrapped after the trait
abstraction was deemed dead).

## Decision

- **Mode**: runtime enum `Off | Active(Shape, Orientation)` with
  exhaustive `match` in `cycle` and `render`. NOT type-state. The
  direct-product decomposition itself is the subject of ADR-0012;
  this ADR records that it lives at runtime, not in the type
  parameters.
- **Lifecycle**: `enum Lifecycle { Active(Mode), Paused(Mode) }`
  carried on `State`. `Action::TogglePause` flips the tag without
  losing the inner `Mode`. The platform layer short-circuits to
  `OverlayFrame::empty()` whenever `lifecycle != Active(_)`.
  Decided in ADR-0011.
- **Coordinate spaces**: phantom-typed `Point<S>` / `ScreenRect<S>`
  where `S = Logical | Physical`. The compiler refuses to pass a
  physical point where a logical one is expected (and vice versa).
- **Hotkey registrations**: the original RAII `HotkeyToken` was part
  of the trait surface scrapped by ADR-0010. The production Windows
  path now owns `GlobalHotKeyManager` directly inside `windows::run`
  and drops it when the event loop returns; OS lifetime is bound to
  the function's stack frame.

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
- The `Paused(_)` arm is render-output-equivalent to `Active(Off)` —
  both produce an empty frame. The two are *semantically* distinct
  (cycle navigation vs lifecycle suspension) and the test suite pins
  both behaviours explicitly so the design invariant is documented
  in code.

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
