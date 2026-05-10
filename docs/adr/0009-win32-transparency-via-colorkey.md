# 0009. v0.1 Win32 transparency via `LWA_COLORKEY`, with emergency-exit hotkey

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: platform, ux, ffi

## Context

The overlay covers the entire monitor, click-through, always-on-top.
Two intertwined problems surfaced during first runtime verification:

1. **Visual transparency.** The naive
   `WS_EX_LAYERED + SetLayeredWindowAttributes(LWA_ALPHA, 255)` makes
   the overlay window per-pixel-opaque — every cleared pixel from
   vello renders as opaque black, hiding the entire desktop. The
   "obvious" fix — set the wgpu `CompositeAlphaMode` to
   `PreMultiplied` and let DWM composite per-pixel alpha — does not
   take effect under `WS_EX_LAYERED`, because wgpu's
   `CreateSwapChainForHwnd` path bypasses Direct Composition. Without
   `CreateSwapChainForComposition` (which wgpu does not yet expose),
   layered-window per-pixel alpha is unavailable.

2. **Emergency exit.** Because the overlay is `WS_EX_TRANSPARENT`
   (mouse pass-through) AND covers the whole screen AND has no UI
   element to focus, the only way to recover from a wedged state
   (renderer panic, lost cursor poll, etc.) is via a system-wide
   hotkey that bypasses the state machine. Without one, the user has
   to Task-Manager-kill the binary.

## Decision

### Transparency

Use `WS_EX_LAYERED + WS_EX_TRANSPARENT + SetLayeredWindowAttributes(
    hwnd, COLORREF(0x00_00_00), 255, LWA_COLORKEY)`.

The colour key `(0, 0, 0)` (pure black) becomes the transparency
sentinel: every pixel exactly that RGB triple drops out, every other
pixel renders fully opaque. Vello's `Color::TRANSPARENT` clears
unrendered regions to `(0, 0, 0, 0)`, which the
`vello::util::TextureBlitter` writes into the swapchain as `(0, 0, 0)`
after alpha is dropped — automatic background hole-out.

Consequences for our colour palette:

- `Rgba::DEFAULT_BAR` is opaque yellow `(255, 235, 59)` — visible.
- `Rgba::DEFAULT_MASK` is *near*-black `(8, 8, 8)`, NOT pure
  `(0, 0, 0)`, because pure black would be silently keyed to
  transparent and the typoscope dim regions would vanish. `(8, 8, 8)`
  is visually indistinguishable from pure black to a human reader,
  but defeats the colour-key match. Pinned by a unit test
  (`linerule-core/tests/unit_newtypes.rs::rgba_default_mask_is_near_black_not_pure_black`).

### Emergency exit

Add `Action::Quit` to `linerule_core::Action`. The state machine
treats it as a no-op (default `StateDelta`). The Windows event-loop
handler intercepts it specially via `event_loop.exit()`. The default
chord is `Ctrl+Alt+Q`, registered through the same `global-hotkey`
host as every other binding, so OS-wide it always reaches us
regardless of which app holds focus.

`HotkeyMap::default()` ships with `quit = "Ctrl+Alt+Q"` so a user who
installs and never edits the config still has the escape hatch.

## Consequences

**Becomes easier**
- Transparency works under wgpu's stable swapchain path; no Direct
  Composition glue required for v0.1.
- Recovery from any runtime wedge is one hotkey away.

**Becomes harder**
- True per-pixel translucency is not available in v0.1 — the bar is
  fully opaque, the mask dim is opaque near-black. Acceptable for a
  reading aid (the bar is the focal element), but a v0.2 enhancement.
- Anyone writing a new mask / bar colour MUST avoid pure `(0, 0, 0)`,
  or its rendering becomes a transparent slit. Pinned by the
  newtype-tests assertion above.

## Alternatives considered

- **`SetLayeredWindowAttributes(LWA_ALPHA, 255)`** — produced opaque
  black overlay (verified empirically). Rejected.
- **`CompositeAlphaMode::PreMultiplied` on the swapchain alone** — no
  effect under `WS_EX_LAYERED` with `CreateSwapChainForHwnd` (verified
  empirically). Rejected.
- **`CreateSwapChainForComposition` + Direct Composition** — would
  give true per-pixel alpha, but wgpu does not expose this. Would
  require a custom wgpu-hal patch or a parallel raw-DXGI surface.
  Deferred to v0.2 / future ADR.
- **`UpdateLayeredWindow` with a GDI bitmap** — would give per-pixel
  alpha but conflicts with DXGI swapchain. Would require a CPU
  read-back path. Rejected on perf grounds.

## References

- Plan file: `~/.claude/plans/velvet-finding-hennessy.md`
- ADR-0001 (tech stack), ADR-0003 (platform trait surface)
- `crates/linerule-platform/src/windows.rs::apply_click_through`
- `crates/linerule-platform/src/windows.rs::COLORKEY_TRANSPARENT`
- `crates/linerule-core/src/lib.rs::Rgba::DEFAULT_MASK`
- [`SetLayeredWindowAttributes`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes)
- [`Window features (LWA flags)`](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features)
- [wgpu `CompositeAlphaMode`](https://docs.rs/wgpu/28.0.0/wgpu/enum.CompositeAlphaMode.html)
