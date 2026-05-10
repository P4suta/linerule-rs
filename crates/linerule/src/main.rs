// The binary entry-point. Subcommand dispatch + tracing/color-eyre setup.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use std::io;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, eyre};
use tracing::instrument;
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Digital reading ruler — a frameless click-through always-on-top
/// overlay that follows the cursor.
#[derive(Debug, Parser)]
#[command(name = "linerule", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the overlay (default).
    Run,

    /// Inspect or modify the user configuration.
    #[command(subcommand)]
    Config(ConfigCommand),
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print the resolved config (file → defaults).
    Show,
    /// Print the resolved config-file path (whether or not it exists).
    Path,
    /// Open the config file in `$EDITOR`. Creates a default file if missing.
    Edit,
}

#[instrument]
fn main() -> Result<()> {
    color_eyre::install()?;

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().with_writer(io::stderr).compact())
        .with(ErrorLayer::default())
        .init();

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Run) {
        Command::Run => run_overlay(),
        Command::Config(ConfigCommand::Show) => config_show(),
        Command::Config(ConfigCommand::Path) => config_path(),
        Command::Config(ConfigCommand::Edit) => config_edit(),
    }
}

#[instrument]
fn run_overlay() -> Result<()> {
    // Probe the platform layer up-front so the user sees a clear error
    // *before* the binary pretends an event loop is starting. The real
    // surface / hotkey / mouse impls land in task #11.
    let _surface = linerule_platform::open_overlay()
        .map_err(|e| eyre!("could not open overlay surface: {e}"))?;
    let _hotkeys =
        linerule_platform::open_hotkeys().map_err(|e| eyre!("could not open hotkey host: {e}"))?;
    let _mouse =
        linerule_platform::open_mouse().map_err(|e| eyre!("could not open mouse tracker: {e}"))?;

    Err(eyre!(
        "platform impls compiled — event loop wiring lands in task #12 (see plan file)"
    ))
}

#[instrument]
fn config_show() -> Result<()> {
    let path = linerule_config::default_path()
        .map_err(|e| eyre!("cannot resolve default config path: {e}"))?;
    if path.exists() {
        let cfg = linerule_config::load(&path)
            .map_err(|e| eyre!("failed to load config from {}: {e}", path.display()))?;
        println!("{cfg:#?}");
    } else {
        println!("# no config file at {} — showing defaults", path.display());
        println!("{:#?}", linerule_config::Config::default());
    }
    Ok(())
}

#[instrument]
fn config_path() -> Result<()> {
    let path: PathBuf = linerule_config::default_path()
        .map_err(|e| eyre!("cannot resolve default config path: {e}"))?;
    println!("{}", path.display());
    Ok(())
}

#[instrument]
fn config_edit() -> Result<()> {
    Err(eyre!(
        "config edit is not wired up yet — see task #12 in the plan file"
    ))
}
