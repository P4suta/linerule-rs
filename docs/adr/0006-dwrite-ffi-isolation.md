# 0006 — Consolidate DWrite FFI into `win32_ffi/dwrite.rs`

**Status:** Accepted (2026-05-20).

**See also:** [[0003-unsafe-isolation]], [[0002-architecture-principles]] §7.

## Context

HUD telemetry display requires DirectWrite (`IDWriteFactory` / `IDWriteTextFormat` / `IDWriteTextLayout` + `ID2D1DeviceContext::DrawText`). The COM surface needs `unsafe` and is independent of the D3D11/DXGI/D2D/DComp in `graphics.rs`. Per ADR-0003, adding a new `#![allow(unsafe_code)]` file requires an ADR.

## Decision

Consolidate a thin safe DWrite wrapper into a single file `win32_ffi/dwrite.rs`; do not colocate it in `graphics.rs`.

## Rationale

- `graphics.rs` is already 359 LOC. Adding DWrite blurs the review boundary.
- DWrite is an independent layer (text layout + font family resolution), orthogonal to the other COM hierarchies.
- `unsafe` audit locality: one file carries `#![allow(unsafe_code)]` and is audited as a single subsystem.

## Consequences

- Add a new `crates/linerule-platform-windows/src/win32_ffi/dwrite.rs`, declared from the parent module via `pub mod dwrite;`.
- `hud_renderer.rs` owns `DwritePipeline { factory, formats: HashMap<HudFontKey, IDWriteTextFormat> }`. Drawing goes through `D2D1DeviceContext::DrawText` into a `D2D1Bitmap1` (`IDCompositionSurface`).
- Share the `DcompPipeline` from `composition_renderer.rs` via a `pub(crate)` accessor, doing text/fill rendering on the same `ID2D1DeviceContext`.

## Alternatives Considered

- **A. Grow DWrite inside `graphics.rs`** — rejected: file bloat, ambiguous `unsafe` audit boundary.
- **B. Generic name `win32_ffi/text.rs`** — rejected: confusable with GDI (`DrawTextW`/`TextOutW`); `dwrite` is more specific.
- **C. Scatter `unsafe` inside `hud_renderer.rs`** — rejected: violates ADR-0003 (`unsafe` consolidated under `win32_ffi/`).

## Checklist

- [x] `#![allow(unsafe_code, reason = "FFI boundary...")]` at the top of `win32_ffi/dwrite.rs`
- [x] `// SAFETY: ...` immediately before each `unsafe { ... }`
- [x] `hud_renderer.rs` keeps `#![forbid(unsafe_code)]`
- [x] Public safe functions: `create_dwrite_factory()` / `create_text_format(...)` / `draw_text(...)`
- [x] `IDWriteFactory` / `IDWriteTextFormat` Drop auto-releases via the windows crate
