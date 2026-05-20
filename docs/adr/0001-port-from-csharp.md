# ADR 0001 — linerule-cs から linerule-rs への作り替え

- 日付: 2026-05-20
- ステータス: accepted
- 提案者: P4suta

## 文脈

`linerule-cs` (C# / .NET 10 / WS_EX_LAYERED + DirectComposition オーバーレイ)
は最終フェーズで `CsWin32` 直叩き + COM RCW + `[UnmanagedCallersOnly]` WndProc
の集合体に近づき、C# 言語ランタイムの旨味（GC, async, 例外, IDisposable, リッチな
型推論）が消費されない領域が支配的になった。AOT publish を必須ゲート化した結果、
`IL2xxx` / `BannedSymbols` / `Phase<TIn,TOut>` Kleisli BootDag といった
「Rust の制約を C# に逆輸入する」作業が増え、メンタルモデルと実装のずれが拡大
していた。

リリースビルドが本質的に AOT 等価で、`unsafe` を明示できる Rust の方が、現在の
linerule の実態 (`dcomp.dll` を直叩きするネイティブオーバーレイ) と一致する。

## 決定

`linerule-cs` を `linerule-rs` (新規 Cargo workspace) として Rust で完全リライト
する。旧 `linerule` (winit + wgpu + vello + peniko) は参照せず、新規スタートで
設計を磨く。

技術選択:

- グラフィックス: `windows` クレートで DirectComposition + Direct2D + DXGI + D3D11
  を直接叩く (`linerule-cs` の dcomp 直叩き構成を Rust で素直に翻訳)
- ワークスペース: 中粒度 4 クレート (`linerule-core`, `linerule-platform-windows`,
  `linerule-app`, `xtask`)
- イベントログ: `tracing` + `tracing-subscriber` + `tracing-appender` の JSONL
- フロントエンド: 1 バイナリ `linerule.exe` (`windows_subsystem = "windows"` +
  サブコマンドで GUI / CLI 切替)
- 最重要指針: **アーキテクチャの美しさ** (ADR 0002 で成文化)

## 旧 ADR の処遇

| 旧 ADR | 件名 | 処遇 | 新 ADR / 備考 |
|---|---|---|---|
| 0001 | tech-stack (WinAppSDK + CsWin32 + Tomlyn + xUnit v3) | superseded | 本 ADR |
| 0002 | state-model (State / StateDelta / reduce) | inherited | `linerule-core::state` で同じ構造を保つ |
| 0003 | platform-abstraction (IOverlaySurface / IHotkeyHost) | dropped | Rust では port-and-adapter trait を作らず具象呼び |
| 0004 | windows-only-mvp | inherited | 同じ |
| 0005 | build-environment-exception (Docker-first) | inherited | `Dockerfile` + `compose.yml` で踏襲 |
| 0006 | workspace-lints-single-source | inherited | `[workspace.lints]` で実現 |
| 0007 | release-pipeline | superseded | windows-latest native ビルド出力を artifact 化 |
| 0008 | ci-strategy (SHA pin, coverage advisory) | inherited | `.github/workflows/ci.yml` で踏襲 |
| 0009 | transparency-via-dcomp | **inherited (core)** | 新 ADR (将来) で `linerule-platform-windows::overlay_window` として記述 |
| 0010 | aot-readiness-gap (IL2xxx / BannedSymbols / IPathfulSink) | dropped | Rust release ビルド = AOT 等価。IL2xxx machinery 不要 |
| 0011 | tunables-as-typed-toml-records | dropped | TOML 廃止済 |
| 0011b | config-integrity-layers | dropped | 同上 |
| 0012 | sqlite-writer-only-event-store | **dropped** | tracing JSONL に置換 (将来 ADR で記述) |
| 0013 | phase-dag-composition-root | dropped | `fn boot() -> Result<…, BootstrapError>` + `?` で十分 |
| 0014 | validation-applicative-and-severity-lattice | partial-inherited | `linerule-core::diagnostics::Severity` 程度に縮減 |
| 0015 | tunables-as-compile-time-constants | inherited | `UserConfig::DEFAULT` を Rust の `const` で表現 |
| 0016 | perceptual-opacity | inherited | `linerule-core::rgba::PerceptualOpacity` |
| 0017 | rolling-release-pipeline | superseded | リリース手順は新 release ADR で書き直す予定 |

番号は 0001 から振り直すため、本リポジトリの ADR 0009 は旧 ADR 0009 と無関係である。
横並びの参照事故を避けるための判断。

## 結果

- C# 由来の dead pattern (`ComLifetime` 手動 Release、`IPathfulSink`、`Phase<TIn,TOut>`
  Kleisli BootDag、リフレクション workaround の翻訳) を持ち込まない
- `Linerule.Diagnostics.Storage` (SQLite) は移植せず JSONL に置換
- 二バイナリ構成 (`linerule.exe` + `Linerule.exe`) は 1 バイナリに統合
- 古い ADR 番号を参照する文書 (CLAUDE.md, README.md, コミットログ) は本リポジトリでは
  すべて新 ADR 番号に書き換える
