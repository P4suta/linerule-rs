//! Inject `Ctrl+Alt+<key>` chords via `SendInput` to drive the overlay's
//! `RegisterHotKey` hotkeys. Windows + interactive desktop only (synthetic
//! input can't reach a `RegisterHotKey` owner on a headless runner).
//!
//! Usage: `inject_chords <action,action,...> [delay_ms]`, action = a field name
//! of [`linerule_core::input::hotkey_map::HotkeyMap`]. Chords come from
//! `HotkeyMap::DEFAULT` so the injector matches what the app registered.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "inject_chords is a dev verification tool; console output is its job"
)]

fn main() {
    let mut args = std::env::args().skip(1);
    let actions = args.next().unwrap_or_default();
    let delay_ms: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(400);
    run(&actions, delay_ms);
}

#[cfg(target_os = "windows")]
fn run(actions: &str, delay_ms: u64) {
    use std::thread::sleep;
    use std::time::Duration;

    use linerule_core::input::chord::parse;
    use linerule_core::input::hotkey_map::HotkeyMap;
    use linerule_core::input::win32_vk::chord_to_win32;
    use linerule_platform_windows::send_chord;

    let map = HotkeyMap::DEFAULT;
    // Let the overlay's message loop start draining input; chords sent
    // immediately after spawn get dropped.
    sleep(Duration::from_millis(delay_ms));

    for action in actions.split(',').filter(|s| !s.is_empty()) {
        let Some(chord_str) = chord_for(&map, action) else {
            eprintln!("inject_chords: unknown action `{action}`");
            std::process::exit(2);
        };
        let spec = match parse(chord_str) {
            Ok(spec) => spec,
            Err(e) => {
                eprintln!("inject_chords: parse `{chord_str}`: {e}");
                std::process::exit(2);
            },
        };
        let (modifiers, vk) = chord_to_win32(spec);
        if let Err(e) = send_chord(modifiers, vk) {
            eprintln!("inject_chords: send_chord {action}: {e}");
            std::process::exit(1);
        }
        println!("inject_chords: sent {action} ({chord_str})");
        // Space chords past one tick so each lands on a distinct frame.
        sleep(Duration::from_millis(delay_ms));
    }
}

#[cfg(target_os = "windows")]
fn chord_for(
    map: &linerule_core::input::hotkey_map::HotkeyMap,
    action: &str,
) -> Option<&'static str> {
    Some(match action {
        "toggle_on_off" => map.toggle_on_off,
        "cycle_mode" => map.cycle_mode,
        "cycle_effect" => map.cycle_effect,
        "thicker" => map.thicker,
        "thinner" => map.thinner,
        "more_opaque" => map.more_opaque,
        "less_opaque" => map.less_opaque,
        "toggle_hud" => map.toggle_hud,
        "quit" => map.quit,
        _ => return None,
    })
}

#[cfg(not(target_os = "windows"))]
fn run(_actions: &str, _delay_ms: u64) {
    eprintln!("inject_chords: Windows + interactive desktop only; no-op here.");
}
