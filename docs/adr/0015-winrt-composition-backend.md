# 0015 — WinRT Composition backend and backdrop blur

**Status:** Accepted. The dual-backend setup was removed in [[0016-default-composition-backend-winrt]], moving to WinRT only.

**See also:** [[0016-default-composition-backend-winrt]], [[0003-unsafe-isolation]], [[0006-dwrite-ffi-isolation]].

## Context

Backdrop blur that blurs only the surroundings is a requirement. Win32 DirectComposition alone cannot do it (`IDCompositionVisual::SetEffect` covers only the visual's own subtree). The only path is WinRT `Windows.UI.Composition`'s `CreateDesktopWindowTarget` + `CreateBackdropBrush` + a Gaussian blur `CompositionEffectBrush`. `CreateBackdropBrush` captures what is behind a `WS_EX_NOREDIRECTIONBITMAP` window and excludes our own drawing (no feedback). `CreateDesktopWindowTarget` and Win32 `CreateTargetForHwnd` cannot coexist on the same HWND, so the whole composition host moves to WinRT.

## Decision

Add a WinRT Composition backend and switch between it and Win32 DComp via `LINERULE_COMPOSITOR` (default `dcomp`). Keep DComp as a fallback until WinRT is stable on real hardware.

- `unsafe` for WinRT interop / DispatcherQueue / the hand-written COM effect lives in `win32_ffi/composition.rs` and `win32_ffi/blur_effect.rs` (ADR-0003). Per-frame safe visual operations live in `winrt_composition_renderer.rs` / `winrt_hud_renderer.rs` (`#![forbid(unsafe_code)]`).
- Since Win2D is UWP-only, Gaussian blur implements `IGraphicsEffectD2D1Interop` by hand (`CLSID_D2D1GaussianBlur` + StandardDeviation, `blur_effect.rs`).
- DispatcherQueue is stood up on the UI thread with `DQTYPE_THREAD_CURRENT` + `DQTAT_COM_STA` and drained by the existing GetMessage loop. WinRT auto-commits per tick, so no explicit commit is needed.
- Backends are bound by the enums in `renderer_backend.rs` (`OverlayBackend` / `HudBackend`); device-lost rebuild reconstructs the chosen backend.

## Consequences

- New: `win32_ffi/composition.rs`, `win32_ffi/blur_effect.rs`, `winrt_composition_renderer.rs`, `winrt_hud_renderer.rs`, `renderer_backend.rs`.
- Add `Blur` to `SurroundEffect` and `Blur { tint }` to `Brush`. The DComp backend degrades `Brush::Blur` to a solid tint fill.
- On Linux, verification is cross-compile only (`cargo xwin check`). Appearance, click-through, DPI, multi-monitor, device-lost, and DispatcherQueue operation are confirmed on real Windows hardware.
