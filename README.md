# linerule — デジタル定規

[![CI](https://github.com/P4suta/linerule-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/P4suta/linerule-rs/actions/workflows/ci.yml)
[![License: Apache-2.0 OR MIT](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](#%E3%83%A9%E3%82%A4%E3%82%BB%E3%83%B3%E3%82%B9)

画面の上に**透明な薄い定規**を浮かせて、マウスカーソルにくっついて
動かすだけのアプリです。長い文章を読んでいるときに「いま何行目を
読んでいたっけ?」となるのを防ぎます。

> 対応 OS: Windows 10 / 11 のみ(v0.1)
>
> macOS / Linux は v0.2 で来ます

## こんな人に

- Kindle / 電子書籍リーダー / PDF / ブラウザの長文を読むのが疲れる
- 行を見失うので指で追いたくなる
- 縦書き(青空文庫など)でも同じ補助がほしい
- 既存アプリに改造を加えたくない(linerule は完全に画面の上に
  *被せる* だけ — 元アプリは何も知りません)

## 2 つのモードを切り替えて使います

| モード   | 何が出る                                              | 用途                       |
| -------- | ----------------------------------------------------- | -------------------------- |
| 横マスク | カーソルの**高さ**だけ見えて、その上下が暗くなる      | 横書き、1 行集中(典型)    |
| 縦マスク | カーソルの**位置**だけ見えて、その左右が暗くなる      | 縦書き、1 列集中(青空文庫)|

起動直後は横マスクで始まります。`Ctrl+Alt+R` を押すたびに

```
横マスク → 縦マスク → なし → 横マスク → ...
```

の順に切り替わります(「なし」は完全に何も出さない状態)。

## キー操作(初期値)

<!-- BEGIN GENERATED: hotkeys -->

| キー         | 何が起きる                                    |
| ---------- | ---------------------------------------------- |
| Ctrl+Alt+R | 4 モード(+ なし)を順に切り替え                   |
| Ctrl+Alt+P | 一時的に **完全 OFF**(もう一度押すと元に戻る)     |
| Ctrl+Alt+] | 帯を太くする                                    |
| Ctrl+Alt+[ | 帯を細くする                                    |
| Ctrl+Alt+= | 濃くする                                        |
| Ctrl+Alt+- | 薄くする                                        |
| Ctrl+Alt+Q | linerule を終了する(緊急脱出用 — 必ず効きます) |

<!-- END GENERATED: hotkeys -->

> キーが他のアプリと被って効かないときは設定ファイルで変更できます
> (下「設定を変える」参照)。

## 困ったとき

**画面が真っ暗で操作できない / マスクが邪魔で消せない:**
`Ctrl+Alt+Q` を押すと linerule が完全終了します。これは設定で
変えても*必ず*効くようにしてあります。

**画面の上に乗っているのに、クリックは下のアプリに通る:**
仕様です(*click-through*)。linerule は表示だけで、
キーボード/マウスの入力は全部下のアプリに素通しします。

**カーソルにくっついてこない:**
ディスプレイが複数あるときは、起動したモニタの中だけで動きます。
別モニタで使いたいときは linerule をそちらで再起動してください。

## インストール

(v0.1.0 リリース後ここに `linerule.exe` のダウンロード手順を入れます。
今は開発中なので「下の "開発者向け" を見て自分でビルド」してください。)

## 設定を変える

設定ファイルは TOML です。場所と内容は次のコマンドで分かります:

```sh
linerule config path     # 設定ファイルの場所を表示
linerule config show     # 現在の設定を表示(ファイルがなくても既定値)
linerule config edit     # $EDITOR で開く(ファイルがなければ作る)
```

設定ファイルの中身はこんな感じです(全項目とも省略可、省略すると
既定値):

```toml
[overlay]
# マスクの色。{ r, g, b, a } 各 0..=255。a が暗さ(透明=0、不透明=255)。
mask_color = { r = 8, g = 8, b = 8, a = 217 }   # 暗いほぼ黒、85% 不透明
thickness  = 28                                  # 隙間(スリット)の太さ(px)

[hotkeys]
cycle_mode  = "Ctrl+Alt+R"
pause       = "Ctrl+Alt+P"
thicker     = "Ctrl+Alt+]"
thinner     = "Ctrl+Alt+["
more_opaque = "Ctrl+Alt+="
less_opaque = "Ctrl+Alt+-"
quit        = "Ctrl+Alt+Q"
```

`Ctrl` / `Alt` / `Shift` / `Win` の組み合わせと
A〜Z / `[` / `]` / `=` / `-` / 矢印キーが指定できます。

## 開発者向け

ビルド / テストはぜんぶ Docker の中で動きます(ホスト側に rust も
要りません)。詳細は [`CONTRIBUTING.md`](CONTRIBUTING.md) と
[`docs/adr/`](docs/adr/) 参照。

```sh
just                 # 使えるレシピ一覧
just build           # debug build
just test            # nextest で全テスト
just lint            # fmt + clippy + typos + strict-code + shear
just coverage        # llvm-cov、region 100% で gate
just windows-exe     # cargo-xwin で .exe を build → dist/ にコピー(Windows から起動できる)
```

> Windows 側から動作確認するときは `just build-windows` ではなく
> **`just windows-exe`** を使う(後者は `dist/linerule.exe` まで
> 同期する。`build-windows` だけだと古い `dist/` が残ったまま
> になり、最新の挙動が試せない)。

## ライセンス

[Apache-2.0](LICENSE-APACHE) と [MIT](LICENSE-MIT) のデュアルライセンス。
お好きな方をどうぞ。
