// xtask — internal dev automation. Replaces ad-hoc shell scripts so
// every gate is type-checked, testable, and runs through the same
// stable Rust toolchain as the rest of the workspace.
//
// Subcommands today:
//   strict-code   — defensive lint gate (forbidden patterns)
//
// Run via `just strict-code` (or `cargo run -p xtask -- strict-code`
// from inside the dev container).

#![forbid(unsafe_code)]

mod strict_code;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "linerule dev automation", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Defensive grep gate — reject known bug-source patterns
    /// (`#[allow]`, bare `TODO`, `unsafe` in pure crates, `println!`
    /// in libraries, `on.schedule` in workflows, etc.). See
    /// `crates/xtask/src/strict_code.rs` for the rule list.
    StrictCode,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::StrictCode => strict_code::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(violations) => {
            eprintln!();
            eprintln!(
                "strict-code: {violations} violation(s) found — refactor the offending sites; do not silence.",
            );
            ExitCode::from(1)
        }
    }
}
