# 0007 — Debug Build profile (`dist-dev`) と panic 戦略の非対称性

**Status:** Accepted (Phase H groundwork, 2026-05-20).

**See also:** [[0003-unsafe-isolation]] (`unsafe` を `win32_ffi/` に集約), [[0004-coverage-policy]] (coverage gate), Phase H plan の H1/H3。

## 文脈

これまで CI で配布される Windows artifact は `release-build (win-x64, native)` job が生成する `linerule-win-x64` (EXE のみ、PDB なし) だけだった。`Cargo.toml [profile.release]` は:

```toml
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"
opt-level = 3
overflow-checks = false
```

shipping characteristic として正しい設定だが、開発者が深掘り debug したいときに以下が手に入らない:

1. **PDB (Program Database / Windows debug symbols)**: シンボルが strip されて
   crash dump の backtrace が address のみになる。
2. **`catch_unwind` の実機検証経路**: `linerule-platform-windows/src/win32_ffi/core.rs::overlay_wnd_proc` は `catch_unwind(AssertUnwindSafe(|| wndproc::dispatch(...)))` で wndproc 内 panic を吸収して `DefWindowProcW` フォールバックする設計だが、`panic = "abort"` 下では **panic で即プロセス abort** されるため catch_unwind は effectively dead。
3. **`overflow-checks`**: `overflow-checks = false` で integer overflow が wrap される。debug 時には panic で気づきたい。

これは ADR-0002 や 0003 のいずれにも明文化されていなかった「未文書化の死コード」状態。

## 判断

**新規 `[profile.dist-dev]` ("distributable debug") を追加し、Debug Build artifact を CI で配布する。Release Build profile は変更しない。**

### `[profile.dist-dev]`

```toml
[profile.dist-dev]
inherits = "release"     # 最適化は維持 (実用速度のまま debug 可能)
debug = "full"           # PDB 完全シンボル
strip = "none"           # シンボル残し
lto = "thin"             # `fat` + `debug=full` は PDB 不整合の既知問題
panic = "unwind"         # catch_unwind 経路を実機検証可能に
overflow-checks = true   # 開発時の integer overflow を panic にする
incremental = false      # CI 再現性
```

CI に `debug-build (win-x64, native, PDB)` job を追加し、`linerule-win-x64-debug` artifact として `linerule.exe` + `linerule.pdb` 両方を upload する。retention 14 days。

### Release Build を `panic = "unwind"` にしない理由

trade-off:
- **Pros (unwind)**: `catch_unwind` が wndproc panic から実際に救済する。crash dump callback も同じ。
- **Cons (unwind)**: バイナリサイズ +10-15%、unwinding tables 分のメモリ常駐、コンパイル時間増。
- ADR レベルの判断: shipping binary では「予期しない panic は即死亡してログだけ残す」方が単純で予測可能。catch_unwind を実機検証する経路は **Debug Build artifact** で代用する。

Release を unwind に切り替える判断は本 ADR の範囲外、Issue で起票して別 PR で評価する。

### `lto = "thin"` を選ぶ理由

`fat` + `debug = "full"` の組み合わせは rustc の既知の不整合 (LLVM ProGuard / PDB の symbol mapping ズレ) がある。`thin` は LTO の最適化を概ね維持しつつ PDB consistency を保つ。release は `fat` のまま (release では debug=不要なので無問題)。

## 結果

- 新規 `[profile.dist-dev]` を `Cargo.toml` に追加 (~10 LOC)
- `.github/workflows/ci.yml` に `debug-build` job を追加 (~40 LOC)
- `Justfile` に `just build-debug` recipe を追加 (~5 LOC, ローカル検証用)
- CI artifact `linerule-win-x64-debug` (EXE 2MB + PDB 27MB ≒ 30MB) が download 可能
- `target/dist-dev/linerule.{exe,pdb}` がローカルでも生成可能
- Phase H の PR-A スコープ

### Release との非対称性

| 項目 | `release` | `dist-dev` |
|---|---|---|
| panic | abort | **unwind** |
| catch_unwind | effectively dead | **live** |
| PDB | not uploaded | **uploaded** |
| strip | symbols | **none** |
| lto | fat | **thin** |
| overflow-checks | false | **true** |

この非対称性は **意図したもの**。release は shipping characteristic を維持し、`dist-dev` は depth-debug characteristic を提供する。両 artifact を並列に CI で出すことで「ユーザーに渡すバイナリ」と「開発者が解析するバイナリ」を分離する。

## 検討した代替案

### A. Release Build に PDB だけ同梱

却下: shipping binary の strip 戦略を変えずに PDB だけ追加することは可能だが、`panic = "abort"` + `lto = "fat"` のままでは catch_unwind が dead、PDB 自体の利用価値が半減する。

### B. 1 つの profile で全部賄う (release を `panic = "unwind"` + `strip = "none"`)

却下: バイナリサイズ +25-30%、shipping characteristic を変える ADR 規模の判断が要る。

### C. `dist-dev` を `inherits = "dev"` にする

却下: `dev` (opt-level=0) では実機 QA に向かないほど遅い。Debug Build artifact の用途は「PDB 付きで適度に速いバイナリ」なので release を base にする。

## 関連

- ADR-0002 §4 (RAII)、§7 (unsafe 局所化) — `catch_unwind` 救済経路が live になることで wndproc dispatch の不変条件が実機検証可能になる
- ADR-0003 — unsafe は `win32_ffi/` に閉じる方針は不変
- `linerule-rs-version-bump-cautious` — 本変更は `fix(ci):` で patch bump
- `linerule-rs-branch-protection` — PR merge 後の別 PR で必須 14 → 15 チェックに `debug build (win-x64, native, PDB)` を追加
