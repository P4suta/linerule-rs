//! Safe owner for the separate Fluent shortcut-settings process.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use linerule_core::{Command, HotkeyBindings};
use serde::{Deserialize, Serialize};

use crate::error::{PlatformError, Result};
use crate::win32_ffi::shell::ShellCommand;

const SETTINGS_DIRECTORY: &str = "settings";
const SETTINGS_EXECUTABLE: &str = "linerule-settings.exe";
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// At most one settings session owned by the resident controller.
pub(crate) struct SettingsHost {
    executable: std::result::Result<PathBuf, String>,
    session: Arc<SessionState>,
}

struct SessionState {
    active: AtomicBool,
    cancel: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Serialize)]
struct SettingsRequest<'a> {
    hotkeys: &'a HotkeyBindings,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    highlight: Option<Command>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsResponse {
    hotkeys: HotkeyBindings,
}

impl SettingsHost {
    /// Resolve the packaged `settings/` sidecar, with a sibling fallback for
    /// unpackaged developer output. Missing files are reported only when the
    /// user opens settings so the tray controller stays available.
    pub(crate) fn discover() -> Self {
        Self {
            executable: discover_executable(),
            session: Arc::new(SessionState {
                active: AtomicBool::new(false),
                cancel: AtomicBool::new(false),
                worker: Mutex::new(None),
            }),
        }
    }

    /// Start a settings session unless one is already active.
    pub(crate) fn open(
        &self,
        sender: Sender<ShellCommand>,
        hotkeys: &HotkeyBindings,
        registration_error: Option<&PlatformError>,
    ) -> Result<()> {
        let executable =
            self.executable
                .as_ref()
                .map_err(|message| PlatformError::SettingsHost {
                    message: message.clone(),
                })?;
        if !executable.is_file() {
            return Err(PlatformError::SettingsHost {
                message: format!("{} was not found", executable.display()),
            });
        }
        if self
            .session
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }
        self.session.cancel.store(false, Ordering::Release);
        if let Err(error) = self.join_previous_worker() {
            self.session.active.store(false, Ordering::Release);
            return Err(error);
        }

        let directory = tempfile::Builder::new()
            .prefix("linerule-settings-")
            .tempdir()
            .map_err(|error| self.release_after_error("create protocol directory", error))?;
        let request_path = directory.path().join("request.json");
        let response_path = directory.path().join("response.json");
        let highlight = registration_error.and_then(registration_error_command);
        let error_message = registration_error.map(ToString::to_string);
        let request = SettingsRequest {
            hotkeys,
            error: error_message.as_deref(),
            highlight,
        };
        write_request(&request_path, &request)
            .map_err(|error| self.release_after_error("write settings request", error))?;

        let executable = executable.clone();
        let session = Arc::clone(&self.session);
        let worker = thread::Builder::new()
            .name("linerule-settings-host".to_owned())
            .spawn(move || {
                let outcome =
                    run_session(&executable, &request_path, &response_path, &session.cancel);
                session.active.store(false, Ordering::Release);
                match outcome {
                    Ok(Some(hotkeys)) => send(&sender, ShellCommand::ApplyBindings(hotkeys)),
                    Ok(None) => {},
                    Err(message) => send(&sender, ShellCommand::SettingsFailed(message)),
                }
                drop(directory);
            });
        let worker = worker.map_err(|error| {
            self.session.active.store(false, Ordering::Release);
            PlatformError::SettingsHost {
                message: format!("start settings monitor thread: {error}"),
            }
        })?;
        match self.session.worker.lock() {
            Ok(mut slot) => *slot = Some(worker),
            Err(error) => {
                self.session.cancel.store(true, Ordering::Release);
                self.session.active.store(false, Ordering::Release);
                let join_failed = worker.join().is_err();
                return Err(PlatformError::SettingsHost {
                    message: format!(
                        "lock settings monitor owner: {error}; worker join failed: {join_failed}"
                    ),
                });
            },
        }
        Ok(())
    }

    fn join_previous_worker(&self) -> Result<()> {
        let previous = self
            .session
            .worker
            .lock()
            .map_err(|error| PlatformError::SettingsHost {
                message: format!("lock previous settings monitor: {error}"),
            })?
            .take();
        if let Some(worker) = previous {
            worker.join().map_err(|_| PlatformError::SettingsHost {
                message: "previous settings monitor panicked".to_owned(),
            })?;
        }
        Ok(())
    }

    fn release_after_error(&self, operation: &str, error: impl std::fmt::Display) -> PlatformError {
        self.session.active.store(false, Ordering::Release);
        PlatformError::SettingsHost {
            message: format!("{operation}: {error}"),
        }
    }
}

impl Drop for SettingsHost {
    fn drop(&mut self) {
        self.session.cancel.store(true, Ordering::Release);
        let worker = match self.session.worker.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => {
                tracing::warn!("settings monitor owner lock was poisoned during shutdown");
                poisoned.into_inner().take()
            },
        };
        if let Some(worker) = worker
            && worker.join().is_err()
        {
            tracing::warn!("settings monitor panicked during shutdown");
        }
    }
}

fn discover_executable() -> std::result::Result<PathBuf, String> {
    let current =
        std::env::current_exe().map_err(|error| format!("resolve linerule executable: {error}"))?;
    let parent = current
        .parent()
        .ok_or_else(|| "linerule executable has no parent directory".to_owned())?;
    let packaged = parent.join(SETTINGS_DIRECTORY).join(SETTINGS_EXECUTABLE);
    let sibling = parent.join(SETTINGS_EXECUTABLE);
    if packaged.is_file() || !sibling.is_file() {
        Ok(packaged)
    } else {
        Ok(sibling)
    }
}

fn write_request(path: &Path, request: &SettingsRequest<'_>) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(request).map_err(std::io::Error::other)?;
    fs::write(path, json)
}

fn run_session(
    executable: &Path,
    request_path: &Path,
    response_path: &Path,
    cancel: &AtomicBool,
) -> std::result::Result<Option<HotkeyBindings>, String> {
    let mut child = ProcessCommand::new(executable)
        .arg("--request")
        .arg(request_path)
        .arg("--response")
        .arg(response_path)
        .spawn()
        .map_err(|error| format!("launch {}: {error}", executable.display()))?;

    let status = loop {
        if cancel.load(Ordering::Acquire) {
            terminate_child(&mut child, "cancel shortcut settings")?;
            return Ok(None);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(PROCESS_POLL_INTERVAL),
            Err(error) => return Err(format!("wait for shortcut settings: {error}")),
        }
    };
    if !status.success() {
        return Err(format!(
            "shortcut settings exited with {}",
            status
                .code()
                .map_or_else(|| "no status code".to_owned(), |code| code.to_string())
        ));
    }
    read_response(response_path)
}

fn read_response(response_path: &Path) -> std::result::Result<Option<HotkeyBindings>, String> {
    if !response_path.is_file() {
        return Ok(None);
    }

    let json =
        fs::read(response_path).map_err(|error| format!("read settings response: {error}"))?;
    let response: SettingsResponse = serde_json::from_slice(&json)
        .map_err(|error| format!("decode settings response: {error}"))?;
    response
        .hotkeys
        .validate()
        .map_err(|error| format!("validate settings response: {error}"))?;
    Ok(Some(response.hotkeys))
}

fn terminate_child(child: &mut Child, operation: &str) -> std::result::Result<(), String> {
    if let Err(error) = child.kill() {
        tracing::debug!(%error, %operation, "settings process ended before termination");
    }
    child
        .wait()
        .map_err(|error| format!("{operation}: wait after termination: {error}"))?;
    Ok(())
}

fn registration_error_command(error: &PlatformError) -> Option<Command> {
    match error {
        PlatformError::HotkeyRegistration { command, .. } => Some(*command),
        _ => None,
    }
}

fn send(sender: &Sender<ShellCommand>, command: ShellCommand) {
    if sender.send(command).is_err() {
        tracing::warn!("desktop runtime command receiver is closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_the_stable_command_keys() {
        let hotkeys = HotkeyBindings::default();
        let error = PlatformError::HotkeyRegistration {
            command: Command::ToggleOnOff,
            source: Box::new(PlatformError::AlreadyRunning),
        };
        let request = SettingsRequest {
            hotkeys: &hotkeys,
            error: Some("occupied"),
            highlight: registration_error_command(&error),
        };
        let json = serde_json::to_value(request).expect("request serializes");
        assert_eq!(json["highlight"], "toggle_on_off");
        assert_eq!(json["error"], "occupied");
        assert_eq!(json["hotkeys"]["toggle_guide"], "Ctrl+Alt+K");
    }

    #[test]
    fn response_rejects_unknown_protocol_fields() {
        let json = r#"{"hotkeys":{},"future":true}"#;
        assert!(serde_json::from_str::<SettingsResponse>(json).is_err());
    }

    fn test_host(executable: std::result::Result<PathBuf, String>) -> SettingsHost {
        SettingsHost {
            executable,
            session: Arc::new(SessionState {
                active: AtomicBool::new(false),
                cancel: AtomicBool::new(false),
                worker: Mutex::new(None),
            }),
        }
    }

    #[test]
    fn write_request_persists_the_protocol_document() {
        let directory = tempfile::tempdir().expect("protocol directory");
        let path = directory.path().join("request.json");
        let hotkeys = HotkeyBindings::default();
        write_request(
            &path,
            &SettingsRequest {
                hotkeys: &hotkeys,
                error: None,
                highlight: None,
            },
        )
        .expect("write request");
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).expect("read request")).expect("decode request");
        assert_eq!(value["hotkeys"]["quit"], "Ctrl+Alt+Q");
        assert!(value.get("error").is_none());
        assert!(value.get("highlight").is_none());
    }

    #[test]
    fn read_response_handles_missing_valid_invalid_and_conflicting_documents() {
        let directory = tempfile::tempdir().expect("protocol directory");
        let path = directory.path().join("response.json");
        assert_eq!(read_response(&path).expect("missing response"), None);

        let defaults = HotkeyBindings::default();
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({ "hotkeys": defaults }))
                .expect("serialize response"),
        )
        .expect("write valid response");
        assert_eq!(
            read_response(&path).expect("valid response"),
            Some(HotkeyBindings::default())
        );

        fs::write(&path, b"{not-json").expect("write malformed response");
        assert!(
            read_response(&path)
                .expect_err("malformed response must fail")
                .contains("decode settings response")
        );

        let mut duplicate = HotkeyBindings::default();
        duplicate.set(Command::CycleEffect, "Ctrl+Alt+R");
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({ "hotkeys": duplicate }))
                .expect("serialize conflicting response"),
        )
        .expect("write conflicting response");
        assert!(
            read_response(&path)
                .expect_err("conflicting response must fail")
                .contains("validate settings response")
        );
    }

    #[test]
    fn open_reports_discovery_and_missing_file_errors_without_starting_a_session() {
        let (sender, _receiver) = std::sync::mpsc::channel();
        let discovery = test_host(Err("discovery failed".to_owned()));
        assert!(
            discovery
                .open(sender.clone(), &HotkeyBindings::default(), None)
                .expect_err("discovery error")
                .to_string()
                .contains("discovery failed")
        );

        let directory = tempfile::tempdir().expect("settings directory");
        let missing = directory.path().join(SETTINGS_EXECUTABLE);
        let host = test_host(Ok(missing));
        assert!(
            host.open(sender, &HotkeyBindings::default(), None)
                .expect_err("missing sidecar")
                .to_string()
                .contains("was not found")
        );
        assert!(!host.session.active.load(Ordering::Acquire));
    }

    #[test]
    fn already_active_open_is_an_idempotent_noop() {
        let directory = tempfile::tempdir().expect("settings directory");
        let executable = directory.path().join(SETTINGS_EXECUTABLE);
        fs::write(&executable, []).expect("sidecar fixture");
        let host = test_host(Ok(executable));
        host.session.active.store(true, Ordering::Release);
        let (sender, receiver) = std::sync::mpsc::channel();
        host.open(sender, &HotkeyBindings::default(), None)
            .expect("active session is idempotent");
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn previous_worker_is_joined_and_panics_become_typed_errors() {
        let joined = test_host(Err("unused".to_owned()));
        *joined.session.worker.lock().expect("worker slot") = Some(thread::spawn(|| {}));
        joined.join_previous_worker().expect("join worker");
        assert!(joined.session.worker.lock().expect("worker slot").is_none());

        let panicked = test_host(Err("unused".to_owned()));
        *panicked.session.worker.lock().expect("worker slot") =
            Some(thread::spawn(|| panic!("worker fixture")));
        assert!(
            panicked
                .join_previous_worker()
                .expect_err("panicked worker")
                .to_string()
                .contains("previous settings monitor panicked")
        );
    }

    #[test]
    fn open_releases_active_after_a_previous_worker_panic() {
        let directory = tempfile::tempdir().expect("settings directory");
        let executable = command_fixture(directory.path(), "unused.cmd", "exit /b 0");
        let host = test_host(Ok(executable));
        *host.session.worker.lock().expect("worker slot") =
            Some(thread::spawn(|| panic!("previous worker fixture")));
        let (sender, _receiver) = std::sync::mpsc::channel();
        assert!(
            host.open(sender, &HotkeyBindings::default(), None)
                .expect_err("previous worker panic must abort open")
                .to_string()
                .contains("previous settings monitor panicked")
        );
        assert!(!host.session.active.load(Ordering::Acquire));
    }

    #[test]
    fn poisoned_worker_owner_is_typed_and_drop_recovers() {
        let host = test_host(Err("unused".to_owned()));
        let session = Arc::clone(&host.session);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = session.worker.lock().expect("worker slot");
            panic!("poison worker owner");
        }));
        assert!(
            host.join_previous_worker()
                .expect_err("poisoned owner must be typed")
                .to_string()
                .contains("lock previous settings monitor")
        );
        drop(host);
    }

    #[test]
    fn drop_contains_a_worker_panic() {
        let host = test_host(Err("unused".to_owned()));
        *host.session.worker.lock().expect("worker slot") =
            Some(thread::spawn(|| panic!("drop worker fixture")));
        drop(host);
    }

    #[test]
    fn release_after_error_clears_active_and_non_registration_errors_do_not_highlight() {
        let host = test_host(Err("unused".to_owned()));
        host.session.active.store(true, Ordering::Release);
        let error = host.release_after_error("write", "denied");
        assert!(!host.session.active.load(Ordering::Acquire));
        assert!(error.to_string().contains("write: denied"));
        assert_eq!(
            registration_error_command(&PlatformError::AlreadyRunning),
            None
        );
    }

    #[test]
    fn sending_after_runtime_receiver_closes_is_nonfatal() {
        let (sender, receiver) = std::sync::mpsc::channel();
        drop(receiver);
        send(&sender, ShellCommand::Exit);
    }

    fn command_fixture(directory: &Path, name: &str, body: &str) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, format!("@echo off\r\n{body}\r\n")).expect("write command fixture");
        path
    }

    #[test]
    fn host_open_delivers_valid_response_and_process_failure() {
        let directory = tempfile::tempdir().expect("settings directory");
        let bindings = HotkeyBindings::default();
        let response = serde_json::to_string(&serde_json::json!({ "hotkeys": bindings.clone() }))
            .expect("serialize response fixture");
        let success = command_fixture(
            directory.path(),
            "settings-success.cmd",
            &format!("> \"%~4\" echo {response}\r\nexit /b 0"),
        );
        let host = test_host(Ok(success));
        let (sender, receiver) = std::sync::mpsc::channel();
        host.open(sender, &bindings, None)
            .expect("start settings host");
        match receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("receive valid settings response")
        {
            ShellCommand::ApplyBindings(received) => assert_eq!(received, bindings),
            other => panic!("unexpected settings command: {other:?}"),
        }
        host.join_previous_worker().expect("join settings host");
        assert!(!host.session.active.load(Ordering::Acquire));

        let failure = command_fixture(directory.path(), "settings-failure.cmd", "exit /b 23");
        let host = test_host(Ok(failure));
        let (sender, receiver) = std::sync::mpsc::channel();
        host.open(sender, &HotkeyBindings::default(), None)
            .expect("start failing settings host");
        match receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("receive settings failure")
        {
            ShellCommand::SettingsFailed(message) => assert!(message.contains("23")),
            other => panic!("unexpected settings command: {other:?}"),
        }
    }

    #[test]
    fn dropping_host_cancels_and_joins_the_settings_process() {
        let directory = tempfile::tempdir().expect("settings directory");
        let slow = command_fixture(
            directory.path(),
            "settings-slow.cmd",
            "ping -n 10 127.0.0.1 >nul\r\nexit /b 0",
        );
        let host = test_host(Ok(slow));
        let (sender, receiver) = std::sync::mpsc::channel();
        host.open(sender, &HotkeyBindings::default(), None)
            .expect("start slow settings host");
        drop(host);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn run_session_handles_launch_success_failure_response_and_cancellation() {
        let directory = tempfile::tempdir().expect("session directory");
        let request = directory.path().join("request.json");
        let response = directory.path().join("response.json");
        fs::write(&request, b"{}").expect("request fixture");
        let cancel = AtomicBool::new(false);

        let missing = directory.path().join("missing-settings.exe");
        assert!(
            run_session(&missing, &request, &response, &cancel)
                .expect_err("missing executable")
                .contains("launch")
        );

        let success = command_fixture(directory.path(), "success.cmd", "exit /b 0");
        assert_eq!(
            run_session(&success, &request, &response, &cancel).expect("cancel response"),
            None
        );

        let bindings = HotkeyBindings::default();
        fs::write(
            &response,
            serde_json::to_vec(&serde_json::json!({ "hotkeys": bindings }))
                .expect("serialize valid response"),
        )
        .expect("response fixture");
        assert_eq!(
            run_session(&success, &request, &response, &cancel).expect("valid response"),
            Some(HotkeyBindings::default())
        );

        let failure = command_fixture(directory.path(), "failure.cmd", "exit /b 7");
        let failure_message =
            run_session(&failure, &request, &response, &cancel).expect_err("nonzero settings exit");
        assert!(
            failure_message.contains('7'),
            "unexpected process failure: {failure_message}"
        );

        let slow = command_fixture(
            directory.path(),
            "slow.cmd",
            "ping -n 3 127.0.0.1 >nul\r\nexit /b 0",
        );
        cancel.store(true, Ordering::Release);
        assert_eq!(
            run_session(&slow, &request, &response, &cancel).expect("cancel session"),
            None
        );
    }

    #[test]
    fn discovery_resolves_a_stable_settings_executable_location() {
        let path = discover_executable().expect("discover settings sidecar");
        assert_eq!(
            path.file_name().and_then(std::ffi::OsStr::to_str),
            Some(SETTINGS_EXECUTABLE)
        );
    }
}
