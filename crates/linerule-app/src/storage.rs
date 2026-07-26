//! Data-directory selection, preferences persistence, and retention policy.

#![forbid(unsafe_code)]

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use linerule_core::{PREFERENCES_SCHEMA_VERSION, Preferences};
use tempfile::NamedTempFile;
use thiserror::Error;

const PACKAGE_IDENTITY: &str = "P4suta.linerule";

/// Selected distribution layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Distribution {
    /// MSIX/local installation, storing data under package `LocalState`.
    Installed,
    /// ZIP distribution with a `linerule.portable` marker beside the EXE.
    Portable,
}

/// All writable application paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataPaths {
    pub(crate) distribution: Distribution,
    pub(crate) root: PathBuf,
    pub(crate) preferences: PathBuf,
    pub(crate) logs: PathBuf,
    pub(crate) crashes: PathBuf,
}

impl DataPaths {
    /// Resolve paths for the running executable.
    pub(crate) fn discover() -> Result<Self, StorageError> {
        let executable = std::env::current_exe().map_err(StorageError::CurrentExecutable)?;
        let executable_dir = executable
            .parent()
            .ok_or_else(|| StorageError::ExecutableWithoutParent(executable.clone()))?;
        if executable_dir.join("linerule.portable").is_file() {
            return Self::from_executable(&executable, None);
        }

        let installed_root = installed_data_root();
        Self::from_executable(&executable, installed_root.as_deref())
    }

    /// Pure path selection used by tests and the runtime boundary.
    pub(crate) fn from_executable(
        executable: &Path,
        installed_root: Option<&Path>,
    ) -> Result<Self, StorageError> {
        let executable_dir = executable
            .parent()
            .ok_or_else(|| StorageError::ExecutableWithoutParent(executable.to_path_buf()))?;
        let marker = executable_dir.join("linerule.portable");
        let (distribution, root) = if marker.is_file() {
            (Distribution::Portable, executable_dir.join("data"))
        } else {
            let root = installed_root.ok_or(StorageError::LocalAppDataUnavailable)?;
            (Distribution::Installed, root.to_path_buf())
        };
        Ok(Self {
            distribution,
            preferences: root.join("settings.json"),
            logs: root.join("logs"),
            crashes: root.join("crashes"),
            root,
        })
    }

    pub(crate) fn ensure_directories(&self) -> Result<(), StorageError> {
        for directory in [&self.root, &self.logs, &self.crashes] {
            fs::create_dir_all(directory).map_err(|source| StorageError::CreateDirectory {
                path: directory.clone(),
                source,
            })?;
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn installed_data_root() -> Option<PathBuf> {
    use windows::Storage::ApplicationData;

    match ApplicationData::Current()
        .and_then(|application_data| application_data.LocalFolder())
        .and_then(|folder| folder.Path())
    {
        Ok(path) => Some(PathBuf::from(path.to_string_lossy())),
        Err(error) => {
            // An unpackaged developer build has no package identity, so WinRT
            // ApplicationData is unavailable. Keep that case isolated from
            // the MSIX LocalState contract while retaining a stable dev root.
            tracing::debug!(
                %error,
                "package LocalState unavailable; using unpackaged data root"
            );
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|root| root.join(PACKAGE_IDENTITY))
        },
    }
}

#[cfg(not(target_os = "windows"))]
fn installed_data_root() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join(PACKAGE_IDENTITY))
}

/// Result of loading preferences.
#[derive(Debug)]
pub(crate) enum LoadOutcome {
    /// A valid schema-v1 document.
    Loaded(Preferences),
    /// No settings exist yet.
    Defaults,
    /// Invalid data was moved aside and defaults are active.
    Recovered {
        preferences: Preferences,
        quarantined: PathBuf,
    },
    /// A newer binary owns this file. It remains untouched and this process
    /// must not write preferences.
    FutureVersion {
        preferences: Preferences,
        found: u32,
    },
}

impl LoadOutcome {
    pub(crate) fn preferences(&self) -> &Preferences {
        match self {
            Self::Loaded(preferences)
            | Self::Recovered { preferences, .. }
            | Self::FutureVersion { preferences, .. } => preferences,
            Self::Defaults => default_preferences(),
        }
    }

    pub(crate) const fn writable(&self) -> bool {
        !matches!(self, Self::FutureVersion { .. })
    }
}

fn default_preferences() -> &'static Preferences {
    static DEFAULTS: std::sync::OnceLock<Preferences> = std::sync::OnceLock::new();
    DEFAULTS.get_or_init(Preferences::default)
}

/// Atomic schema-v1 preferences store.
#[derive(Debug, Clone)]
pub(crate) struct PreferencesStore {
    path: PathBuf,
}

impl PreferencesStore {
    pub(crate) const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn load(&self) -> Result<LoadOutcome, StorageError> {
        let raw = match fs::read(&self.path) {
            Ok(raw) => raw,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LoadOutcome::Defaults);
            },
            Err(source) => {
                return Err(StorageError::ReadPreferences {
                    path: self.path.clone(),
                    source,
                });
            },
        };

        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&raw)
            && let Some(found) = value
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
            && found > u64::from(PREFERENCES_SCHEMA_VERSION)
        {
            return Ok(LoadOutcome::FutureVersion {
                preferences: Preferences::default(),
                found: u32::try_from(found).unwrap_or(u32::MAX),
            });
        }

        match serde_json::from_slice::<Preferences>(&raw) {
            Ok(preferences) if preferences.validate().is_ok() => {
                Ok(LoadOutcome::Loaded(preferences))
            },
            Ok(_) | Err(_) => {
                let quarantined = self.quarantine_path();
                fs::rename(&self.path, &quarantined).map_err(|source| {
                    StorageError::QuarantinePreferences {
                        from: self.path.clone(),
                        to: quarantined.clone(),
                        source,
                    }
                })?;
                Ok(LoadOutcome::Recovered {
                    preferences: Preferences::default(),
                    quarantined,
                })
            },
        }
    }

    /// Write to a temporary file in the destination directory, flush it, then
    /// atomically replace the live document.
    pub(crate) fn save(&self, preferences: &Preferences) -> Result<(), StorageError> {
        preferences
            .validate()
            .map_err(StorageError::InvalidPreferences)?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| StorageError::PreferencesWithoutParent(self.path.clone()))?;
        fs::create_dir_all(parent).map_err(|source| StorageError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;

        let mut temporary =
            NamedTempFile::new_in(parent).map_err(|source| StorageError::CreateTemporary {
                directory: parent.to_path_buf(),
                source,
            })?;
        {
            let mut writer = BufWriter::new(temporary.as_file_mut());
            serde_json::to_writer_pretty(&mut writer, preferences)
                .map_err(StorageError::SerializePreferences)?;
            writer
                .write_all(b"\n")
                .map_err(StorageError::WritePreferences)?;
            writer.flush().map_err(StorageError::WritePreferences)?;
        }
        temporary
            .as_file()
            .sync_all()
            .map_err(StorageError::WritePreferences)?;
        let persisted =
            temporary
                .persist(&self.path)
                .map_err(|error| StorageError::PersistPreferences {
                    path: self.path.clone(),
                    source: error.error,
                })?;
        persisted.sync_all().map_err(StorageError::WritePreferences)
    }

    fn quarantine_path(&self) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());
        let name = format!("settings.corrupt-{timestamp}.json");
        self.path.with_file_name(name)
    }
}

/// Delete old matching files, then keep only the newest `max_count`.
pub(crate) fn prune_files(
    directory: &Path,
    prefix: &str,
    max_age: Option<Duration>,
    max_count: usize,
) -> Result<(), StorageError> {
    if !directory.exists() {
        return Ok(());
    }
    let now = SystemTime::now();
    let mut files = Vec::new();
    for item in fs::read_dir(directory).map_err(|source| StorageError::ReadDirectory {
        path: directory.to_path_buf(),
        source,
    })? {
        let item = item.map_err(|source| StorageError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        if !item.file_name().to_string_lossy().starts_with(prefix) {
            continue;
        }
        let path = item.path();
        let modified = item
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|source| StorageError::ReadMetadata {
                path: path.clone(),
                source,
            })?;
        if max_age.is_some_and(|age| {
            now.duration_since(modified)
                .is_ok_and(|elapsed| elapsed > age)
        }) {
            fs::remove_file(&path)
                .map_err(|source| StorageError::RemoveExpired { path, source })?;
        } else {
            files.push((modified, path));
        }
    }
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in files.into_iter().skip(max_count) {
        fs::remove_file(&path).map_err(|source| StorageError::RemoveExpired { path, source })?;
    }
    Ok(())
}

/// Typed persistence and path-selection failures.
#[derive(Debug, Error)]
pub(crate) enum StorageError {
    #[error("cannot resolve the running executable: {0}")]
    CurrentExecutable(std::io::Error),
    #[error("executable path has no parent: {0}")]
    ExecutableWithoutParent(PathBuf),
    #[error("LOCALAPPDATA is unavailable for the installed distribution")]
    LocalAppDataUnavailable,
    #[error("preferences path has no parent: {0}")]
    PreferencesWithoutParent(PathBuf),
    #[error("cannot create data directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot read preferences {path}: {source}")]
    ReadPreferences {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot quarantine preferences from {from} to {to}: {source}")]
    QuarantinePreferences {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid preferences: {0}")]
    InvalidPreferences(linerule_core::PreferencesError),
    #[error("cannot create a temporary preferences file in {directory}: {source}")]
    CreateTemporary {
        directory: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot serialize preferences: {0}")]
    SerializePreferences(serde_json::Error),
    #[error("cannot write preferences: {0}")]
    WritePreferences(std::io::Error),
    #[error("cannot atomically replace preferences {path}: {source}")]
    PersistPreferences {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot read data directory {path}: {source}")]
    ReadDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot read metadata for {path}: {source}")]
    ReadMetadata {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot remove expired file {path}: {source}")]
    RemoveExpired {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    fn paths(root: &Path) -> DataPaths {
        DataPaths {
            distribution: Distribution::Portable,
            root: root.to_path_buf(),
            preferences: root.join("settings.json"),
            logs: root.join("logs"),
            crashes: root.join("crashes"),
        }
    }

    #[test]
    fn portable_marker_keeps_data_beside_distribution() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("linerule.portable"), []).expect("marker");
        let executable = temp.path().join("linerule.exe");
        let selected = DataPaths::from_executable(&executable, Some(Path::new("ignored")))
            .expect("portable paths");
        assert_eq!(selected.distribution, Distribution::Portable);
        assert_eq!(selected.root, temp.path().join("data"));
    }

    #[test]
    fn installed_layout_uses_local_state() {
        let local_state =
            Path::new("C:/Users/test/AppData/Local/Packages/P4suta.linerule_hash/LocalState");
        let selected = DataPaths::from_executable(
            Path::new("C:/Program Files/linerule/linerule.exe"),
            Some(local_state),
        )
        .expect("installed paths");
        assert_eq!(selected.distribution, Distribution::Installed);
        assert_eq!(selected.root, local_state);
    }

    #[test]
    fn data_path_discovery_and_directory_creation_are_explicit() {
        let discovered = DataPaths::discover().expect("discover current executable layout");
        assert!(!discovered.root.as_os_str().is_empty());

        let temp = tempfile::tempdir().expect("tempdir");
        let data = paths(&temp.path().join("nested"));
        data.ensure_directories().expect("create data directories");
        assert!(data.root.is_dir());
        assert!(data.logs.is_dir());
        assert!(data.crashes.is_dir());
    }

    #[test]
    fn directory_creation_failure_is_typed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("not-a-directory");
        fs::write(&root, []).expect("blocking file");
        let error = paths(&root)
            .ensure_directories()
            .expect_err("file cannot become a directory");
        assert!(matches!(error, StorageError::CreateDirectory { .. }));
    }

    #[test]
    fn missing_preferences_expose_stable_writable_defaults() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = PreferencesStore::new(temp.path().join("settings.json"));
        let first = store.load().expect("missing settings");
        let second = store.load().expect("missing settings");
        assert!(matches!(first, LoadOutcome::Defaults));
        assert!(first.writable());
        assert_eq!(first.preferences(), &Preferences::default());
        assert!(std::ptr::eq(first.preferences(), second.preferences()));
    }

    #[test]
    fn invalid_preferences_are_rejected_before_writing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut preferences = Preferences::default();
        preferences
            .hotkeys
            .set(linerule_core::Command::Quit, "Ctrl+Alt+H");
        let error = PreferencesStore::new(temp.path().join("settings.json"))
            .save(&preferences)
            .expect_err("invalid preferences");
        assert!(matches!(error, StorageError::InvalidPreferences(_)));
    }

    #[test]
    fn parentless_preferences_path_is_typed() {
        let error = PreferencesStore::new(PathBuf::new())
            .save(&Preferences::default())
            .expect_err("empty path has no parent");
        assert!(matches!(error, StorageError::PreferencesWithoutParent(_)));
    }

    #[test]
    fn preferences_round_trip_through_atomic_save() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data = paths(temp.path());
        let store = PreferencesStore::new(data.preferences);
        let mut preferences = Preferences::default();
        preferences.ruler.last_active = linerule_core::ActiveMode::Vertical;
        store.save(&preferences).expect("save");
        let LoadOutcome::Loaded(loaded) = store.load().expect("load") else {
            panic!("expected loaded preferences");
        };
        assert_eq!(loaded, preferences);
    }

    #[test]
    fn atomic_replace_failure_is_typed_and_preserves_the_existing_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let destination = temp.path().join("settings.json");
        fs::create_dir(&destination).expect("blocking destination directory");

        let error = PreferencesStore::new(destination.clone())
            .save(&Preferences::default())
            .expect_err("a directory cannot be atomically replaced by the settings file");

        assert!(matches!(error, StorageError::PersistPreferences { .. }));
        assert!(destination.is_dir());
    }

    #[test]
    fn corrupt_preferences_are_quarantined() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data = paths(temp.path());
        fs::write(&data.preferences, b"{not-json").expect("corrupt file");
        let store = PreferencesStore::new(data.preferences.clone());
        let LoadOutcome::Recovered { quarantined, .. } = store.load().expect("recover") else {
            panic!("expected recovery");
        };
        assert!(!data.preferences.exists());
        assert!(quarantined.exists());
    }

    #[test]
    fn future_schema_is_preserved_byte_for_byte() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data = paths(temp.path());
        let raw = br#"{"schema_version":99,"ruler":{},"hotkeys":{}}"#;
        fs::write(&data.preferences, raw).expect("future file");
        let store = PreferencesStore::new(data.preferences.clone());
        let outcome = store.load().expect("future detected");
        assert!(matches!(
            outcome,
            LoadOutcome::FutureVersion { found: 99, .. }
        ));
        assert!(!outcome.writable());
        assert_eq!(fs::read(data.preferences).expect("preserved"), raw);
    }

    #[test]
    fn retention_keeps_only_newest_count() {
        let temp = tempfile::tempdir().expect("tempdir");
        for index in 0..8 {
            fs::write(temp.path().join(format!("crash-{index}.json")), []).expect("crash");
        }
        fs::write(temp.path().join("keep.txt"), []).expect("unrelated");
        prune_files(temp.path(), "crash-", None, 5).expect("prune");
        let crash_count = fs::read_dir(temp.path())
            .expect("list")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("crash-"))
            .count();
        assert_eq!(crash_count, 5);
        assert!(temp.path().join("keep.txt").exists());
    }

    #[test]
    fn invalid_but_deserializable_preferences_are_quarantined() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data = paths(temp.path());
        let mut preferences = Preferences::default();
        preferences
            .hotkeys
            .set(linerule_core::Command::Quit, "Ctrl+Alt+H");
        fs::write(
            &data.preferences,
            serde_json::to_vec(&preferences).expect("serialize invalid preferences"),
        )
        .expect("invalid settings fixture");

        let outcome = PreferencesStore::new(data.preferences)
            .load()
            .expect("recover invalid preferences");
        assert!(matches!(outcome, LoadOutcome::Recovered { .. }));
    }

    #[test]
    fn json_without_schema_is_quarantined() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data = paths(temp.path());
        fs::write(&data.preferences, b"{}").expect("schema-less fixture");

        let outcome = PreferencesStore::new(data.preferences)
            .load()
            .expect("recover schema-less preferences");
        assert!(matches!(outcome, LoadOutcome::Recovered { .. }));
    }

    #[test]
    fn non_file_preferences_path_returns_typed_read_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data = paths(temp.path());
        fs::create_dir(&data.preferences).expect("directory at preferences path");

        let error = PreferencesStore::new(data.preferences)
            .load()
            .expect_err("reading a directory as preferences must fail");
        assert!(matches!(error, StorageError::ReadPreferences { .. }));
    }

    #[test]
    fn retention_handles_absent_directories_and_expires_old_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let absent = temp.path().join("absent");
        prune_files(&absent, "events.jsonl", Some(Duration::ZERO), 7).expect("absent directory");

        let expired = temp.path().join("events.jsonl.old");
        fs::write(&expired, []).expect("expired fixture");
        std::thread::sleep(Duration::from_millis(2));
        prune_files(temp.path(), "events.jsonl", Some(Duration::ZERO), 7).expect("remove expired");
        assert!(!expired.exists());
    }
}
