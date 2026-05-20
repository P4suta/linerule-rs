//! linerule-app
//!
//! 単一バイナリ `linerule.exe` のエントリーポイント。CLI 解析・ロギング
//! 初期化・`linerule-platform-windows` の `windows_app::run` への結線のみを
//! 行い、ドメインロジックは書かない。
//!
//! GUI モードでは `windows_subsystem = "windows"` によりコンソールが開かず、
//! CLI 系コマンド (`diagnostics`, `version`, `--cli`) が要求された時にだけ
//! `console` モジュールがコンソールを attach / alloc する。

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "linerule-app は CLI / boundary なので stdout / stderr を直接使う"
)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "pub(crate) は意図表現。unreachable_pub と redundant_pub_crate の衝突は前者を優先"
)]
#![allow(
    clippy::missing_const_for_fn,
    clippy::unnecessary_wraps,
    reason = "ブート系関数は将来副作用を追加する余地を残す"
)]

use clap::Parser;

mod boot;
mod cli;
mod console;
mod crash_dump;
mod error;
mod logging;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    boot::boot(cli)
}
