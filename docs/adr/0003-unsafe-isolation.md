# ADR 0003 — Confine `unsafe` to one FFI-boundary file

- Date: 2026-05-20
- Status: accepted
- Author: P4suta

## Context

`linerule-platform-windows` calls Win32 / COM / DirectComposition / Direct2D / DirectWrite / D3D11 directly. The `windows` crate's Win32 APIs are essentially all `unsafe fn`, so `unsafe` blocks are unavoidable. The default rule is "no unsafe", but a click-through + per-pixel alpha + Alt+Tab-hidden overlay requires `DirectComposition` + `WS_EX_LAYERED` + `WS_EX_NOREDIRECTIONBITMAP` + `WS_EX_TOOLWINDOW`; zero `unsafe` is impossible under any abstraction.

## Decision

Confine `unsafe` to **a single file, `crates/linerule-platform-windows/src/win32_ffi.rs`**. Specifically:

1. At the top of `win32_ffi.rs`:
   ```rust
   #![allow(
       unsafe_code,
       reason = "FFI boundary. Win32 / COM APIs are all unsafe fn even via the windows crate.
                 Other modules are #![forbid(unsafe_code)]; concentrate it only here.
                 See ADR-0003 for details."
   )]
   ```
2. **Every** other `.rs` file under `crates/linerule-platform-windows/src/` declares `#![forbid(unsafe_code)]` at the top of the file.
3. `lib.rs` keeps `#![cfg(windows)]` + `#![deny(unsafe_op_in_unsafe_fn)]` but writes no `unsafe` itself (only `pub mod` declarations).
4. `win32_ffi.rs` is a collection of thin safe wrappers:
   - Each `pub fn` is just a few lines of `unsafe { windows::Win32::...::CallW(...) }` plus mapping errors into a Result.
   - Argument and return types may come from the windows crate, but the unsafe in the function body is localized.
   - A `// SAFETY: …` comment is required immediately before each `unsafe { }` block.
5. The dispatch logic in `wndproc.rs` is also `forbid(unsafe_code)`. The `extern "system" fn` body lives in `win32_ffi.rs` as `win32_ffi::overlay_wnd_proc`.

## Scope

| File | unsafe policy |
|---|---|
| `win32_ffi.rs` | `#![allow(unsafe_code, reason = "...")]`, includes `unsafe extern "system" fn` |
| `lib.rs` | no `unsafe` appears (guarded by `#![deny(unsafe_op_in_unsafe_fn)]`) |
| `error.rs`, `messages.rs`, `overlay_state.rs`, `window_class.rs`, `wndproc.rs`, `overlay_window.rs`, `ex_style_snapshot.rs`, `monitor_info.rs`, `windows_app.rs` | all `#![forbid(unsafe_code)]` |
| examples / tests | `#![forbid(unsafe_code)]` (including the smoke test `main.rs`) |

## Mechanical verification

```bash
# Ensure win32_ffi.rs is the only file containing unsafe
grep -lr '^#!\[allow(unsafe_code' crates/linerule-platform-windows/src/ \
  | grep -v '/win32_ffi.rs$' \
  && exit 1 || true

# Every file other than win32_ffi.rs must contain forbid(unsafe_code)
for f in $(find crates/linerule-platform-windows/src -name '*.rs' ! -name 'win32_ffi.rs' ! -name 'lib.rs'); do
  grep -q '^#!\[forbid(unsafe_code\b' "$f" || (echo "missing forbid: $f" && exit 1)
done
```

Wire the above checks into `xtask lint` (future).

## Consequences

- User-facing code (dispatch, OverlayWindow, MonitorInfo, run_message_pump, etc.) is `forbid(unsafe_code)`. The `unsafe` review surface is closed within the single file `win32_ffi.rs`.
- DirectComposition / Direct2D / DWrite / D3D11 wrappers are absorbed into `win32_ffi.rs` or its submodules (`win32_ffi/graphics.rs`, etc.). **Adding any new `#![allow(unsafe_code)]` file requires an ADR.**
