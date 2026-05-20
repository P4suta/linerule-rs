# 0006 — DWrite FFI を `win32_ffi/dwrite.rs` に集約

**Status:** Accepted (Phase G HUD telemetry 描画の前段, 2026-05-20).

**See also:** [[0003-unsafe-isolation]] (`unsafe` を `win32_ffi/` に集約), [[0002-architecture-principles]] §7 (unsafe 局所化), Phase G groundwork PR #41.

## 文脈

Phase G の HUD telemetry 表示には DirectWrite (DWrite) によるテキスト描画が必須 (`IDWriteFactory` / `IDWriteTextFormat` / `IDWriteTextLayout` + `ID2D1DeviceContext::DrawText`)。これらは windows crate でも `unsafe fn` で覆われている COM 接面で、Phase D で確立した D3D11 + DXGI + D2D + DComp 統合パイプライン (`win32_ffi/graphics.rs`) と並列の独立した FFI 接面を構成する。

ADR-0003 で「`#![allow(unsafe_code)]` のファイル数を増やすときは ADR を要する」と定めたため、`win32_ffi/dwrite.rs` の新規追加には本 ADR を要する。

## 判断

DWrite の薄い safe wrapper を **`win32_ffi/dwrite.rs` 1 ファイルに集約**する。`graphics.rs` に同居させない。

## 根拠

1. **ファイル単位の責務分離**: `graphics.rs` は既に D3D11 + DXGI + D2D + DComp を扱って 359 LOC ある。DWrite を追加すると複合度が増し、レビュー時の境界が曖昧になる。
2. **DWrite だけ「テキストレイアウト + フォント family 解決」というレイヤを 1 段上に持つ**ため、独立した API 設計が自然 (`IDWriteFactory` の create_text_format / `IDWriteTextLayout::HitTestMetrics` 等は他の COM 階層と直交)。
3. **ADR-0003 が想定する拡張パターンに合致**: 「`win32_ffi/graphics.rs` の続編で DWrite 専用」というドキュメント上の例示と同じ位置付け。
4. **`unsafe` 監査の局所性**: `win32_ffi/dwrite.rs` 1 ファイルに `#![allow(unsafe_code, reason = "FFI 境界...")]` を貼ることで、レビュアーは「このファイル内の `unsafe` ブロックを精査すれば DWrite 接面の安全性が確認できる」と判断できる。`graphics.rs` に混入させると 2 つの subsystem を 1 ファイルで監査することになる。

## 結果

- 新規 `crates/linerule-platform-windows/src/win32_ffi/dwrite.rs` (~180 LOC) を追加し、`win32_ffi.rs` 親モジュールから `pub mod dwrite;` 宣言する。
- `IDWriteFactory` を持つ `DcompPipeline` ではなく、`DwritePipeline { factory, formats: HashMap<HudFontKey, IDWriteTextFormat> }` のような薄いコンテナを `hud_renderer.rs` 側で所有する。HUD 描画は `D2D1DeviceContext::DrawText` 経由で `D2D1Bitmap1` (`IDCompositionSurface` 経由) に書く。
- `composition_renderer.rs` の `DcompPipeline` を `pub(crate)` accessor 経由で `hud_renderer.rs` に共有し、同じ `ID2D1DeviceContext` で text rendering / fill rendering を行う。

## 検討した代替案

### A. `graphics.rs` に DWrite を生やす

却下: ファイルが大きくなりすぎ、`unsafe` 監査の境界が曖昧になる。

### B. `win32_ffi/text.rs` という汎用名

却下: Win32 のテキスト描画手段は `DrawTextW` (GDI) / `TextOutW` (GDI) / DWrite と複数あり、`text` だと曖昧。`dwrite` で具体 API を名乗る方が誠実。

### C. `hud_renderer.rs` 内に `unsafe` を散らす

却下: ADR-0003 違反 (`unsafe` は `win32_ffi/` 集約)。HUD 描画ロジックと FFI が混ざると以後の HUD 機能追加で `unsafe` が膨らむ温床。

## チェックリスト

- [x] `win32_ffi/dwrite.rs` には `#![allow(unsafe_code, reason = "FFI 境界...")]` を頭に置く
- [x] 各 `unsafe { ... }` ブロックの直前に `// SAFETY: ...` コメントを書く
- [x] `hud_renderer.rs` は `#![forbid(unsafe_code)]` を維持
- [x] 公開する safe 関数: `create_dwrite_factory()` / `create_text_format(...)` / `draw_text(...)`
- [x] `IDWriteFactory` / `IDWriteTextFormat` の Drop chain は windows crate の COM type が自動で Release する
