//! `RegisterHotKey` を受信する message-only HWND を立てて、
//! `WM_HOTKEY` を `OverlayAction` に変換し SPSC channel へ流す。

#![forbid(unsafe_code)]
#![cfg(windows)]

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use linerule_core::{ChordSpec, HotkeyMap, KeyCode, Letter, Modifiers, OverlayAction};
use windows::Win32::Foundation::HWND;
use windows::core::w;

use crate::error::{Result, Win32Error};
use crate::win32_ffi::{self, hotkey as hotkey_ffi};

/// chord 受信用 message-only HWND を所有し、`WM_HOTKEY` を `OverlayAction` の
/// channel に転送する。Drop で全 hotkey の `UnregisterHotKey` + `DestroyWindow`。
pub struct HotkeyHost {
    hwnd: HWND,
    sender: Sender<OverlayAction>,
    receiver: Receiver<OverlayAction>,
    /// hotkey id → action / chord の双方向マップ
    id_to_action: HashMap<i32, OverlayAction>,
    id_to_chord: HashMap<i32, ChordSpec>,
}

impl HotkeyHost {
    /// 起動時の `HotkeyMap` を一括登録した HotkeyHost を作る。
    ///
    /// # Errors
    /// message-only HWND 作成、chord 解析、`RegisterHotKey` のいずれかが失敗したとき。
    pub fn new(map: HotkeyMap) -> Result<Self> {
        let hwnd = hotkey_ffi::create_message_only_window(
            w!("linerule-rs-hotkey-host"),
            w!("linerule-hotkey"),
            Some(win32_ffi::overlay_wnd_proc),
        )?;

        let (sender, receiver) = channel::<OverlayAction>();
        let mut host = Self {
            hwnd,
            sender,
            receiver,
            id_to_action: HashMap::new(),
            id_to_chord: HashMap::new(),
        };

        host.register_pair(1, &map.cycle_mode, OverlayAction::CycleMode)?;
        host.register_pair(2, &map.toggle_visible, OverlayAction::ToggleVisible)?;
        host.register_pair(3, &map.thicker, OverlayAction::BumpThickness(8))?;
        host.register_pair(4, &map.thinner, OverlayAction::BumpThickness(-8))?;
        host.register_pair(5, &map.more_opaque, OverlayAction::BumpOpacity(8))?;
        host.register_pair(6, &map.less_opaque, OverlayAction::BumpOpacity(-8))?;
        host.register_pair(7, &map.quit, OverlayAction::Quit)?;

        Ok(host)
    }

    fn register_pair(&mut self, id: i32, spec: &str, action: OverlayAction) -> Result<()> {
        let chord = linerule_core::input::chord::parse(spec)
            .map_err(|e| Win32Error::BadHr {
                operation: "ChordParser::parse",
                hr: -1,
            })
            .map_err(|_| Win32Error::LastError {
                operation: "ChordParser::parse",
                code: 0,
                symbol: "chord parse failed",
            })?;
        // TODO Phase E: chord 解析エラーは LineruleError 経由で linerule-core から伝搬する。
        // 現状は HotkeyHost::Result（Win32Error）でしか返せないため、雑に LastError に潰している。
        let (mods, vk) = chord_to_win32(chord);
        hotkey_ffi::register_hotkey(self.hwnd, id, mods, vk)?;
        self.id_to_action.insert(id, action);
        self.id_to_chord.insert(id, chord);
        Ok(())
    }

    /// channel から次の `OverlayAction` を non-blocking で取り出す。
    /// Phase F の tick pipeline から呼ばれる想定。
    pub fn try_drain(&self) -> Vec<OverlayAction> {
        let mut out = Vec::new();
        while let Ok(a) = self.receiver.try_recv() {
            out.push(a);
        }
        out
    }

    /// chord → action の lookup（WndProc から呼ぶ）。
    #[must_use]
    pub fn action_for(&self, id: i32) -> Option<OverlayAction> {
        self.id_to_action.get(&id).copied()
    }
}

impl Drop for HotkeyHost {
    fn drop(&mut self) {
        for &id in self.id_to_action.keys() {
            if let Err(e) = hotkey_ffi::unregister_hotkey(self.hwnd, id) {
                tracing::warn!(?id, error = %e, "UnregisterHotKey failed");
            }
        }
        if let Err(e) = win32_ffi::destroy_window(self.hwnd) {
            tracing::warn!(error = %e, "DestroyWindow(hotkey host) failed");
        }
    }
}

/// `ChordSpec` を `RegisterHotKey` の (modifiers, vk) に変換する。
fn chord_to_win32(chord: ChordSpec) -> (u32, u32) {
    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_WIN: u32 = 0x0008;

    let mut mods = 0u32;
    if chord.modifiers.contains(Modifiers::ALT) {
        mods |= MOD_ALT;
    }
    if chord.modifiers.contains(Modifiers::CTRL) {
        mods |= MOD_CONTROL;
    }
    if chord.modifiers.contains(Modifiers::SHIFT) {
        mods |= MOD_SHIFT;
    }
    if chord.modifiers.contains(Modifiers::META) {
        mods |= MOD_WIN;
    }
    let vk = key_to_vk(chord.key);
    (mods, vk)
}

fn key_to_vk(key: KeyCode) -> u32 {
    use linerule_core::Direction;
    match key {
        KeyCode::Letter(letter) => u32::from(letter.as_u8()),
        KeyCode::BracketLeft => 0xDB,  // VK_OEM_4
        KeyCode::BracketRight => 0xDD, // VK_OEM_6
        KeyCode::Minus => 0xBD,        // VK_OEM_MINUS
        KeyCode::Equal => 0xBB,        // VK_OEM_PLUS
        KeyCode::Arrow(Direction::Up) => 0x26,
        KeyCode::Arrow(Direction::Down) => 0x28,
        KeyCode::Arrow(Direction::Left) => 0x25,
        KeyCode::Arrow(Direction::Right) => 0x27,
    }
}

#[allow(
    dead_code,
    reason = "Sender は Phase F でホットキー HWND の WndProc から呼ばれる"
)]
fn _send(sender: &Sender<OverlayAction>, action: OverlayAction) {
    let _ = sender.send(action);
}

#[allow(dead_code, reason = "Letter import 警告抑止")]
const _: fn() = || {
    let _: Option<Letter> = None;
};
