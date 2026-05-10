// The binary entry-point. Subcommand dispatch + tracing/color-eyre setup.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, eyre};
use linerule_config::{Config, HotkeyMap};
use linerule_core::{Action, HotkeyEffect, State};
use tracing::instrument;
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Digital reading ruler — a frameless click-through always-on-top
/// overlay that follows the cursor.
#[derive(Debug, Parser)]
#[command(name = "linerule", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
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
    match cli.command.unwrap_or(CliCommand::Run) {
        CliCommand::Run => run_overlay(),
        CliCommand::Config(ConfigCommand::Show) => config_show(),
        CliCommand::Config(ConfigCommand::Path) => config_path(),
        CliCommand::Config(ConfigCommand::Edit) => config_edit(),
    }
}

#[instrument]
fn run_overlay() -> Result<()> {
    let cfg = load_or_default()?;
    let mut initial_state = State::default();
    initial_state.config = cfg.overlay;
    let bindings = hotkey_bindings(&cfg.hotkeys);
    linerule_platform::run(initial_state, &bindings)
        .map_err(|e| eyre!("overlay event loop failed: {e}"))
}

#[instrument]
fn config_show() -> Result<()> {
    let cfg = load_or_default()?;
    println!("{cfg:#?}");
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
    let path = linerule_config::default_path()
        .map_err(|e| eyre!("cannot resolve default config path: {e}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| eyre!("create config dir {}: {e}", parent.display()))?;
    }
    if !path.exists() {
        let body = toml::to_string_pretty(&Config::default())
            .map_err(|e| eyre!("seed default toml: {e}"))?;
        fs::write(&path, body).map_err(|e| eyre!("seed config {}: {e}", path.display()))?;
    }
    let editor = env::var("EDITOR").unwrap_or_else(|_| {
        if cfg!(windows) {
            "notepad".into()
        } else {
            "vi".into()
        }
    });
    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|e| eyre!("spawn editor {editor:?}: {e}"))?;
    if !status.success() {
        return Err(eyre!("editor {editor:?} exited {status}"));
    }
    Ok(())
}

fn load_or_default() -> Result<Config> {
    let path = linerule_config::default_path()
        .map_err(|e| eyre!("cannot resolve default config path: {e}"))?;
    if path.exists() {
        linerule_config::load(&path).map_err(|e| eyre!("load config {}: {e}", path.display()))
    } else {
        Ok(Config::default())
    }
}

fn hotkey_bindings(map: &HotkeyMap) -> Vec<(String, HotkeyEffect)> {
    vec![
        (
            map.cycle_mode.clone(),
            HotkeyEffect::Apply(Action::CycleMode),
        ),
        (map.pause.clone(), HotkeyEffect::Apply(Action::TogglePause)),
        (
            map.thicker.clone(),
            HotkeyEffect::Apply(Action::BumpThickness(2)),
        ),
        (
            map.thinner.clone(),
            HotkeyEffect::Apply(Action::BumpThickness(-2)),
        ),
        (
            map.more_opaque.clone(),
            HotkeyEffect::Apply(Action::BumpOpacity(15)),
        ),
        (
            map.less_opaque.clone(),
            HotkeyEffect::Apply(Action::BumpOpacity(-15)),
        ),
        (map.quit.clone(), HotkeyEffect::Quit),
    ]
}
