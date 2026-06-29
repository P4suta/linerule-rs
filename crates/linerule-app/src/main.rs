//! Entry point for `linerule.exe`: CLI parsing, logging, wiring; no domain logic.
//!
//! `windows_subsystem = "windows"` keeps the console closed in GUI mode; the
//! `console` module attaches one only for a CLI command.

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
