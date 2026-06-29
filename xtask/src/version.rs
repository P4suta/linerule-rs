//! `xtask version --channel <dev|nightly|stable> [--date YYYYMMDD]` — print the
//! canonical channel-aware version string. This is the single source of the
//! *format*: CI exports the result as `LINERULE_VERSION` so `linerule-app`'s
//! `build.rs` stamps it verbatim into the shipped binary, and `nightly.yml`
//! names the nightly artifact from it.
//!
//!   dev     → `0.4.1-dev+g<sha>`
//!   nightly → `0.4.1-nightly.<date>+g<sha>`
//!   stable  → `0.4.1`                        (clean; the release tag itself)
//!
//! The base `X.Y.Z` triple is `env!("CARGO_PKG_VERSION")`: xtask inherits
//! `version.workspace = true`, so it equals `[workspace.package] version` — the
//! value release-please bumps. No TOML parsing needed. The git sha is resolved
//! at call time; when git / the `.git` dir is absent the metadata is omitted.
//!
//! Release *bumping* is NOT here — release-please owns version/tag/CHANGELOG.
//! This subcommand only formats a build identity for the dev/nightly/stable lanes.

use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};

/// The base `X.Y.Z` triple, shared across the workspace via
/// `version.workspace = true` (equals `[workspace.package] version`).
const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build channel a version string is formatted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Channel {
    /// Local / ad-hoc builds: `X.Y.Z-dev+g<sha>`.
    Dev,
    /// Scheduled unsigned builds: `X.Y.Z-nightly.<date>+g<sha>`.
    Nightly,
    /// Released builds: clean `X.Y.Z` (the release tag itself).
    Stable,
}

/// Arguments for `xtask version`.
#[derive(Debug, Args)]
pub(crate) struct VersionArgs {
    /// Build channel to format the version for.
    #[arg(long, value_enum)]
    channel: Channel,
    /// Build date (YYYYMMDD); required for the nightly channel.
    #[arg(long)]
    date: Option<String>,
}

/// Print the channel-aware version string to stdout.
///
/// # Errors
/// When the nightly channel is requested without a valid `--date`.
pub(crate) fn run(args: &VersionArgs) -> Result<()> {
    let sha = git_short_sha();
    let version = compute(
        BASE_VERSION,
        args.channel,
        args.date.as_deref(),
        sha.as_deref(),
    )?;
    println!("{version}");
    Ok(())
}

/// Pure formatter — unit-tested without touching git or the filesystem.
fn compute(base: &str, channel: Channel, date: Option<&str>, sha: Option<&str>) -> Result<String> {
    let meta = sha.map(|s| format!("+g{s}")).unwrap_or_default();
    Ok(match channel {
        Channel::Stable => base.to_owned(),
        Channel::Dev => format!("{base}-dev{meta}"),
        Channel::Nightly => {
            let date = date.context("--date YYYYMMDD is required for the nightly channel")?;
            if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
                bail!("--date must be 8 digits (YYYYMMDD), got '{date}'");
            }
            format!("{base}-nightly.{date}{meta}")
        },
    })
}

/// Resolve the short git sha (without the `g` prefix `compute` adds). Returns
/// `None` when git or the `.git` dir is absent, so source-tarball builds drop
/// the metadata rather than failing.
fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!sha.is_empty()).then_some(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_is_the_clean_base() {
        assert_eq!(
            compute("0.4.1", Channel::Stable, None, Some("abc1234")).unwrap(),
            "0.4.1"
        );
    }

    #[test]
    fn dev_carries_channel_and_sha() {
        assert_eq!(
            compute("0.4.1", Channel::Dev, None, Some("abc1234")).unwrap(),
            "0.4.1-dev+gabc1234"
        );
    }

    #[test]
    fn dev_without_sha_drops_metadata() {
        assert_eq!(
            compute("0.4.1", Channel::Dev, None, None).unwrap(),
            "0.4.1-dev"
        );
    }

    #[test]
    fn nightly_embeds_date_and_sha() {
        assert_eq!(
            compute("0.4.1", Channel::Nightly, Some("20260629"), Some("abc1234")).unwrap(),
            "0.4.1-nightly.20260629+gabc1234"
        );
    }

    #[test]
    fn nightly_requires_a_date() {
        assert!(compute("0.4.1", Channel::Nightly, None, Some("abc1234")).is_err());
    }

    #[test]
    fn nightly_rejects_a_malformed_date() {
        for bad in ["2026-06-29", "20260", "2026062x", ""] {
            assert!(
                compute("0.4.1", Channel::Nightly, Some(bad), None).is_err(),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn channel_parse_rejects_unknown() {
        assert!(Channel::from_str("canary", false).is_err());
        assert_eq!(
            Channel::from_str("nightly", false).unwrap(),
            Channel::Nightly
        );
    }
}
