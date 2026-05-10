# 0001. Tech stack: winit + vello + wgpu + peniko + windows crate + global-hotkey

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: architecture, gui, rendering

## Context

linerule needs a frameless, transparent, always-on-top, click-through
overlay window that follows the cursor on Windows (v0.1; macOS / Linux
in v0.2+). Two realities shape the choice:

1. The runtime job, stripped down, is a pure function:
   `(Mode, ScreenPoint, Config, MonitorGeometry) -> Vec<TranslucentRect>`
   followed by a present.
2. Every cross-platform GUI framework still requires native bypass for
   click-through (Tauri 2 issue [#13070][tauri-13070] open;
   `WS_EX_LAYERED | WS_EX_TRANSPARENT` not exposed; macOS needs
   `tauri-nspanel`; Wayland needs `wlr-layer-shell`).

[tauri-13070]: https://github.com/tauri-apps/tauri/issues/13070

## Decision

Adopt the following stack:

| Concern | Choice |
|---|---|
| Window / event loop | [`winit`](https://crates.io/crates/winit) `0.30` |
| GPU 2D renderer | [`vello`](https://crates.io/crates/vello) `0.8` + [`wgpu`](https://crates.io/crates/wgpu) `29` + [`peniko`](https://crates.io/crates/peniko) `0.6` (Linebender stack) |
| Win32 click-through + layered window | [`windows`](https://crates.io/crates/windows) `0.62` |
| System-wide hotkeys | [`global-hotkey`](https://crates.io/crates/global-hotkey) `0.8` |
| Buffer transfer | [`bytemuck`](https://crates.io/crates/bytemuck) `1.21` |
| IPC channels | [`crossbeam-channel`](https://crates.io/crates/crossbeam-channel) `0.5` |

Settings UI (when added in v0.2) goes into a separate egui-on-demand
window, *not* into the overlay.

## Consequences

**Becomes easier**
- Single statically-linked binary in the 2–6 MB range; cold start <100 ms.
- The render path is a pure function with `OverlayFrame` as the IO
  boundary — fully testable without any OS API.
- vello + peniko encode color, brushes, paths in types — `peniko::Brush::Solid` etc. — matching the type-theoretic style we adopt elsewhere.
- v0.2 animation work (bar fade, mask easing) lands on a GPU stack that
  can already do compositing, no rewrite.

**Becomes harder**
- We hand-write the optional settings window (mitigated by spawning
  egui-on-demand for `linerule settings` — deferred to v0.2).
- No browser devtools for the overlay (covered by `tracing` spans).
- Slightly more wiring around HiDPI text (egui handles this in the
  settings window; the overlay itself draws no text).

## Alternatives considered

- **Tauri 2** — drags ~80 MB of webview to draw two rectangles; click-through
  unfixed (issue #13070); we'd still have to native-bypass for every OS.
  Negative net.
- **egui+eframe as primary** — pure Rust, works, but pays a per-frame
  immediate-mode redraw budget the overlay never uses. Right tool for
  the *settings* window, wrong for the always-on overlay. Adopted
  there in v0.2 only.
- **iced** — comparable to egui but transparent-overlay support was
  uneven in 2026. Mainstream-risk.
- **slint** — declarative model is ill-matched to "draw two rects per
  frame driven by a 60 Hz mouse stream"; transparent overlay support
  uneven. Niche risk.
- **Bevy** — game engine; ECS overkill for two rectangles.
- **xilem** — exciting but pre-1.0 (Linebender). Adopt-when-stable.
- **softbuffer (CPU blit)** — minimal but: (a) we'd hand-write blend /
  antialias on HiDPI, (b) v0.2 animation requires GPU re-write, (c) no
  type-theoretic vocabulary on top of raw pixel buffers. vello wins on
  all three.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- [Tauri Window Customization](https://v2.tauri.app/learn/window-customization/)
- [Tauri Issue #13070 — click-through transparency](https://github.com/tauri-apps/tauri/issues/13070)
- [Linebender — vello](https://github.com/linebender/vello)
- [Linebender — peniko](https://github.com/linebender/peniko)
- [Microsoft Learn — SetLayeredWindowAttributes](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes)
