//! linerule-rs build automation (xtask pattern).
//!
//! Subcommands:
//! - `dep-graph`: parses `cargo metadata` and asserts the one-way dependency
//!   `linerule-app` → `linerule-platform-windows` → `linerule-core`. This is
//!   the only project-specific lint that no clippy / cargo-deny configuration
//!   can replicate.
//! - `lint`: runs the full local lint pipeline (fmt, clippy, deny, typos,
//!   actionlint, dep-graph, machete).
//! - `ci`: replays the CI test/build matrix locally.
//!
//! xtask is a CLI boundary: stdout/stderr printing and panic-on-misconfig
//! are intentional, hence the narrow crate-level lint relaxations below.

#![forbid(unsafe_code)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "xtask is a CLI tool; printing is its job"
)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "xtask is a boundary; panicking on misconfiguration is correct"
)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "pub(crate) on submodule funcs is intent-signalling; rust's \
              unreachable_pub conflicts with clippy's redundant_pub_crate, \
              and unreachable_pub wins"
)]

mod ci;
mod dep_graph;
mod lint;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "xtask",
    about = "linerule-rs build automation",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Assert the one-way crate dependency graph.
    DepGraph,
    /// Run the full local lint pipeline.
    Lint,
    /// Replay the CI test/build matrix locally.
    Ci,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::DepGraph => dep_graph::run(),
        Command::Lint => lint::run(),
        Command::Ci => ci::run(),
    }
}
