//! Narrow public owner for the Windows desktop runtime.

#![forbid(unsafe_code)]

use linerule_core::{
    AnimConfig, NotificationClass, OverlayAction, Preferences, RulerPreferences, TapStepConfig,
    TickWorld,
};

use crate::error::Result;
use crate::foreground_hook::ForegroundHook;
use crate::overlay_window::OverlayWindow;
use crate::render_clock::RenderClock;

type PersistenceFn = dyn Fn(&Preferences) -> std::result::Result<(), String> + 'static;
type PersistenceCallback = Box<PersistenceFn>;

/// Requested top-level user experience.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchIntent {
    /// Start the resident shell with the ruler off.
    Resident,
    /// Open shortcut settings on startup.
    Settings,
}

/// Inputs needed to start the desktop runtime.
pub struct RuntimeOptions {
    preferences: Preferences,
    startup_notice: Option<String>,
    show_startup_guide: bool,
    persist: Option<PersistenceCallback>,
    #[cfg(test)]
    request_test_exit: bool,
    #[cfg(test)]
    test_actions: Vec<OverlayAction>,
    #[cfg(test)]
    test_shell_commands: Vec<crate::win32_ffi::shell::ShellCommand>,
}

impl RuntimeOptions {
    /// Create options from validated user preferences.
    #[must_use]
    pub fn new(preferences: Preferences) -> Self {
        Self {
            preferences,
            startup_notice: None,
            show_startup_guide: false,
            persist: None,
            #[cfg(test)]
            request_test_exit: false,
            #[cfg(test)]
            test_actions: Vec::new(),
            #[cfg(test)]
            test_shell_commands: Vec::new(),
        }
    }

    /// Add a one-time startup notification.
    #[must_use]
    pub fn with_startup_notice(mut self, notice: Option<String>) -> Self {
        self.startup_notice = notice;
        self
    }

    /// Show the five-second teaching guide for this launch.
    #[must_use]
    pub fn with_startup_guide(mut self, show: bool) -> Self {
        self.show_startup_guide = show;
        self
    }

    /// Install the application-owned atomic preferences writer. The runtime
    /// invokes it 500 ms after the latest change and synchronously on exit.
    #[must_use]
    pub fn with_persistence(
        mut self,
        persist: impl Fn(&Preferences) -> std::result::Result<(), String> + 'static,
    ) -> Self {
        self.persist = Some(Box::new(persist));
        self
    }

    #[cfg(test)]
    fn with_test_exit(mut self) -> Self {
        self.request_test_exit = true;
        self
    }

    #[cfg(test)]
    fn with_test_actions(mut self, actions: impl IntoIterator<Item = OverlayAction>) -> Self {
        self.test_actions.extend(actions);
        self
    }

    #[cfg(test)]
    fn with_test_shell_commands(
        mut self,
        commands: impl IntoIterator<Item = crate::win32_ffi::shell::ShellCommand>,
    ) -> Self {
        self.test_shell_commands.extend(commands);
        self
    }
}

/// Owner and entry point for HWNDs, hooks, hotkeys, pacing, and renderers.
pub struct DesktopRuntime;

impl DesktopRuntime {
    /// Run until the controller receives an exit request, returning values to
    /// persist on clean shutdown.
    ///
    /// # Errors
    /// Returns a typed platform error for initialization or message-pump
    /// failures.
    pub fn run(intent: LaunchIntent, options: RuntimeOptions) -> Result<Preferences> {
        options.preferences.validate()?;

        // Acquire the single-instance owner and expose tray/settings before
        // touching graphics. A duplicate exits without creating an overlay,
        // and graphics initialization may degrade without killing controls.
        let shell =
            crate::win32_ffi::shell::DesktopShell::new(options.preferences.hotkeys.clone())?;
        let monitor = crate::monitor_info::virtual_screen_bounds();
        let initial = TickWorld::with_initial_state(options.preferences.ruler.initial_state())
            .with_startup_guide(options.show_startup_guide);
        let mut overlay = OverlayWindow::new_with_initial_world(
            monitor,
            linerule_core::HudConfig::DEFAULT,
            AnimConfig::DEFAULT,
            initial,
        )?;
        if let Err(error) = overlay.attach_compositor() {
            tracing::error!(
                %error,
                "graphics initialization failed; controller remains available"
            );
            overlay.disable_rendering_until_display_retry(format!(
                "Drawing unavailable: {error}; hide and show to retry"
            ))?;
        }

        let startup_registration_error = if let Err(error) =
            overlay.register_hotkeys(&options.preferences.hotkeys, TapStepConfig::DEFAULT)
        {
            overlay.push_notification(
                NotificationClass::Warn,
                format!("Shortcut registration failed: {error}"),
                i64::MAX,
            )?;
            tracing::warn!(%error, "shortcut transaction rolled back; controller remains available");
            Some(error)
        } else {
            None
        };
        if let Some(notice) = options.startup_notice {
            overlay.push_notification(NotificationClass::Warn, notice, 10_000)?;
        }
        if let Some(error) = startup_registration_error.as_ref() {
            if let Err(settings_error) = shell.show_settings_error(error) {
                notify_settings_failure(&mut overlay, &settings_error)?;
            }
        } else if matches!(intent, LaunchIntent::Settings)
            && let Err(error) = shell.open_settings()
        {
            notify_settings_failure(&mut overlay, &error)?;
        }

        let _foreground = match ForegroundHook::install(overlay.hwnd()) {
            Ok(hook) => Some(hook),
            Err(error) => {
                tracing::warn!(%error, "foreground hook unavailable");
                None
            },
        };
        let clock = RenderClock::spawn(overlay.hwnd())?;
        overlay.install_render_clock(clock.control())?;
        #[cfg(test)]
        for action in options.test_actions.iter().copied() {
            overlay.enqueue_action(action)?;
        }
        #[cfg(test)]
        for command in options.test_shell_commands {
            shell.send_for_test(command)?;
        }
        #[cfg(test)]
        if options.request_test_exit {
            shell.request_exit_for_test()?;
        }
        let mut updated = options.preferences;
        let mut observed = updated.clone();
        let mut persisted = updated.clone();
        tracing::info!(target: "WindowsApp", "entering Win32 message loop");
        loop {
            let keep_running = match crate::win32_ffi::pump_one() {
                Some(value) => value,
                None => {
                    return Err(crate::error::PlatformError::LastError {
                        operation: "GetMessageW",
                        code: 0,
                        symbol: "GetMessageW returned -1",
                    });
                },
            };
            for command in shell.drain() {
                match command {
                    crate::win32_ffi::shell::ShellCommand::Toggle => {
                        overlay.enqueue_action(OverlayAction::ToggleOnOff)?;
                    },
                    crate::win32_ffi::shell::ShellCommand::ApplyBindings(bindings) => {
                        match overlay.register_hotkeys(&bindings, TapStepConfig::DEFAULT) {
                            Ok(()) => {
                                updated.hotkeys = bindings.clone();
                                shell.set_bindings(bindings)?;
                                overlay.push_notification(
                                    NotificationClass::Info,
                                    "Shortcuts saved".to_owned(),
                                    3_000,
                                )?;
                            },
                            Err(error) => {
                                overlay.push_notification(
                                    NotificationClass::Warn,
                                    format!("Shortcut registration failed: {error}"),
                                    10_000,
                                )?;
                                if let Err(settings_error) = shell.show_settings_error(&error) {
                                    notify_settings_failure(&mut overlay, &settings_error)?;
                                }
                            },
                        }
                    },
                    crate::win32_ffi::shell::ShellCommand::SettingsFailed(message) => {
                        tracing::warn!(%message, "Fluent shortcut settings unavailable");
                        overlay.push_notification(
                            NotificationClass::Warn,
                            format!("Shortcut settings unavailable: {message}"),
                            10_000,
                        )?;
                    },
                    crate::win32_ffi::shell::ShellCommand::FlushPreferences => {
                        match persist_preferences(options.persist.as_deref(), &updated) {
                            Ok(()) => persisted = updated.clone(),
                            Err(error) => {
                                tracing::error!(
                                    %error,
                                    "background preferences save failed; retaining dirty state"
                                );
                                overlay.push_notification(
                                    NotificationClass::Error,
                                    format!("Settings were not saved: {error}"),
                                    10_000,
                                )?;
                            },
                        }
                    },
                    crate::win32_ffi::shell::ShellCommand::Exit => {
                        crate::win32_ffi::post_quit(0);
                    },
                }
            }
            update_ruler(&mut updated, overlay.state_snapshot()?);
            if options.persist.is_some() && updated != observed {
                shell.schedule_preferences_flush()?;
                observed = updated.clone();
            }
            if !keep_running {
                break;
            }
        }
        tracing::info!(target: "WindowsApp", "Win32 message loop exited");

        let (telemetry, tick_samples) = overlay.telemetry_snapshot()?;
        let refresh_hz = crate::render_timing::refresh_rate_hz().max(1);
        let frame_budget_ms = 1000.0 / f64::from(refresh_hz);
        tracing::info!(
            target: "performance",
            tick_samples,
            tick_p99_ms = f64::from(telemetry.tick_p99_ms),
            frame_budget_ms,
            refresh_hz,
            frames_dropped = telemetry.frames_dropped,
            commit_timeouts = telemetry.commit_timeouts,
            within_frame_budget = f64::from(telemetry.tick_p99_ms) <= frame_budget_ms,
            "runtime performance summary"
        );
        update_ruler(&mut updated, overlay.state_snapshot()?);
        if updated != persisted {
            persist_preferences(options.persist.as_deref(), &updated)?;
        }
        Ok(updated)
    }

    /// Attach the parent console, allocating one when launched directly.
    ///
    /// # Errors
    /// Console allocation failure.
    pub fn attach_console() -> Result<()> {
        crate::win32_ffi::console::attach()
    }
}

fn update_ruler(preferences: &mut Preferences, state: linerule_core::State) {
    preferences.ruler = RulerPreferences {
        last_active: state.last_active,
        effect: state.config.effect,
        thickness: state.config.thickness,
        opacity: state.config.opacity,
        blur: state.config.blur,
    };
}

fn persist_preferences(persist: Option<&PersistenceFn>, preferences: &Preferences) -> Result<()> {
    if let Some(persist) = persist {
        persist(preferences)
            .map_err(|message| crate::error::PlatformError::Persistence { message })?;
    }
    Ok(())
}

fn notify_settings_failure(
    overlay: &mut OverlayWindow,
    error: &crate::error::PlatformError,
) -> Result<()> {
    tracing::warn!(%error, "Fluent shortcut settings unavailable");
    overlay.push_notification(
        NotificationClass::Warn,
        format!("Shortcut settings unavailable: {error}"),
        10_000,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn preference_helpers_cover_noop_failure_and_state_projection() {
        let mut preferences = Preferences::default();
        assert!(persist_preferences(None, &preferences).is_ok());

        let failure = |_value: &Preferences| Err("read-only storage".to_owned());
        let error =
            persist_preferences(Some(&failure), &preferences).expect_err("persistence must fail");
        assert!(matches!(
            error,
            crate::error::PlatformError::Persistence { message }
                if message == "read-only storage"
        ));

        let mut state = preferences.ruler.initial_state();
        state.config.thickness = linerule_core::Thickness::try_new(31).expect("valid thickness");
        state.config.opacity = linerule_core::Opacity::try_new(99).expect("valid opacity");
        update_ruler(&mut preferences, state);
        assert_eq!(preferences.ruler.thickness.get(), 31);
        assert_eq!(preferences.ruler.opacity.get(), 99);
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop and graphics stack"]
    fn runtime_exercises_settings_active_render_and_clean_persistence() {
        DesktopRuntime::attach_console().expect("test console should be available");
        let mut preferences = Preferences::default();
        preferences.ruler.effect = linerule_core::SurroundEffect::Blur;
        let persisted = Rc::new(RefCell::new(None));
        let persistence_sink = Rc::clone(&persisted);
        let persistence_attempts = Rc::new(Cell::new(0_u8));
        let attempt_counter = Rc::clone(&persistence_attempts);
        let returned = DesktopRuntime::run(
            LaunchIntent::Settings,
            RuntimeOptions::new(preferences.clone())
                .with_startup_notice(Some("test startup notice".to_owned()))
                .with_persistence(move |value| {
                    let attempt = attempt_counter.get().saturating_add(1);
                    attempt_counter.set(attempt);
                    if attempt == 1 {
                        return Err("injected transient write failure".to_owned());
                    }
                    *persistence_sink.borrow_mut() = Some(value.clone());
                    Ok(())
                })
                .with_test_actions([
                    OverlayAction::ToggleOnOff,
                    OverlayAction::BumpThickness(8),
                    OverlayAction::BumpOpacity(8),
                    OverlayAction::CycleMode,
                ])
                .with_test_shell_commands([
                    crate::win32_ffi::shell::ShellCommand::Toggle,
                    crate::win32_ffi::shell::ShellCommand::ApplyBindings(
                        preferences.hotkeys.clone(),
                    ),
                    crate::win32_ffi::shell::ShellCommand::SettingsFailed(
                        "test settings failure".to_owned(),
                    ),
                    crate::win32_ffi::shell::ShellCommand::FlushPreferences,
                    crate::win32_ffi::shell::ShellCommand::Exit,
                ])
                .with_test_exit(),
        )
        .expect("resident runtime should start and stop cleanly");
        assert_eq!(returned.hotkeys, preferences.hotkeys);
        returned
            .validate()
            .expect("returned preferences remain valid");
        assert_ne!(returned.ruler, preferences.ruler);
        assert_eq!(persistence_attempts.get(), 2);
        assert_eq!(persisted.borrow().as_ref(), Some(&returned));
    }
}
