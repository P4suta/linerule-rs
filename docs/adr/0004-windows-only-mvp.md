# 0004. v0.1 Windows-only scope

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: scope, infra

## Context

Cross-platform support was the original aim. Investigating the 2026
desktop overlay landscape surfaced significant per-OS complexity:

- **Windows** — straightforward; `WS_EX_LAYERED | WS_EX_TRANSPARENT` is
  one syscall.
- **macOS** — needs `NSPanel` (not `NSWindow`) for non-activating
  overlays. Mature via `tauri-nspanel` / `objc2-app-kit`.
- **Linux X11** — XShape + empty input region. Standard.
- **Linux Wayland (wlroots)** — `wlr-layer-shell` works on Sway /
  Hyprland / Wayfire.
- **Linux Wayland (GNOME)** — Mutter refuses `wlr-layer-shell` and
  exposes no equivalent. Click-through always-on-top overlay is
  *impossible* without XWayland fallback.

The user's actual reading environment is WSL2 ↔ Windows: e-books are
read on the Windows side. Shipping cross-platform would either cut
corners or balloon scope.

## Decision

v0.1 ships **Windows 10 / 11 only**. The platform trait surface
(ADR-0003) is fully landed so adding macOS / Linux in v0.2+ requires
no changes to `linerule-core` / `linerule-config`.

GNOME-Wayland — when Linux is added — will be handled with a clear
startup-time error message + `GDK_BACKEND=x11` workaround instructions.

## Consequences

**Becomes easier**
- Single `windows` crate FFI to handle.
- Single CI matrix entry (`windows-2022`) plus a Linux cross-build
  smoke test via `cargo-xwin`.
- No need to reason about wlr-layer-shell, `NSPanel.collectionBehavior`,
  or Spaces / multi-Space behaviour in the MVP.

**Becomes harder**
- macOS / Linux users cannot use linerule until v0.2.
- Any v0.2 cross-platform work must land an additional ADR
  describing the per-OS surface implementation.

## Alternatives considered

- **Cross-platform from v0.1** — multiplies surface area; concretely
  doubles or triples the verification matrix. Rejected for MVP.
- **Linux X11 only** — possible but doesn't match the user's actual
  environment.
- **Tauri-driven cross-platform** — explored in ADR-0001 alternatives;
  rejected on click-through grounds.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- ADR-0003 (platform trait surface)
