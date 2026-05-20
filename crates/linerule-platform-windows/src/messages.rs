//! Custom Win32 message numbers and special hit-test return values used by
//! the overlay. Plain `const` data only; no FFI calls.

#![forbid(unsafe_code)]

/// `WM_NCHITTEST` で「クリックは下層に貫通させる」と答えるための戻り値。
/// `LRESULT(-1)` を `i32` で持つ。
pub const HTTRANSPARENT: i32 = -1;

/// `WM_APP` 帯 (0x8000–0xBFFF) のカスタムメッセージ。pacer thread（Phase F）が
/// UI thread に vsync tick を通知するために使う。Phase C ではまだ送信側がいない
/// ので定数だけ置く。
pub const WM_APP_TICK: u32 = 0x8001;
