# 0011. Lifecycle ADT: collapse `enabled: bool` + `mode: Mode` into `Active(Mode) | Paused(Mode)`

- Status: accepted
- Date: 2026-05-11
- Deciders: @P4suta
- Tags: architecture, types, state-machine

## Context

The first cut of `State` carried two flags that together described the
overlay's runtime status:

```rust
pub struct State {
    pub mode: Mode,        // Off | Bar | Mask | Vertical | VerticalMask
    pub enabled: bool,     // false ⇒ render empty frame
    pub config: OverlayConfig,
}
```

User feedback pinned the semantics of `Pause`: it should *temporarily
silence the entire overlay* without losing the user's mode selection
or the live config. The two-flag representation accomplishes this
correctly, but it admits states the system doesn't actually use:

| `enabled` | `mode`         | meaning                                           |
|-----------|----------------|---------------------------------------------------|
| `true`    | `Bar`          | drawing a horizontal bar — *intended*             |
| `false`   | `Bar`          | paused, "remembering" Bar — *intended*            |
| `true`    | `Off`          | drawing nothing — *intended* (cycle position)     |
| `false`   | `Off`          | paused, remembering Off — **structurally same**   |

The last two render-output-equivalent rows are not the same thing
semantically: cycling out of `Off` differs from resuming from
`Paused(Off)`. Carrying that distinction in two flags forced every
consumer to remember which one to consult.

## Decision

Replace the two flags with a single sum type:

```rust
#[non_exhaustive]
pub enum Lifecycle {
    Active(Mode),
    Paused(Mode),
}

pub struct State {
    pub lifecycle: Lifecycle,
    pub config: OverlayConfig,
}
```

`State.lifecycle.mode()` reads through to the inner mode regardless
of pause state. `Action::CycleMode` advances the inner mode while
preserving the active/paused tag (`Lifecycle::with_mode`).
`Action::TogglePause` flips the tag while preserving the inner mode
(`Lifecycle::toggled_pause`). `render` is dispatched only when the
tag is `Active`; the platform layer short-circuits to
`OverlayFrame::empty()` for `Paused(_)` (and, by `if let`
fall-through, any future variant added under `#[non_exhaustive]`).

The `mode: Off` cycle position remains structurally distinct from
the `Paused(_)` lifecycle, which preserves the user-visible
distinction the two-flag design also encoded — but now without
unreachable states.

## Consequences

**Becomes easier**
- Every consumer reads `state.lifecycle.mode()` once; no
  "which-flag-wins" branching at call sites.
- `StateDelta.lifecycle: Option<Lifecycle>` collapses two prior
  `Option` fields (`mode`, `enabled`) into one — the diff vocabulary
  matches the state vocabulary 1:1.
- Adding a third lifecycle variant in the future (e.g. `Loading(_)`,
  `Suspended(_)` for a battery-saver mode) is purely additive thanks
  to `#[non_exhaustive]` + the `if let Active(_)` fall-through in
  `repaint`.

**Becomes harder**
- A consumer that wanted "is the overlay rendering anything right
  now?" must check `lifecycle.is_active() && mode != Mode::Off`
  rather than just `enabled`. In practice nobody asks this question
  outside `repaint`, which already short-circuits structurally.

## Alternatives considered

- **Keep `enabled: bool`, drop `paused`** (which the previous design
  had as a third independent flag). Rejected: the boolean still
  invites the same "which flag wins" discipline at every call site
  and pushes the active/paused distinction into prose comments
  rather than the type system.

- **Type-state on the `OverlayApp` struct** (`OverlayApp<Active>` /
  `OverlayApp<Paused>`). Rejected: the `OverlayApp` lives inside
  winit's event loop and the loop owns it concretely; type-state on
  the surrounding struct would just be a wrapper around an internal
  enum, with the `state.lifecycle = ...` assignment becoming a
  `replace` of the wrapper instead.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- `crates/linerule-core/src/lib.rs` (Lifecycle / State / reduce)
- `crates/linerule-platform/src/windows.rs` (repaint)
- `feedback_linerule_pause_semantics` (auto-memory)
