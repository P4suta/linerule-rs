//! `windows_subsystem = "windows"` 下でも CLI モードのときだけ console を
//! 接続する。
//!
//! `AttachConsole(ATTACH_PARENT_PROCESS)` を試し、失敗時は `AllocConsole`
//! でフォールバック。
//!
//! 本ファイルは唯一 `linerule-app` 内で `unsafe` を必要とする箇所だが、
//! Win32 API 呼び出しが `#![cfg(target_os = "windows")]` で gate され、Linux
//! 側ではビルドされない。Linux ビルドのために `cfg(not(windows))` の no-op
//! も同居する。

#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]
#![cfg_attr(
    target_os = "windows",
    allow(unsafe_code, reason = "Console attach は Win32 API 直叩き")
)]

/// 親プロセスのコンソールを attach し、なければ新規 allocate する。
/// stdout / stderr / stdin を再バインドしてから `println!` 等が可視化される。
pub(crate) fn ensure_console_attached() {
    #[cfg(target_os = "windows")]
    win::ensure_console_attached();
    #[cfg(not(target_os = "windows"))]
    {
        // 非 Windows ターゲットでは default で console あり
    }
}

#[cfg(target_os = "windows")]
mod win {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole};

    pub(crate) fn ensure_console_attached() {
        // SAFETY: AttachConsole は失敗してもプロセスを壊さない。FALSE のときは AllocConsole にフォールバック。
        let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok();
        if !attached {
            // SAFETY: AllocConsole は新規 console を割り当てる
            let _ = unsafe { AllocConsole() };
        }
        // stdout / stderr の再バインドは windows crate の AttachConsole 経由で自動。
        // Rust の println! / eprintln! は std::io::stdout/stderr を見るが、
        // それらは LSE が AttachConsole 後に CONOUT$ をオープンする。
    }
}
