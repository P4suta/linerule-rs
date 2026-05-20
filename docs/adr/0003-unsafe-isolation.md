# ADR 0003 — `unsafe` を FFI 境界 1 ファイルに集約する

- 日付: 2026-05-20
- ステータス: accepted
- 提案者: P4suta

## 文脈

`linerule-platform-windows` は Win32 / COM / DirectComposition / Direct2D / DirectWrite / D3D11 を直接叩く。`windows` crate (Microsoft 謹製) の Win32 API は実質すべて `unsafe fn` であり、これを呼ぶ以上 `unsafe` ブロックは避けられない。

ユーザ要件: 「unsafe は禁止」 ([[linerule-rs-architecture-priority]] / [[no-suppress-warnings]] / 本セッション)。だが click-through + per-pixel α + Alt+Tab 非表示の overlay は `DirectComposition` + `WS_EX_LAYERED` + `WS_EX_NOREDIRECTIONBITMAP` + `WS_EX_TOOLWINDOW` を要し、winit / winsafe / wgpu / tiny-skia いずれの抽象でも完全 `unsafe` ゼロは技術的に不可能であることを Phase C 計画段階の調査で確認。

## 決定

`unsafe` を **`crates/linerule-platform-windows/src/win32_ffi.rs` の 1 ファイル**に集約する。具体的には:

1. `win32_ffi.rs` の冒頭で:
   ```rust
   #![allow(
       unsafe_code,
       reason = "FFI 境界。Win32 / COM API は windows crate 経由でも全部 unsafe fn。
                 他のモジュールは #![forbid(unsafe_code)]、本ファイルでのみ集約する。
                 詳細は ADR-0003。"
   )]
   ```
2. `crates/linerule-platform-windows/src/` 配下の他の `.rs` ファイルは **全部** ファイル先頭に `#![forbid(unsafe_code)]` を宣言する
3. `lib.rs` も `#![cfg(windows)]` + `#![deny(unsafe_op_in_unsafe_fn)]` を保持しつつ、自身では `unsafe` を書かない（`pub mod` 宣言だけ）
4. `win32_ffi.rs` は薄い safe wrapper の集まり:
   - 各 `pub fn` は 1〜数行の `unsafe { windows::Win32::...::CallW(...) }` + エラー Result 化のみ
   - 引数型・戻り型は windows crate 由来でも、関数本体の unsafe は局所化
   - 各 `unsafe { }` ブロックの直前に `// SAFETY: …` コメントを必須化
5. `wndproc.rs` 内の dispatch ロジックも `forbid(unsafe_code)`。`extern "system" fn` 本体は `win32_ffi::overlay_wnd_proc` として `win32_ffi.rs` 側に置く

## 範囲

| ファイル | unsafe 方針 |
|---|---|
| `win32_ffi.rs` | `#![allow(unsafe_code, reason = "...")]`、`unsafe extern "system" fn` 含む |
| `lib.rs` | `unsafe` 出てこない（`#![deny(unsafe_op_in_unsafe_fn)]` でガード） |
| `error.rs`, `messages.rs`, `overlay_state.rs`, `window_class.rs`, `wndproc.rs`, `overlay_window.rs`, `ex_style_snapshot.rs`, `monitor_info.rs`, `windows_app.rs` | 全部 `#![forbid(unsafe_code)]` |
| examples / tests | `#![forbid(unsafe_code)]`（smoke test の `main.rs` も含む） |

## 機械検証

```bash
# unsafe を含むファイルは win32_ffi.rs のみであることを保証
grep -lr '^#!\[allow(unsafe_code' crates/linerule-platform-windows/src/ \
  | grep -v '/win32_ffi.rs$' \
  && exit 1 || true

# win32_ffi.rs 以外は forbid(unsafe_code) を含むこと
for f in $(find crates/linerule-platform-windows/src -name '*.rs' ! -name 'win32_ffi.rs' ! -name 'lib.rs'); do
  grep -q '^#!\[forbid(unsafe_code\b' "$f" || (echo "missing forbid: $f" && exit 1)
done
```

`xtask lint` に上記チェックを組み込む（将来）。

## 結果

- user-facing コード（dispatch, OverlayWindow, MonitorInfo, run_message_pump 等）は `forbid(unsafe_code)`。`grep unsafe` の review attention surface は `win32_ffi.rs` 内に閉じる
- C# 版 (linerule-cs) が `[UnmanagedCallersOnly]` + WndProc static dispatch を `Linerule.Platform.Windows.OverlayWindow` 1 ファイルにある程度集約していたのと方針一致
- Phase D の DirectComposition / Direct2D / DWrite / D3D11 wrapper も同じファイル (`win32_ffi.rs`) に追加するか、関連サブモジュール (`win32_ffi/dcomp.rs`, `win32_ffi/d2d.rs` 等) を作って `win32_ffi.rs` 親に対する `mod` 宣言で吸収する。**`#![allow(unsafe_code)]` のファイル数を増やすときは ADR を要する**
- ユーザの「unsafe 禁止」要件への対応:
  - user code（dispatch ロジック、状態管理、メッセージポンプ）からは unsafe 完全に消える
  - FFI 境界の薄い wrapper だけが unsafe を持つ
  - これにより review/audit の attention surface は最小化、`grep unsafe` の戦場が「ファイル 1 つ」に絞られる
