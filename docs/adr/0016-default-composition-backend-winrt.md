# 0016 — Remove DComp, make WinRT the single composition backend

**Status:** Accepted. Partially supersedes the dual-backend setup of [[0015-winrt-composition-backend]].

**See also:** [[0015-winrt-composition-backend]], [[0003-unsafe-isolation]].

## Context

With dual backends (DComp default + `LINERULE_COMPOSITOR=winrt` toggle), `Blur` only produced real blur under WinRT; the default DComp path degraded to a flat tint fill indistinguishable from black dimming. If WinRT works, maintaining the DComp pipeline is not worth it.

## Decision

Remove the DComp backend and the backend-switching abstraction; use **WinRT `Windows.UI.Composition` alone**. Drop the `LINERULE_COMPOSITOR` env var.

- Delete: `composition_renderer.rs`, `hud_renderer.rs`, the DComp pipeline in `graphics.rs`, `dwrite.rs::draw_hud_to_surface`, the `Win32_Graphics_DirectComposition` feature of the `windows` crate, and the DComp-related disallowed-methods in `clippy.toml`.
- Keep only the shared `D2dStack` / `create_d2d_stack` in `graphics.rs`.
- Shrink `renderer_backend.rs` from enum dispatch to a thin `build_backends(hwnd, hud_config)`. `OverlayWndState` holds the WinRT renderer directly, and device-lost rebuild recreates WinRT directly.
- With no fallback, WinRT init failure (`attach_compositor`) is a fatal error.
- Setting `Blur`'s tint to the same opacity as `DimBlack` hides the blur behind the tint and looks black, so `surround_brush` scales the Blur tint alpha to about 1/3 of the perceptual byte (`render.rs::blur_tint_alpha`).
- Add `--initial-effect {dim|white|blur}` to the CLI and launch the CI GUI smoke with `--initial-effect blur` to exercise the WinRT backdrop-blur COM path headless.

## Consequences

- Single backend; enum dispatch and the DComp pipeline are gone. `Blur` renders as WinRT backdrop blur by default.
- Risk: no fallback where WinRT init fails, so it is fatal. If the runner cannot create WinRT composition, the headless GUI smoke turns CI red (intentional verification signal).
- On Linux only cross-compile (`cargo xwin check`) is possible; whether blur actually samples the background on real hardware is unverified. If it is flat, toggle `LINERULE_BLUR_HOST=1` to switch `CreateBackdropBrush` ↔ `CreateHostBackdropBrush` and isolate the cause.
