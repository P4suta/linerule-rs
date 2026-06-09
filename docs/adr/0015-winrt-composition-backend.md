# 0015 — WinRT Composition backend と backdrop blur

**Status:** Accepted。二重 backend 構成 (DComp fallback + `LINERULE_COMPOSITOR` 切替) は
[[0016-default-composition-backend-winrt]] で撤去され、WinRT 単一に移行した。

**See also:** [[0016-default-composition-backend-winrt]] (DComp 撤去・WinRT 一本化),
[[0003-unsafe-isolation]] (`unsafe` を `win32_ffi/` に集約), [[0006-dwrite-ffi-isolation]] (HUD 描画).

## 文脈

周囲効果に「ボカシ」(スリットはくっきり・周囲だけ背後をぼかす) を追加したい。Win32
DirectComposition 単体では backdrop blur ができない (`IDCompositionVisual::SetEffect` は
visual 自身のサブツリーをぼかすだけで、topmost click-through 窓の背後はぼかせない)。

唯一の正攻法は WinRT `Windows.UI.Composition`: `Compositor` +
`ICompositorDesktopInterop::CreateDesktopWindowTarget` + `CreateBackdropBrush` +
Gaussian blur の `CompositionEffectBrush`。`CreateBackdropBrush` は `WS_EX_NOREDIRECTIONBITMAP`
窓 (本 overlay は既にそう) で背後内容を捕捉し、自分の描画は含めない (feedback 無し)。
WinRT の `CreateDesktopWindowTarget` と Win32 の `CreateTargetForHwnd` は同一 HWND に
共存できないため、overlay の composition host ごと WinRT へ移す必要がある。

## 決定

WinRT Composition backend を追加し、`LINERULE_COMPOSITOR` 環境変数で Win32 DComp と切り替える
(既定 `dcomp`、`winrt` で WinRT)。WinRT 経路が実機で安定するまで DComp を fallback として残す。

- WinRT interop / DispatcherQueue / 自作 COM effect の `unsafe` は `win32_ffi/composition.rs` と
  `win32_ffi/blur_effect.rs` に集約 (ADR-0003)。毎フレームの visual 操作 (windows crate では
  safe) は `winrt_composition_renderer.rs` / `winrt_hud_renderer.rs` (`#![forbid(unsafe_code)]`)。
- Gaussian blur は Win2D が UWP 専用なので、`IGraphicsEffectD2D1Interop` を自前実装し
  `CLSID_D2D1GaussianBlur` + StandardDeviation を記述する (`blur_effect.rs`)。
- DispatcherQueue は `DQTYPE_THREAD_CURRENT` + `DQTAT_COM_STA` で UI thread に立て、既存の
  GetMessage ループで drain する。WinRT は per-tick で自動 commit するので明示 commit は不要。
- backend は `renderer_backend.rs` の enum (`OverlayBackend` / `HudBackend`) で束ね、
  `OverlayWndState` がどちらかを保持する。device-lost rebuild は採用 backend を再構築する。

## 結果

- 新規: `win32_ffi/composition.rs` (WinRT pipeline)、`win32_ffi/blur_effect.rs` (自作 effect)、
  `winrt_composition_renderer.rs`、`winrt_hud_renderer.rs`、`renderer_backend.rs`。
- `OverlayConfig` の `SurroundEffect` に `Blur` を追加、`Brush` に `Blur { tint }` を追加。
  DComp backend は `Brush::Blur` を tint の単色塗りに degrade する。
- 検証は Linux では cross-compile (`cargo xwin check`) のみ。見た目・click-through・DPI・
  multi-monitor・device-lost・DispatcherQueue 駆動は Windows 実機で確認する。
