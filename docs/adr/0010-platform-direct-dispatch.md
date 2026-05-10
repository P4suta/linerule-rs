# 0010. Platform-direct dispatch: drop the OverlaySurface / HotkeyHost / MouseTracker traits

- Status: accepted
- Date: 2026-05-11
- Deciders: @P4suta
- Tags: architecture, platform, supersedes-0003

## Context

ADR-0003 introduced three platform traits — `OverlaySurface`,
`HotkeyHost`, `MouseTracker` — together with a feature-gated `mock`
module so that `linerule-core` could be tested without touching any OS
API. After the production Windows implementation landed (commits
`3555c67`, `e1b7ea2`, `0277504`), the trait surface had **zero
production consumers**:

- The production `windows::run` event loop calls into winit + Win32 +
  `global-hotkey` directly. winit's `Window` is `!Send` and the event
  loop owns it concretely; there is no callsite that takes a `dyn
  OverlaySurface`.
- `HotkeyHost` was never instantiated outside the mock. Hotkeys are
  registered against `GlobalHotKeyManager` directly inside `run`, and
  events are forwarded through a `winit::EventLoopProxy<UserMessage>`
  rather than a `HotkeySink`.
- `MouseTracker` was never instantiated outside the mock either.
  `windows::poll_cursor_logical` calls `GetCursorPos` directly inside
  the `about_to_wait` hook.
- `linerule-core` tests are pure-logic unit tests; none of them
  consumed the trait surface (they exercise `render` / `reduce`
  directly).

The traits and their mock impls thus formed a parallel API surface
that was carried forward in a hope-to-need way that did not pay
dividends — and *would not* pay dividends in v0.2 either, because the
upcoming macOS / Linux event loops will face the same `!Send`
constraint and own their surface concretely too.

## Decision

Remove the trait surface entirely. The production crate exposes a
single verb:

```rust
pub fn run(initial_state: State, hotkeys: &[(String, HotkeyEffect)])
    -> Result<(), RunError>;
```

Behind the scenes, `cfg(target_os = ...)` dispatches to the per-OS
event-loop module (today: `mod windows`; v0.2: parallel `mod macos` /
`mod linux_wayland` / `mod linux_x11` modules). Each module owns its
event loop, its window, and its hotkey manager directly — no shared
trait, no mock layer.

`HotkeyHost` / `OverlaySurface` / `MouseTracker` traits, the
`HotkeySink` / `HotkeyToken` / `HotkeyRelease` capability types, and
the `SurfaceError` / `HotkeyError` / `MouseError` enums are all
deleted.

## Consequences

**Becomes easier**
- `linerule-platform/src/lib.rs` shrinks from ~250 LOC of trait /
  mock plumbing to ~95 LOC of `run` + `RunError`.
- `cargo-shear` no longer needs an exception for the dev-only
  `crossbeam-channel` (it's used directly by `windows.rs` for
  forwarding `global_hotkey` events).
- v0.2 OS impls land as **additive** parallel modules behind
  `cfg(target_os)` — exactly as intended for ADR-0003 — without the
  shared-trait crutch that did not actually help.
- Coverage gate is no longer dragged down by mock impls that exist
  only to satisfy the trait surface.

**Becomes harder**
- `linerule-core` cannot be tested *through* the platform traits.
  This is fine: the platform layer was always thin enough that
  property tests over `render` + `reduce` cover the algebraic surface
  completely; integration with the OS is intrinsically untestable
  outside Windows itself, so the windows-2022 CI matrix entry
  (`.github/workflows/ci.yml::windows-smoke`) remains the canonical
  guard.
- A `Cargo.toml` `mock` feature gate is no longer offered. If a
  future test consumer needs an in-process simulator, it will have
  to be a per-test inline harness, not a workspace-wide trait.

## Alternatives considered

- **Keep the traits, document them as v0.2-only forward-declarations.**
  Rejected: dead code rots, and the constraint they encode (a
  `dyn OverlaySurface` indirection over a `Send + 'static` Window)
  is exactly what winit forbids — the trait would need a non-trivial
  rewrite to make it work even once.

- **Keep the traits, instantiate them inside `windows::run` for
  uniformity.** Rejected: the trait surface offers no extension
  point — the binary has exactly one platform per build target — and
  carrying the indirection just to "match the design doc" is
  ceremony.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- ADR-0003 (superseded)
- `crates/linerule-platform/src/lib.rs`
- `crates/linerule-platform/src/windows.rs`
