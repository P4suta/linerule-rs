# Phase η — Roadmap (Draft)

**Status:** Draft (post Phase ζ + cs-port residual cleanup, 2026-05-24).

**Predecessors:**
- Phase ζ (`4ea9e11`): layout-independent hotkeys + HUD help + opaque background + repeat
- cs-port residual cleanup PR series (#68..#73): indicator bar shape, base_opacity pin, ForegroundHook, HUD telemetry pipeline, Phase H PR-E (`AppError::class()` → HUD notification), SBOM in release assets

## Context

Phase A〜ζ で「読書補助オーバーレイ」として動作する MVP が完成し、cs-port の漏れと rs 自身の `dead_code allow` をすべて解消した (2026-05-24)。残る積み残しは外部リソース (証明書 / packaging) と user feedback 駆動の改善が中心で、Phase ζ までのように内部品質ゲートだけで完結する話ではなくなった。次フェーズは **「外部接点を増やす」+「user feedback を待つ」** モードに切り替える。

このドキュメントはコミット権限のある決定ではなく、**雑感の集合体 (Draft)** として記録する。次フェーズの起点は user feedback で決まる前提。

## 候補テーマ

### A. Code signing + Winget 統合 (外部依存あり、blocker: cert 取得)

- **動機**: SmartScreen 警告の解消、Microsoft Store / Winget 経由配布。
- **必要なもの**:
  - Code signing 証明書 (Sectigo / DigiCert / SSL.com 等、年 ~$200-500)。OV (Organization Validation) で十分か、EV (Extended Validation) が必要かは SmartScreen 即時通過の要件次第 (EV は ~$400-800/year で Smart Screen reputation を瞬時に獲得できる)
  - GH Secrets に証明書 + パスワード設定
  - `signtool sign /sha1 ... /tr <timestamp-server> /td sha256 /fd sha256 linerule.exe` を `release-assets.yml` に挿入
- **Winget manifest**: `microsoft/winget-pkgs` への PR 自動生成 (`winget-create` CLI を CI に組み込み)。pkg id は `P4suta.linerule` あたり。
- **Open question**: cert 取得は個人 vs 組織? 個人での EV 取得は CA によって弾かれるため、OV から始めて SmartScreen reputation を蓄積するか、組織化してから EV に移るかを user (= owner) 判断。
- **Exit criteria**: tag push で signed exe + winget manifest update PR が自動化される。

### B. i18n / accessibility (現在 scope 外、優先度は feedback 次第)

- **動機**: HUD ラベル ("Mode", "Thickness", "Opacity", "Refresh:", "Hotkeys") が英語固定。日本語環境で読書補助として使うなら日本語化したいかもしれない、しかし HUD は overlay の隅でちらっと見るだけのものでもあるので、需要は不明。
- **やり方**:
  - `HudConfig` に `locale: Locale` (enum `En` / `Ja` / `Auto`) を追加
  - `mode_label` / hotkey help section の string を locale-resolved table から引く
  - `Auto` は `GetUserDefaultLocaleName` で OS から取得
- **Accessibility**:
  - Screen reader 対応 (Narrator): overlay HWND は WS_EX_TOOLWINDOW + WS_EX_TRANSPARENT で focus を取らないので Narrator は素通り。 HUD は読み上げ対象にしたいなら別 HWND 化 (重い)
  - High contrast mode: `SystemParametersInfo(SPI_GETHIGHCONTRAST)` を probe して mask 色を反転、indicator を背景色との対比で再選択
- **Exit criteria**: 言語切替が settings で可能 / High contrast 環境で overlay が破綻しない。
- **Open question**: 「薄い読書ツール」志向 (ADR-0011) に対して i18n table と locale switch を導入するコストが妥当か? user 判断待ち。

### C. User feedback 駆動の改善 (具体的アイテム未定)

- HUD の表示位置をユーザーが選べるようにする (右上固定を上書き)
- Slit の色をユーザーが選べるようにする (現状 `Rgba::DEFAULT_MASK` 固定)
- マウスではなくキャレット位置に追従するモード
- ... etc

これらは想定であって committed ではない。実際に届いた issue から優先度を引き出す。

## 明示的に Out of Scope (Phase η では着手しない)

- **TOML / JSON config file** — ADR-0011 / [[project_slim_reading_tool_philosophy]] により compile-time const を維持。ユーザーから「個別環境で値を上書きしたい」需要が出たら再検討。
- **`linerule-platform-linux` / `linerule-platform-macos`** — cs も rs も Windows 専用設計 (DirectComposition 依存)。クロスプラットフォーム化は別プロジェクト相当。
- **Plugin system / scripting** — overlay の責務範囲を超える。
- **GPU 専有レンダラ最適化** — 現行の DirectComposition + Direct2D pipeline で実機 60fps を確認済み、ADR-0011 の slim doctrine と整合しない。

## Pending decisions

| 判断項目 | 待っているもの |
|---|---|
| Code signing cert の有無・種別 | owner の予算判断 |
| Winget publish の優先度 | ユーザー数の見立て (現状ほぼ自家用) |
| i18n テーブル導入 | 日本語環境ユーザーからのフィードバック |
| Accessibility / High contrast | 該当環境ユーザーからの報告 |

## Exit criteria for this draft

- 上記項目のいずれかについて **具体的な commit / 着手の合意** が user との対話で取れた時点で、本 draft を該当 ADR (例: `docs/adr/0014-code-signing.md`) に昇格し、本 doc は更新履歴を残しつつアーカイブする。
- Phase η が完了したら `docs/roadmap/phase-eta.md` の status を `Closed` に更新し、Phase θ 用 doc に引き継ぐ。

## 関連

- [[../adr/0001-port-from-csharp]]: cs → rs 全面リライト判断。本 Phase で cs port は機能 parity に到達した
- [[../adr/0011-phase-j-slim-down]]: 「薄い読書ツール」doctrine。本 doc の Out of Scope の論拠
- [[../adr/0012-foreground-hook-and-hud-telemetry]]: cs-port 漏れ補完 (Phase ζ 後)
- [[../adr/0013-hud-notification-from-app-errors]]: Phase H PR-E 完了 (`AppError::class()` 消費)
