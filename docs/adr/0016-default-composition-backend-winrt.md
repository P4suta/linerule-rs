# 0016 — DComp 撤去、WinRT を単一 composition backend に

**Status:** Accepted。[[0015-winrt-composition-backend]] の二重 backend 構成を一部 supersede する。

**See also:** [[0015-winrt-composition-backend]] (WinRT backend 導入), [[0003-unsafe-isolation]] (`unsafe` を `win32_ffi/` に集約).

## 文脈

[[0015-winrt-composition-backend]] で WinRT Composition backend を追加したとき、安定性が
未実証だったため Win32 DirectComposition (DComp) を既定に残し、`LINERULE_COMPOSITOR=winrt`
で切り替える二重 backend 構成にした。その結果:

- `Blur` は WinRT backend でしか本物のぼかしにならず、既定 (DComp) では `Brush::Blur` が
  tint (黒) の単色塗りに degrade する。普通に起動したユーザーには `Blur` が黒協調 (`DimBlack`)
  と区別できず「効いていない」ように見えた。
- 二重 backend (`OverlayBackend` / `HudBackend` enum 分岐、DComp 用 `composition_renderer.rs`
  / `hud_renderer.rs` / `graphics.rs` の DComp パイプライン、env 切替) の保守コストが、
  WinRT が動くなら割に合わない。

## 決定

DComp backend と backend 切替の抽象化を撤去し、**WinRT `Windows.UI.Composition` 単一**にする。
`LINERULE_COMPOSITOR` 環境変数は廃止。

- 削除: `composition_renderer.rs`、`hud_renderer.rs`、`graphics.rs` の DComp パイプライン
  (`DcompPipeline` / `create_dcomp_pipeline` / 各 `IDComposition*` ヘルパ / `fill_surface` /
  `commit`)、`dwrite.rs` の `draw_hud_to_surface`、`windows` crate の
  `Win32_Graphics_DirectComposition` feature、`clippy.toml` の DComp 系 disallowed-methods。
- `graphics.rs` は共有の `D2dStack` / `create_d2d_stack` (WinRT pipeline が使用) だけ残す。
- `renderer_backend.rs` は enum 分岐を捨て、薄い `build_backends(hwnd, hud_config)` 構築ヘルパに
  縮約。`OverlayWndState` は `WinrtCompositionRenderer` / `WinrtHudRenderer` を直接保持し、
  `compositor_kind` の保持をやめる。device-lost rebuild も WinRT を直接作り直す。
- フォールバックは無くなったので、WinRT 初期化失敗 (`attach_compositor`) は致命エラーになる。
- `Blur` の tint は `DimBlack` と同じ不透明度 (既定 perceptual ≈ 85%) だと、ぼかしが濃い tint に
  隠れてやはり黒に見える。`surround_brush` で Blur の tint アルファを perceptual byte の
  約 1/3 にスケールし (`render.rs::blur_tint_alpha`、opacity ホットキー連動は維持)、ぼけが
  透けるようにした。
- CLI に `--initial-effect {dim|white|blur}` を追加し、CI GUI smoke を `--initial-effect blur`
  で起動して WinRT backdrop-blur の COM 経路 (effect factory / 自作 interop / backdrop brush /
  tint sprite) を headless でも exercise する。

## 結果

- 単一 backend になり enum dispatch と DComp パイプラインが消えた。`Blur` は既定起動で
  WinRT backdrop blur としてレンダリングされる。
- リスク: WinRT 初期化に失敗する環境では fallback が無く致命となる。GitHub-hosted Windows
  runner の headless GUI smoke も WinRT を走らせるため、runner が WinRT composition を
  作れなければ CI が赤になる (= 検証信号。意図的にスモークを WinRT のまま走らせる)。
- 検証は Linux では cross-compile (`cargo xwin check`) のみ。実際にぼけが背後をサンプルするか
  (frosted vs flat tint) は Windows 実機で確認する。単色に見える場合は `LINERULE_BLUR_HOST=1`
  で `CreateBackdropBrush` ↔ `CreateHostBackdropBrush` を切り替えて切り分ける。
