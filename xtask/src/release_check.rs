//! Final, fail-closed validation over a staged stable-release directory.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;
use sha2::{Digest, Sha256};

const REQUIRED_ASSETS: &[&str] = &[
    "linerule.msixbundle",
    "linerule.appinstaller",
    "linerule-portable-x64.zip",
    "linerule-portable-arm64.zip",
    "linerule-sbom.cdx.json",
    "linerule-source.spdx",
    "SHA256SUMS.txt",
];

// v0.5.0's signed x64 portable EXE was 1,699,224 bytes. The 0.6 contract
// permits at most 20% growth in the main portable executable; the separately
// shipped self-contained Fluent settings host is intentionally not part of
// this binary-size budget.
const PORTABLE_X64_BASELINE_BYTES: u64 = 1_699_224;
const PORTABLE_EXE_MAX_GROWTH_PERCENT: u64 = 20;
const PE_MACHINE_X64: u16 = 0x8664;
const PE_MACHINE_ARM64: u16 = 0xAA64;

#[derive(Debug, Args)]
pub(crate) struct ReleaseCheckArgs {
    /// Directory containing the final signed stable assets.
    #[arg(long, default_value = "dist")]
    artifacts: PathBuf,
    /// Release tag that must equal `v<workspace-version>`.
    #[arg(long)]
    expected_tag: Option<String>,
    /// Permit unsigned artifacts only for a non-publishing packaging smoke run.
    #[arg(long)]
    allow_unsigned: bool,
}

pub(crate) fn run(args: &ReleaseCheckArgs) -> Result<()> {
    crate::policy::run()?;

    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .context("reading workspace metadata")?;
    let versions = metadata
        .packages
        .iter()
        .filter(|package| metadata.workspace_members.contains(&package.id))
        .map(|package| package.version.to_string())
        .collect::<BTreeSet<_>>();
    if versions.len() != 1 {
        bail!("workspace package versions are not identical: {versions:?}");
    }
    let version = versions
        .iter()
        .next()
        .context("workspace has no packages")?;

    check_expected_tag(version, args.expected_tag.as_deref())?;
    check_documented_version(metadata.workspace_root.as_std_path(), version)?;
    check_assets(&args.artifacts, version, args.allow_unsigned)?;
    check_ruleset(metadata.workspace_root.as_std_path())?;
    check_package_channel_contract(metadata.workspace_root.as_std_path())?;
    check_mise_lock(metadata.workspace_root.as_std_path())?;
    check_clean_tree()?;
    println!("release-check: ok ({version})");
    Ok(())
}

fn check_expected_tag(version: &str, expected_tag: Option<&str>) -> Result<()> {
    let Some(expected_tag) = expected_tag else {
        return Ok(());
    };
    let required = format!("v{version}");
    if expected_tag != required {
        bail!("release tag is `{expected_tag}`; expected `{required}`");
    }
    Ok(())
}

fn check_clean_tree() -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .context("running git status")?;
    if !output.status.success() {
        bail!("git status failed with {}", output.status);
    }
    if !output.stdout.is_empty() {
        bail!("release-check requires a clean working tree");
    }
    Ok(())
}

fn check_documented_version(root: &Path, version: &str) -> Result<()> {
    let readme = fs::read_to_string(root.join("README.md")).context("reading README.md")?;
    let expected = format!("Version {version} supports Windows 11 build 26100 or newer");
    if !readme.contains(&expected) {
        bail!("README.md does not contain `{expected}`");
    }
    let settings_manifest =
        fs::read_to_string(root.join("ui/linerule-settings/Package.appxmanifest"))
            .context("reading WinUI Package.appxmanifest")?;
    let expected = format!("Version=\"{version}.0\"");
    if !settings_manifest.contains(&expected) {
        bail!("WinUI Package.appxmanifest does not contain `{expected}`");
    }
    Ok(())
}

fn check_assets(directory: &Path, version: &str, allow_unsigned: bool) -> Result<()> {
    if !directory.is_dir() {
        bail!(
            "release artifact directory does not exist: {}",
            directory.display()
        );
    }
    for name in REQUIRED_ASSETS {
        let path = directory.join(name);
        if !path.is_file() {
            bail!("missing required release asset: {}", path.display());
        }
    }

    let appinstaller = fs::read_to_string(directory.join("linerule.appinstaller"))
        .context("reading linerule.appinstaller")?;
    for required in [
        "xmlns=\"http://schemas.microsoft.com/appx/appinstaller/2021\"",
        "releases/latest/download/linerule.msixbundle",
        "releases/latest/download/linerule.appinstaller",
        "Name=\"P4suta.linerule\"",
        &format!("Version=\"{version}.0\""),
        "<UpdateSettings>",
        "<OnLaunch",
        "HoursBetweenUpdateChecks=\"0\"",
        "ShowPrompt=\"true\"",
        "UpdateBlocksActivation=\"false\"",
        "<AutomaticBackgroundTask",
    ] {
        if !appinstaller.contains(required) {
            bail!("linerule.appinstaller does not contain `{required}`");
        }
    }

    verify_archive_contents(directory, version)?;
    verify_sbom(&directory.join("linerule-sbom.cdx.json"))?;
    verify_source_spdx(&directory.join("linerule-source.spdx"))?;
    if !allow_unsigned {
        verify_signatures(directory, version)?;
    }
    verify_checksums(directory)
}

fn verify_archive_contents(directory: &Path, version: &str) -> Result<()> {
    for (archive, machine) in [
        ("linerule-portable-x64.zip", PE_MACHINE_X64),
        ("linerule-portable-arm64.zip", PE_MACHINE_ARM64),
    ] {
        let archive_path = directory.join(archive);
        let entries = archive_entries(&archive_path)?;
        for required in [
            "linerule.exe",
            "settings/linerule-settings.exe",
            "linerule.portable",
            "LICENSES/MIT.txt",
            "LICENSES/Apache-2.0.txt",
        ] {
            if !entries.contains(required) {
                bail!("{archive} does not contain {required}");
            }
        }
        verify_pe_machine(
            &archive_file_bytes(&archive_path, "linerule.exe")?,
            machine,
            &format!("{archive}:linerule.exe"),
        )?;
        verify_pe_machine(
            &archive_file_bytes(&archive_path, "settings/linerule-settings.exe")?,
            machine,
            &format!("{archive}:settings/linerule-settings.exe"),
        )?;
        if archive == "linerule-portable-x64.zip" {
            let unpacked =
                tempfile::tempdir().context("creating portable executable inspection directory")?;
            extract_archive(&archive_path, unpacked.path())?;
            let actual = fs::metadata(unpacked.path().join("linerule.exe"))
                .context("reading x64 portable executable size")?
                .len();
            let maximum = PORTABLE_X64_BASELINE_BYTES
                .saturating_mul(100 + PORTABLE_EXE_MAX_GROWTH_PERCENT)
                / 100;
            if actual > maximum {
                bail!(
                    "x64 portable executable is {actual} bytes; the v0.5.0 baseline \
                     permits at most {maximum} bytes (+{PORTABLE_EXE_MAX_GROWTH_PERCENT}%)"
                );
            }
        }
    }

    let bundle = directory.join("linerule.msixbundle");
    let bundle_entries = archive_entries(&bundle)?;
    for required in ["linerule-x64.msix", "linerule-arm64.msix"] {
        if !bundle_entries.contains(required) {
            bail!("linerule.msixbundle does not contain {required}");
        }
    }

    let unpacked = tempfile::tempdir().context("creating bundle inspection directory")?;
    extract_archive(&bundle, unpacked.path())?;
    for (package, architecture) in [
        ("linerule-x64.msix", "x64"),
        ("linerule-arm64.msix", "arm64"),
    ] {
        let path = unpacked.path().join(package);
        let entries = archive_entries(&path)?;
        for required in [
            "AppxManifest.xml",
            "linerule.exe",
            "settings/linerule-settings.exe",
        ] {
            if !entries.contains(required) {
                bail!("{package} does not contain {required}");
            }
        }
        let machine = if architecture == "x64" {
            PE_MACHINE_X64
        } else {
            PE_MACHINE_ARM64
        };
        verify_pe_machine(
            &archive_file_bytes(&path, "linerule.exe")?,
            machine,
            &format!("{package}:linerule.exe"),
        )?;
        verify_pe_machine(
            &archive_file_bytes(&path, "settings/linerule-settings.exe")?,
            machine,
            &format!("{package}:settings/linerule-settings.exe"),
        )?;
        let manifest = archive_file(&path, "AppxManifest.xml")?;
        for expected in [
            "Name=\"P4suta.linerule\"".to_owned(),
            format!("Version=\"{version}.0\""),
            format!("ProcessorArchitecture=\"{architecture}\""),
            "MinVersion=\"10.0.26100.0\"".to_owned(),
            "MaxVersionTested=\"10.0.26200.0\"".to_owned(),
        ] {
            if !manifest.contains(&expected) {
                bail!("{package} manifest does not contain {expected}");
            }
        }
    }
    Ok(())
}

fn verify_sbom(path: &Path) -> Result<()> {
    let file = fs::File::open(path)
        .with_context(|| format!("opening CycloneDX SBOM {}", path.display()))?;
    let document: serde_json::Value = serde_json::from_reader(file)
        .with_context(|| format!("parsing CycloneDX SBOM {}", path.display()))?;
    if document
        .get("bomFormat")
        .and_then(serde_json::Value::as_str)
        != Some("CycloneDX")
    {
        bail!("linerule-sbom.cdx.json is not a CycloneDX document");
    }
    if document
        .get("specVersion")
        .and_then(serde_json::Value::as_str)
        != Some("1.6")
    {
        bail!("linerule-sbom.cdx.json is not CycloneDX 1.6");
    }

    let mut purls = Vec::new();
    collect_purls(&document, &mut purls);
    for ecosystem in ["cargo", "nuget"] {
        let prefix = format!("pkg:{ecosystem}/");
        if !purls
            .iter()
            .any(|purl| purl.to_ascii_lowercase().starts_with(&prefix))
        {
            bail!("linerule-sbom.cdx.json has no {ecosystem} package URLs");
        }
    }
    Ok(())
}

fn verify_source_spdx(path: &Path) -> Result<()> {
    let document = fs::read_to_string(path)
        .with_context(|| format!("reading source SPDX document {}", path.display()))?;
    for required in [
        "SPDXVersion: SPDX-2.1",
        "DataLicense: CC0-1.0",
        "DocumentName: linerule-rs",
        "SPDXID: SPDXRef-DOCUMENT",
    ] {
        if !document.lines().any(|line| line.starts_with(required)) {
            bail!("linerule-source.spdx does not contain `{required}`");
        }
    }
    if !document.contains("LicenseInfoInFile: MIT")
        || !document.contains("LicenseInfoInFile: Apache-2.0")
        || !document.contains("LicenseInfoInFile: CC0-1.0")
    {
        bail!("linerule-source.spdx does not declare every repository license");
    }
    Ok(())
}

fn collect_purls<'a>(value: &'a serde_json::Value, output: &mut Vec<&'a str>) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(purl) = object.get("purl").and_then(serde_json::Value::as_str) {
                output.push(purl);
            }
            for child in object.values() {
                collect_purls(child, output);
            }
        },
        serde_json::Value::Array(array) => {
            for child in array {
                collect_purls(child, output);
            }
        },
        _ => {},
    }
}

fn archive_entries(path: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("tar")
        .arg("-tf")
        .arg(path)
        .output()
        .with_context(|| format!("listing archive {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "tar failed to list {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .map(|line| line.trim_start_matches("./").replace('\\', "/"))
        .collect())
}

fn archive_file(path: &Path, name: &str) -> Result<String> {
    String::from_utf8(archive_file_bytes(path, name)?).context("archive member is not UTF-8")
}

fn archive_file_bytes(path: &Path, name: &str) -> Result<Vec<u8>> {
    let output = Command::new("tar")
        .arg("-xOf")
        .arg(path)
        .arg(name)
        .output()
        .with_context(|| format!("reading {name} from {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "tar failed to read {name} from {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}

fn verify_pe_machine(bytes: &[u8], expected: u16, label: &str) -> Result<()> {
    if bytes.get(..2) != Some(b"MZ") {
        bail!("{label} is not a PE file (missing MZ header)");
    }
    let offset_bytes = bytes
        .get(0x3C..0x40)
        .context("PE file is shorter than its DOS header")?;
    let pe_offset = usize::try_from(u32::from_le_bytes(
        offset_bytes
            .try_into()
            .context("invalid PE header offset bytes")?,
    ))
    .context("PE header offset does not fit usize")?;
    let signature_end = pe_offset
        .checked_add(4)
        .context("PE header offset overflow")?;
    if bytes.get(pe_offset..signature_end) != Some(b"PE\0\0") {
        bail!("{label} is not a PE file (missing PE signature)");
    }
    let machine_end = signature_end
        .checked_add(2)
        .context("PE machine offset overflow")?;
    let machine = u16::from_le_bytes(
        bytes
            .get(signature_end..machine_end)
            .context("PE file has no COFF machine field")?
            .try_into()
            .context("invalid PE machine field")?,
    );
    if machine != expected {
        bail!("{label} has PE machine {machine:#06x}; expected {expected:#06x}");
    }
    Ok(())
}

fn extract_archive(path: &Path, output: &Path) -> Result<()> {
    let status = Command::new("tar")
        .arg("-xf")
        .arg(path)
        .arg("-C")
        .arg(output)
        .status()
        .with_context(|| format!("extracting archive {}", path.display()))?;
    if !status.success() {
        bail!("tar failed to extract {}", path.display());
    }
    Ok(())
}

#[cfg(windows)]
fn verify_signatures(directory: &Path, version: &str) -> Result<()> {
    verify_authenticode(&directory.join("linerule.msixbundle"))?;
    let expected_file_version = format!("{version}.0");
    for archive in ["linerule-portable-x64.zip", "linerule-portable-arm64.zip"] {
        let unpacked = tempfile::tempdir().context("creating PE inspection directory")?;
        extract_archive(&directory.join(archive), unpacked.path())?;
        let main = unpacked.path().join("linerule.exe");
        let settings = unpacked
            .path()
            .join("settings")
            .join("linerule-settings.exe");
        verify_authenticode(&main)?;
        verify_authenticode(&settings)?;
        verify_file_version(&main, &expected_file_version)?;
        verify_file_version(&settings, &expected_file_version)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn verify_signatures(_directory: &Path, _version: &str) -> Result<()> {
    bail!("signature verification requires Windows; use --allow-unsigned only for smoke runs")
}

#[cfg(windows)]
fn verify_authenticode(path: &Path) -> Result<()> {
    let script = concat!(
        "$signature = Get-AuthenticodeSignature -LiteralPath $args[0]; ",
        "if ($signature.Status -ne 'Valid') { ",
        "Write-Error \"invalid Authenticode status: $($signature.Status)\"; exit 1 }"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .arg(path)
        .output()
        .with_context(|| format!("verifying signature {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "signature verification failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[cfg(windows)]
fn verify_file_version(path: &Path, expected: &str) -> Result<()> {
    let script = concat!(
        "$actual = (Get-Item -LiteralPath $args[0]).VersionInfo.FileVersion; ",
        "if ($actual -ne $args[1]) { ",
        "Write-Error \"file version mismatch: expected $($args[1]), found $actual\"; exit 1 }"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .arg(path)
        .arg(expected)
        .output()
        .with_context(|| format!("reading file version {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "file version verification failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn verify_checksums(directory: &Path) -> Result<()> {
    let checksum_path = directory.join("SHA256SUMS.txt");
    let content = fs::read_to_string(&checksum_path).context("reading SHA256SUMS.txt")?;
    let mut entries = BTreeMap::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let (digest, name) = line
            .split_once("  ")
            .with_context(|| format!("invalid checksum line: {line}"))?;
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("invalid SHA-256 digest for {name}: {digest}");
        }
        if entries
            .insert(name.to_owned(), digest.to_ascii_lowercase())
            .is_some()
        {
            bail!("SHA256SUMS.txt contains duplicate entry for {name}");
        }
    }

    let expected_names = REQUIRED_ASSETS
        .iter()
        .copied()
        .filter(|name| *name != "SHA256SUMS.txt")
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let actual_names = entries.keys().cloned().collect::<BTreeSet<_>>();
    if actual_names != expected_names {
        bail!(
            "SHA256SUMS.txt entries differ from release assets: \
             expected {expected_names:?}, found {actual_names:?}"
        );
    }

    for name in REQUIRED_ASSETS
        .iter()
        .copied()
        .filter(|name| *name != "SHA256SUMS.txt")
    {
        let expected = entries
            .get(name)
            .with_context(|| format!("SHA256SUMS.txt has no entry for {name}"))?;
        let actual = sha256(&directory.join(name))?;
        if &actual != expected {
            bail!("SHA-256 mismatch for {name}: expected {expected}, found {actual}");
        }
    }
    Ok(())
}

fn sha256(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("opening artifact {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn check_ruleset(root: &Path) -> Result<()> {
    let default_branch = check_ruleset_source(
        root,
        "protect-default-branch",
        "branch",
        "refs/heads/main",
        &[
            "deletion",
            "non_fast_forward",
            "required_linear_history",
            "pull_request",
            "required_status_checks",
        ],
    )?;
    let status_checks = default_branch["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|rule| rule["type"].as_str() == Some("required_status_checks"))
        .flat_map(|rule| {
            rule["parameters"]["required_status_checks"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(|check| check["context"].as_str())
        .collect::<BTreeSet<_>>();
    if !status_checks.contains("ci-required") {
        bail!("default-branch ruleset does not require ci-required");
    }
    check_ruleset_source(
        root,
        "protect-release-tags",
        "tag",
        "refs/tags/v*",
        &["deletion", "non_fast_forward", "update"],
    )?;
    check_ruleset_source(
        root,
        "require-signed-commits",
        "branch",
        "~ALL",
        &["required_signatures"],
    )?;

    let release_workflow = fs::read_to_string(root.join(".github/workflows/release-assets.yml"))
        .context("reading release-assets workflow")?;
    for required in [
        "ci-required",
        "hardware-required",
        "protect-default-branch",
        "protect-release-tags",
        "require-signed-commits",
    ] {
        if !release_workflow.contains(required) {
            bail!("release workflow does not require {required}");
        }
    }
    let ci_workflow =
        fs::read_to_string(root.join(".github/workflows/ci.yml")).context("reading CI workflow")?;
    for required in ["CycloneDX dependency SBOM", "tools/New-Sbom.ps1", "- sbom"] {
        if !ci_workflow.contains(required) {
            bail!("CI workflow does not enforce `{required}`");
        }
    }
    let ignores = fs::read_to_string(root.join(".gitignore")).context("reading .gitignore")?;
    for generated in [
        "/binaries",
        "/dist",
        "/install-test-old",
        "/nightly-dist",
        "/settings-publish",
        "/settings-publish-arm64",
        "/settings-publish-x64",
        "/sign-bundle",
        "/sign-pe",
        "/signed-bundle",
        "/signed-pe",
    ] {
        if !ignores.lines().any(|line| line.trim() == generated) {
            bail!("release-generated directory is not ignored: {generated}");
        }
    }
    Ok(())
}

fn check_package_channel_contract(root: &Path) -> Result<()> {
    let package_script =
        fs::read_to_string(root.join("packaging/package.ps1")).context("reading package.ps1")?;
    for required in [
        r#"[string]$Identity = "P4suta.linerule""#,
        r#"Replace("@IDENTITY@", $identity)"#,
        r#"Replace("@ARTIFACT_NAME@", $ArtifactName)"#,
    ] {
        if !package_script.contains(required) {
            bail!("package.ps1 does not enforce `{required}`");
        }
    }

    let appinstaller = fs::read_to_string(root.join("packaging/linerule.appinstaller.in"))
        .context("reading App Installer template")?;
    for required in ["@ARTIFACT_NAME@.appinstaller", "@ARTIFACT_NAME@.msixbundle"] {
        if !appinstaller.contains(required) {
            bail!("App Installer template does not use `{required}`");
        }
    }

    let nightly = fs::read_to_string(root.join(".github/workflows/nightly.yml"))
        .context("reading nightly workflow")?;
    for required in [
        r#"-Identity "P4suta.linerule.Nightly""#,
        r#"-ArtifactName "linerule-nightly""#,
        "--target x86_64-pc-windows-msvc",
        "--target aarch64-pc-windows-msvc",
        "-SkipAppInstaller",
    ] {
        if !nightly.contains(required) {
            bail!("Nightly workflow does not enforce `{required}`");
        }
    }
    if nightly.contains(r#"-Identity "P4suta.linerule""#) {
        bail!("Nightly workflow reuses the Stable Package Identity");
    }
    Ok(())
}

fn check_mise_lock(root: &Path) -> Result<()> {
    let config = fs::read_to_string(root.join("mise.toml")).context("reading mise.toml")?;
    if !config.contains("[settings]\nlockfile = true") {
        bail!("mise.toml does not enable the committed lockfile");
    }
    let configured = parse_mise_config_versions(&config)?;

    let lock = fs::read_to_string(root.join("mise.lock")).context("reading mise.lock")?;
    if !lock.starts_with("# @generated - this file is auto-generated by `mise lock`") {
        bail!("mise.lock is not a generated mise lockfile");
    }
    let (locked, platforms) = parse_mise_lock(&lock)?;
    if configured != locked {
        bail!("mise.toml and mise.lock tool versions differ");
    }

    let required_platforms = ["linux-x64", "windows-arm64", "windows-x64"];
    for tool in configured.keys() {
        if !platforms.keys().any(|(name, _)| name == tool) {
            if tool != "dotnet"
                && tool != "rust"
                && !tool.starts_with("cargo:")
                && !tool.starts_with("dotnet:")
                && !tool.starts_with("npm:")
                && !tool.starts_with("pipx:")
            {
                bail!("mise.lock has no asset integrity metadata for {tool}");
            }
            continue;
        }
        for platform in required_platforms {
            let Some((url, checksum)) = platforms.get(&(tool.clone(), platform.to_owned())) else {
                bail!("mise.lock has no {platform} asset for {tool}");
            };
            if !url.starts_with("https://") {
                bail!("mise.lock has a non-HTTPS asset URL for {tool} on {platform}");
            }
            let Some(digest) = checksum
                .as_deref()
                .and_then(|value| value.strip_prefix("sha256:"))
            else {
                bail!("mise.lock has no SHA-256 for {tool} on {platform}");
            };
            if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!("mise.lock has an invalid SHA-256 for {tool} on {platform}");
            }
        }
    }
    Ok(())
}

fn parse_mise_config_versions(document: &str) -> Result<BTreeMap<String, String>> {
    let mut tools = BTreeMap::new();
    let mut in_tools = false;
    for raw in document.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line == "[tools]" {
            in_tools = true;
            continue;
        }
        if line.starts_with('[') {
            in_tools = false;
        }
        if !in_tools || line.is_empty() {
            continue;
        }
        if raw.chars().next().is_some_and(char::is_whitespace) {
            continue;
        }
        let Some((raw_name, raw_value)) = line.split_once('=') else {
            continue;
        };
        let raw_name = raw_name.trim();
        let name = if let Some(quoted) = raw_name
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            quoted
        } else if raw_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            raw_name
        } else {
            continue;
        };
        let value = raw_value.trim();
        let version = value
            .strip_prefix('"')
            .map_or_else(
                || {
                    value
                        .split_once("version = \"")
                        .and_then(|(_, rest)| rest.split('"').next())
                },
                |quoted| quoted.split('"').next(),
            )
            .with_context(|| format!("mise tool {name} has no exact version"))?;
        if version.is_empty()
            || version.eq_ignore_ascii_case("latest")
            || version.eq_ignore_ascii_case("lts")
        {
            bail!("mise tool {name} is not pinned to an exact version");
        }
        tools.insert(name.to_owned(), version.to_owned());
    }
    if tools.is_empty() {
        bail!("mise.toml has no configured tools");
    }
    Ok(tools)
}

type LockedPlatform = (String, Option<String>);
type LockedVersions = BTreeMap<String, String>;
type LockedPlatforms = BTreeMap<(String, String), LockedPlatform>;

fn parse_mise_lock(document: &str) -> Result<(LockedVersions, LockedPlatforms)> {
    let mut tools = BTreeMap::new();
    let mut platforms = BTreeMap::new();
    let mut current_tool = None;
    let mut current_platform = None;
    for raw in document.lines() {
        let line = raw.trim();
        if let Some(name) = line
            .strip_prefix("[[tools.")
            .and_then(|value| value.strip_suffix("]]"))
        {
            current_tool = Some(name.trim_matches('"').to_owned());
            current_platform = None;
            continue;
        }
        if let Some(section) = line
            .strip_prefix("[tools.")
            .and_then(|value| value.strip_suffix(']'))
            && let Some((name, platform)) = section.rsplit_once(".\"platforms.")
        {
            let key = (
                name.trim_matches('"').to_owned(),
                platform.trim_end_matches('"').to_owned(),
            );
            platforms.insert(key.clone(), (String::new(), None));
            current_platform = Some(key);
            current_tool = None;
            continue;
        }
        if line.starts_with('[') {
            current_platform = None;
        }
        if let Some(name) = current_tool.as_ref()
            && let Some(version) = toml_string_value(line, "version")
        {
            tools.insert(name.clone(), version);
            current_tool = None;
            continue;
        }
        if let Some(key) = current_platform.as_ref() {
            if let Some(url) = toml_string_value(line, "url") {
                platforms.entry(key.clone()).or_default().0 = url;
            } else if let Some(checksum) = toml_string_value(line, "checksum") {
                platforms.entry(key.clone()).or_default().1 = Some(checksum);
            }
        }
    }
    if tools.is_empty() {
        bail!("mise.lock has no locked tools");
    }
    Ok((tools, platforms))
}

fn toml_string_value(line: &str, key: &str) -> Option<String> {
    line.strip_prefix(&format!("{key} = \""))
        .and_then(|value| value.strip_suffix('"'))
        .map(str::to_owned)
}

fn check_ruleset_source(
    root: &Path,
    name: &str,
    target: &str,
    included_ref: &str,
    required_rules: &[&str],
) -> Result<serde_json::Value> {
    let path = root.join(".github/rulesets").join(format!("{name}.json"));
    let ruleset: serde_json::Value = serde_json::from_reader(
        fs::File::open(&path).with_context(|| format!("opening ruleset {}", path.display()))?,
    )
    .with_context(|| format!("parsing ruleset {}", path.display()))?;
    for (field, expected) in [
        ("name", name),
        ("target", target),
        ("enforcement", "active"),
    ] {
        if ruleset[field].as_str() != Some(expected) {
            bail!("{name} ruleset field {field} is not `{expected}`");
        }
    }
    if ruleset["bypass_actors"]
        .as_array()
        .is_none_or(|actors| !actors.is_empty())
    {
        bail!("{name} ruleset permits bypass actors");
    }
    if !ruleset["conditions"]["ref_name"]["include"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|value| value.as_str() == Some(included_ref))
    {
        bail!("{name} ruleset does not include {included_ref}");
    }
    let actual_rules = ruleset["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|rule| rule["type"].as_str())
        .collect::<BTreeSet<_>>();
    for required in required_rules {
        if !actual_rules.contains(required) {
            bail!("{name} ruleset does not enforce {required}");
        }
    }
    Ok(ruleset)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use std::fs;

    use super::{
        PE_MACHINE_ARM64, PE_MACHINE_X64, REQUIRED_ASSETS, check_expected_tag, check_mise_lock,
        check_package_channel_contract, check_ruleset, sha256, verify_checksums, verify_pe_machine,
        verify_sbom, verify_source_spdx,
    };

    fn minimal_pe(machine: u16) -> Vec<u8> {
        let pe_offset = 0x80_u32;
        let pe_offset_usize = usize::try_from(pe_offset).expect("test PE offset fits usize");
        let mut bytes = vec![0_u8; pe_offset_usize + 6];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3C..0x40].copy_from_slice(&pe_offset.to_le_bytes());
        bytes[pe_offset_usize..pe_offset_usize + 4].copy_from_slice(b"PE\0\0");
        bytes[pe_offset_usize + 4..pe_offset_usize + 6].copy_from_slice(&machine.to_le_bytes());
        bytes
    }

    #[test]
    fn release_tag_must_match_the_workspace_version_exactly() {
        check_expected_tag("0.6.0", None).expect("local smoke may omit a tag");
        check_expected_tag("0.6.0", Some("v0.6.0")).expect("matching release tag");
        assert!(check_expected_tag("0.6.0", Some("0.6.0")).is_err());
        assert!(check_expected_tag("0.6.0", Some("v0.6.1")).is_err());
    }

    #[test]
    fn release_sbom_must_cover_rust_and_dotnet_dependencies() {
        let directory = tempfile::tempdir().expect("temporary SBOM directory");
        let path = directory.path().join("bom.json");
        fs::write(
            &path,
            r#"{
                "bomFormat": "CycloneDX",
                "specVersion": "1.6",
                "components": [
                    {"purl": "pkg:cargo/linerule-app@0.6.0"},
                    {"purl": "pkg:nuget/Microsoft.WindowsAppSDK@2.3.1"}
                ]
            }"#,
        )
        .expect("write valid SBOM");
        verify_sbom(&path).expect("both shipped dependency ecosystems are present");

        fs::write(
            &path,
            r#"{
                "bomFormat": "CycloneDX",
                "specVersion": "1.6",
                "components": [{"purl": "pkg:cargo/linerule-app@0.6.0"}]
            }"#,
        )
        .expect("write incomplete SBOM");
        assert!(verify_sbom(&path).is_err());
    }

    #[test]
    fn pe_machine_must_match_the_declared_release_architecture() {
        verify_pe_machine(&minimal_pe(PE_MACHINE_X64), PE_MACHINE_X64, "x64").expect("x64 PE");
        verify_pe_machine(&minimal_pe(PE_MACHINE_ARM64), PE_MACHINE_ARM64, "arm64")
            .expect("ARM64 PE");

        assert!(verify_pe_machine(&minimal_pe(PE_MACHINE_X64), PE_MACHINE_ARM64, "wrong").is_err());
        assert!(verify_pe_machine(b"not a PE", PE_MACHINE_X64, "missing MZ").is_err());
        assert!(verify_pe_machine(b"MZ", PE_MACHINE_X64, "truncated").is_err());
    }

    #[test]
    fn source_spdx_must_be_a_complete_reuse_document() {
        let directory = tempfile::tempdir().expect("temporary SPDX directory");
        let path = directory.path().join("linerule-source.spdx");
        fs::write(
            &path,
            "\
SPDXVersion: SPDX-2.1
DataLicense: CC0-1.0
SPDXID: SPDXRef-DOCUMENT
DocumentName: linerule-rs

LicenseInfoInFile: MIT
LicenseInfoInFile: Apache-2.0
LicenseInfoInFile: CC0-1.0
",
        )
        .expect("write valid source SPDX document");
        verify_source_spdx(&path).expect("complete source SPDX document");

        fs::write(
            &path,
            "\
SPDXVersion: SPDX-2.1
DataLicense: CC0-1.0
SPDXID: SPDXRef-DOCUMENT
DocumentName: linerule-rs
LicenseInfoInFile: MIT
",
        )
        .expect("write incomplete source SPDX document");
        assert!(verify_source_spdx(&path).is_err());
    }

    #[test]
    fn checksum_manifest_must_have_one_exact_entry_per_asset() {
        let directory = tempfile::tempdir().expect("temporary release directory");
        for asset in REQUIRED_ASSETS
            .iter()
            .copied()
            .filter(|name| *name != "SHA256SUMS.txt")
        {
            fs::write(directory.path().join(asset), asset).expect("write release asset");
        }
        let entries = REQUIRED_ASSETS
            .iter()
            .copied()
            .filter(|name| *name != "SHA256SUMS.txt")
            .map(|name| {
                let digest = sha256(&directory.path().join(name)).expect("hash release asset");
                format!("{digest}  {name}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let checksum_path = directory.path().join("SHA256SUMS.txt");
        fs::write(&checksum_path, format!("{entries}\n")).expect("write checksum manifest");
        verify_checksums(directory.path()).expect("exact checksum manifest");

        fs::write(
            &checksum_path,
            format!("{entries}\n{0}  linerule.msixbundle\n", "0".repeat(64)),
        )
        .expect("write duplicate checksum entry");
        assert!(verify_checksums(directory.path()).is_err());
    }

    #[test]
    fn repository_rulesets_are_structurally_release_ready() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        check_ruleset(&root).expect("repository rulesets and release gates");
    }

    #[test]
    fn stable_and_nightly_use_distinct_package_identities() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        check_package_channel_contract(&root).expect("separate Stable and Nightly identities");
    }

    #[test]
    fn mise_environment_is_version_and_asset_locked_for_all_ci_platforms() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        check_mise_lock(&root).expect("complete multi-platform mise lock");
    }
}
