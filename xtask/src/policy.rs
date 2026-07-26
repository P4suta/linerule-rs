//! Mechanical source-policy checks that do not depend on a compiler target.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const WINDOWS_FFI: &str = "crates/linerule-platform-windows/src/win32_ffi/";
const MAX_UNSAFE_SITES: usize = 81;

pub(crate) fn run() -> Result<()> {
    let root = workspace_root()?;
    let mut rust_files = Vec::new();
    collect_rust_files(&root.join("crates"), &mut rust_files)?;

    let mut violations = Vec::new();
    let mut unsafe_sites = 0;
    for path in rust_files {
        inspect_rust_file(&root, &path, &mut violations, &mut unsafe_sites)?;
    }
    if unsafe_sites > MAX_UNSAFE_SITES {
        violations.push(format!(
            "unsafe budget exceeded: {unsafe_sites} sites, maximum {MAX_UNSAFE_SITES}"
        ));
    }
    inspect_windows_api(&root, &mut violations)?;
    inspect_core_api(&root, &mut violations)?;
    inspect_settings_localization(&root, &mut violations)?;

    if violations.is_empty() {
        println!("policy: ok (unsafe sites {unsafe_sites}/{MAX_UNSAFE_SITES})");
        Ok(())
    } else {
        for violation in &violations {
            eprintln!("[policy] {violation}");
        }
        bail!("policy: {} violation(s)", violations.len())
    }
}

fn workspace_root() -> Result<PathBuf> {
    let manifest = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .context("reading workspace metadata")?;
    Ok(manifest.workspace_root.into_std_path_buf())
}

fn collect_rust_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("reading source directory {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, output)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
    Ok(())
}

fn inspect_rust_file(
    root: &Path,
    path: &Path,
    violations: &mut Vec<String>,
    unsafe_sites: &mut usize,
) -> Result<()> {
    let relative = slash_path(path.strip_prefix(root).unwrap_or(path));
    let source = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let lines = source.lines().collect::<Vec<_>>();

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if is_code_unsafe(trimmed) {
            *unsafe_sites += 1;
            if relative.starts_with(WINDOWS_FFI) {
                let start = index.saturating_sub(8);
                let has_safety = lines[start..index]
                    .iter()
                    .any(|previous| previous.contains("SAFETY:") || previous.contains("# Safety"));
                if !has_safety {
                    violations.push(format!(
                        "{relative}:{} unsafe operation lacks a nearby SAFETY comment",
                        index + 1
                    ));
                }
            } else {
                violations.push(format!(
                    "{relative}:{} unsafe code is outside {WINDOWS_FFI}",
                    index + 1
                ));
            }
        }

        for forbidden in ["Box::leak", "mem::forget", ".Release("] {
            if trimmed.contains(forbidden) && !is_comment(trimmed) {
                violations.push(format!(
                    "{relative}:{} forbidden ownership escape `{forbidden}`",
                    index + 1
                ));
            }
        }
    }

    if relative.contains("/tests/") || relative.contains("/benches/") {
        return Ok(());
    }
    let production = production_source(&source);
    for (index, line) in production.lines().enumerate() {
        let trimmed = line.trim();
        if is_comment(trimmed) {
            continue;
        }
        for forbidden in [".unwrap()", ".expect(", "panic!("] {
            if trimmed.contains(forbidden) {
                violations.push(format!(
                    "{relative}:{} production `{forbidden}` is forbidden",
                    index + 1
                ));
            }
        }
    }
    Ok(())
}

fn production_source(source: &str) -> &str {
    source
        .find("\nmod tests {")
        .map_or(source, |test_module| &source[..test_module])
}

fn inspect_windows_api(root: &Path, violations: &mut Vec<String>) -> Result<()> {
    inspect_api_facade(
        root,
        "Windows",
        "crates/linerule-platform-windows/src/lib.rs",
        "api/platform-windows.txt",
        violations,
    )
}

fn inspect_core_api(root: &Path, violations: &mut Vec<String>) -> Result<()> {
    inspect_api_facade(
        root,
        "core",
        "crates/linerule-core/src/lib.rs",
        "api/core.txt",
        violations,
    )
}

fn inspect_api_facade(
    root: &Path,
    label: &str,
    facade_path: &str,
    snapshot_path: &str,
    violations: &mut Vec<String>,
) -> Result<()> {
    let facade = fs::read_to_string(root.join(facade_path))
        .with_context(|| format!("reading {label} facade"))?;
    let actual = exported_names(&facade);
    let expected = fs::read_to_string(root.join(snapshot_path))
        .with_context(|| format!("reading {label} API snapshot"))?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();

    if actual != expected {
        violations.push(format!(
            "{label} public API drift: expected {expected:?}, found {actual:?}"
        ));
    }
    if facade
        .lines()
        .any(|line| line.trim().starts_with("pub mod "))
    {
        violations.push(format!(
            "{label} facade must not expose implementation modules"
        ));
    }
    Ok(())
}

fn inspect_settings_localization(root: &Path, violations: &mut Vec<String>) -> Result<()> {
    let strings_root = root.join("ui/linerule-settings/Strings");
    let english_path = strings_root.join("en-US/Resources.resw");
    let japanese_path = strings_root.join("ja-JP/Resources.resw");
    let english_source =
        fs::read_to_string(&english_path).context("reading English WinUI resources")?;
    let japanese_source =
        fs::read_to_string(&japanese_path).context("reading Japanese WinUI resources")?;
    let english = resource_keys(&english_source);
    let japanese = resource_keys(&japanese_source);

    if english != japanese {
        violations.push(format!(
            "WinUI localization key drift: en-US only {:?}; ja-JP only {:?}",
            english.difference(&japanese).collect::<Vec<_>>(),
            japanese.difference(&english).collect::<Vec<_>>()
        ));
    }

    let mut csharp_files = Vec::new();
    collect_files_with_extension(&root.join("ui/linerule-settings"), "cs", &mut csharp_files)?;
    let mut referenced = BTreeSet::new();
    for path in csharp_files {
        let source =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        referenced.extend(localized_string_references(&source));
    }
    let missing = referenced.difference(&english).collect::<Vec<_>>();
    if !missing.is_empty() {
        violations.push(format!(
            "WinUI code references missing en-US resources: {missing:?}"
        ));
    }
    Ok(())
}

fn collect_files_with_extension(
    directory: &Path,
    extension: &str,
    output: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("reading source directory {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "obj") {
                continue;
            }
            collect_files_with_extension(&path, extension, output)?;
        } else if path
            .extension()
            .is_some_and(|candidate| candidate == extension)
        {
            output.push(path);
        }
    }
    Ok(())
}

fn resource_keys(source: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter_map(|line| line.split_once("<data name=\"").map(|(_, suffix)| suffix))
        .filter_map(|suffix| suffix.split_once('"').map(|(name, _)| name.to_owned()))
        .collect()
}

fn localized_string_references(source: &str) -> BTreeSet<String> {
    ["Strings.Get(\"", "Strings.Format(\""]
        .into_iter()
        .flat_map(|prefix| {
            source.match_indices(prefix).filter_map(move |(index, _)| {
                let suffix = &source[index + prefix.len()..];
                suffix.split_once('"').map(|(name, _)| name.to_owned())
            })
        })
        .collect()
}

fn exported_names(source: &str) -> BTreeSet<String> {
    let statements = source
        .lines()
        .filter(|line| !is_comment(line.trim()))
        .collect::<Vec<_>>()
        .join(" ");
    statements
        .split(';')
        .flat_map(|statement| {
            let statement = statement.trim();
            if let Some(export) = statement.strip_prefix("pub use ") {
                if let Some((_, names)) = export.split_once("::{") {
                    return names
                        .trim_end_matches('}')
                        .split(',')
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(exported_name)
                        .collect::<Vec<_>>();
                }
                return export
                    .rsplit("::")
                    .next()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(exported_name)
                    .into_iter()
                    .collect();
            }
            statement
                .strip_prefix("pub type ")
                .and_then(|declaration| {
                    declaration
                        .split(['<', '=', ' '])
                        .next()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                })
                .map(ToOwned::to_owned)
                .into_iter()
                .collect()
        })
        .collect()
}

fn exported_name(declaration: &str) -> String {
    declaration
        .split_once(" as ")
        .map_or(declaration, |(_, alias)| alias)
        .to_owned()
}

fn is_code_unsafe(line: &str) -> bool {
    !is_comment(line)
        && !line.starts_with("reason =")
        && (line.contains("unsafe {")
            || line.contains("unsafe fn ")
            || line.contains("unsafe extern "))
}

fn is_comment(line: &str) -> bool {
    line.starts_with("//")
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_braced_and_single_exports() {
        let exports = exported_names(
            "pub use desktop::{DesktopRuntime, RuntimeOptions};\n\
             pub use error::PlatformError;\n\
             pub use parser::parse as parse_chord;\n\
             pub type Result<T, E = PlatformError> = core::result::Result<T, E>;",
        );
        assert_eq!(
            exports,
            BTreeSet::from([
                "DesktopRuntime".to_owned(),
                "PlatformError".to_owned(),
                "Result".to_owned(),
                "RuntimeOptions".to_owned(),
                "parse_chord".to_owned()
            ])
        );
    }

    #[test]
    fn unsafe_detection_ignores_policy_text() {
        assert!(is_code_unsafe("let value = unsafe { call() };"));
        assert!(!is_code_unsafe(
            "reason = \"Windows APIs are unsafe fn declarations\""
        ));
        assert!(!is_code_unsafe("// unsafe { example }"));
    }

    #[test]
    fn production_scan_ignores_only_the_unit_test_module() {
        let source = r#"
struct Runtime {
    #[cfg(test)]
    fixture: bool,
}

fn production() {
    operation().expect("must be rejected");
}

#[cfg(test)]
mod tests {
    fn fixture() {
        operation().expect("allowed in tests");
    }
}
"#;
        let production = production_source(source);
        assert!(production.contains("must be rejected"));
        assert!(!production.contains("allowed in tests"));
    }

    #[test]
    fn localization_parsers_extract_resource_and_code_keys() {
        let resources = resource_keys(
            r#"<data name="SaveButton.Content"><value>Save</value></data>
<data name="DialogTitle"><value>Title</value></data>"#,
        );
        assert_eq!(
            resources,
            BTreeSet::from(["DialogTitle".to_owned(), "SaveButton.Content".to_owned()])
        );

        let references = localized_string_references(
            r#"Strings.Get("DialogTitle"); Strings.Format("FailureFormat", error);"#,
        );
        assert_eq!(
            references,
            BTreeSet::from(["DialogTitle".to_owned(), "FailureFormat".to_owned()])
        );
    }
}
