//! linerule-app
//!
//! Entry point for the single binary `linerule.exe`. Does CLI parsing, logging
//! init, and wiring into `linerule-platform-windows`; no domain logic.
//!
//! In GUI mode `windows_subsystem = "windows"` keeps the console closed; the
//! `console` module attaches/allocates one only when a CLI command
//! (`diagnostics`, `version`, `--cli`) is requested.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI/boundary crate uses stdout/stderr directly"
)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "prefer unreachable_pub over redundant_pub_crate"
)]
#![allow(
    clippy::missing_const_for_fn,
    clippy::unnecessary_wraps,
    reason = "boot fns may gain side effects later"
)]

use clap::Parser;

mod boot;
mod cli;
mod console;
mod crash_dump;
mod error;
mod event_ring;
mod logging;
mod version;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    boot::boot(cli)
}
