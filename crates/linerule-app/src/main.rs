//! Entry point for `linerule.exe`: CLI parsing, logging, wiring; no domain logic.
//!
//! `windows_subsystem = "windows"` keeps the console closed in GUI mode; the
//! platform runtime attaches one only for a CLI command.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![forbid(unsafe_code)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI/boundary crate uses stdout/stderr directly"
)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "prefer unreachable_pub over redundant_pub_crate"
)]

use clap::Parser;
use std::process::ExitCode;

mod boot;
mod cli;
mod crash_dump;
mod error;
mod event_ring;
mod logging;
mod storage;
mod version;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match boot::boot(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        },
    }
}
