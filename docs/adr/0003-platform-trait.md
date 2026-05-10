# 0003. Platform abstraction: OverlaySurface + HotkeyHost + MouseTracker

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: architecture, platform

## Context

The MVP targets Windows (ADR-0004), but macOS and Linux are deferred
rather than refused. The crate split must structurally guarantee that
adding a new OS impl in v0.2 does not retro-break `linerule-core` or
`linerule-config`.

## Decision

Three traits in `linerule-platform` that the binary wires together:

```rust
pub trait OverlaySurface: Send + 'static {
    fn show(&mut self) -> Result<(), SurfaceError>;
    fn hide(&mut self) -> Result<(), SurfaceError>;
    fn present(&mut self, frame: &OverlayFrame) -> Result<(), SurfaceError>;
    fn monitor(&self) -> ScreenRect<Logical>;
    fn dpi_scale(&self) -> f32;
}

pub trait HotkeyHost: Send + 'static {
    fn register(&mut self, chord: &str, action: Action, sink: HotkeySink)
        -> Result<HotkeyToken, HotkeyError>;
}

pub trait MouseTracker: Send + 'static {
    fn position(&self) -> Result<Point<Logical>, MouseError>;
}
```

Per-OS impls live behind `#[cfg(target_os = "windows")] mod windows;`
(and future `mod macos;` / `mod linux_x11;` / `mod linux_wayland;`).
A `mock` module is feature-gated for tests and non-Windows hosts.

Hotkey callbacks fire on OS threads and are bridged to the main loop
via a bounded `crossbeam_channel::Sender<Action>` (`HotkeySink`) — on
overflow the impl emits `tracing::warn!` and drops.

## Consequences

**Becomes easier**
- `linerule-core` is fully testable without any OS API ever being
  called: the mock impls live in `crates/linerule-platform/src/mock.rs`
  and satisfy the same trait surface.
- v0.2 OS additions land as new `#[cfg(target_os = ...)] mod xyz;`
  files; no API change to `linerule-core`.
- Synchronous trait surface — no `async`. Matches winit's event loop.

**Becomes harder**
- Per-OS code uses `unsafe` to call into Win32 / Cocoa / X11 / Wayland.
  Each `unsafe` block is gated by `// SAFETY:` comments enforced by
  `scripts/strict-code.sh`.
- The `Box<dyn OverlaySurface>` indirection costs a vtable jump per
  present. Negligible at 60 fps and well within the cost budget.

## Alternatives considered

- **Concrete `WindowsOverlay` struct with no trait** — fastest, but
  forces every v0.2 OS addition to also touch `linerule-core` /
  `linerule` callsites. Rejected.
- **Single trait `Platform` with all three concerns** — easier wiring,
  but couples concerns that have wildly different lifetimes (the
  surface lives forever, the hotkey host can be torn down on
  rebinding, the mouse tracker is a degenerate getter). Rejected.
- **Async trait surface** — would force an async runtime where there
  is no asynchronous IO. winit's event loop is sync; introducing an
  async layer would just be ceremony. Rejected.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- `crates/linerule-platform/src/lib.rs`
