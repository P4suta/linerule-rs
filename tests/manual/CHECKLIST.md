# Release pre-flight — manual verification (Windows)

The Windows event-loop / vello renderer / Win32 click-through wiring
has been compiled into `dist/linerule.exe` from the WSL2 dev container,
but **runtime behaviour can only be verified on a real Windows host**.
This checklist walks through that verification.

## 0. Build the artifact

From inside the linerule repo (WSL2 side):

```sh
just windows-exe
```

This runs `cargo xwin build --release --target x86_64-pc-windows-msvc`,
then copies the resulting `linerule.exe` (~8 MB) from the `cargo-target`
docker volume into `./dist/linerule.exe` on the host filesystem.

## 1. Run the binary from Windows

From PowerShell on the Windows side, either run via the WSL UNC path
directly or copy the exe to a Windows-side location first:

```powershell
# Option A — run in place via UNC path
\\wsl.localhost\Ubuntu\home\yasunobu\projects\linerule\dist\linerule.exe --version

# Option B — copy and run
cp \\wsl.localhost\Ubuntu\home\yasunobu\projects\linerule\dist\linerule.exe $env:USERPROFILE\linerule.exe
$env:USERPROFILE\linerule.exe --version
```

- [ ] `linerule.exe --version` prints the workspace version.
- [ ] `linerule.exe --help` lists subcommands (`run`, `config`).
- [ ] `linerule.exe config path` prints `%APPDATA%\linerule\config.toml`.

## 2. Overlay basics — `linerule.exe run`

- [ ] Process launches; no Dock icon, no taskbar entry.
- [ ] On startup, the bar mode is auto-engaged (so the user immediately
      sees something) — a translucent yellow horizontal bar appears at
      the cursor's Y.
- [ ] `Ctrl+Alt+R` cycles `Bar → Mask → Vertical → Off → Bar`.
- [ ] `Ctrl+Alt+H` toggles visibility on/off.

## 3. Click-through (the load-bearing invariant)

- [ ] With Bar mode active, click anywhere on the screen.
      The click reaches the window UNDER the overlay; the overlay
      itself does not absorb it.
- [ ] Drag-select text in a browser. Selection works through the overlay.
- [ ] Right-click on the desktop. Context menu opens through the overlay.

## 4. Cursor follow

- [ ] Move the mouse vertically in Bar mode — bar tracks Y at every
      position, no perceptible lag (16 ms vsync target).
- [ ] Switch to Vertical mode (`Ctrl+Alt+R` × 2) — vertical bar
      tracks X, for 縦書き reading.
- [ ] Switch to Mask mode — slit tracks Y, screen above + below dims.

## 5. Hotkey adjustments

- [ ] `Ctrl+Alt+[` thinner — bar / slit shrinks (saturates at 1 px).
- [ ] `Ctrl+Alt+]` thicker — bar / slit grows (saturates at 512 px).
- [ ] `Ctrl+Alt+-` less opaque — alpha decreases (saturates at 1).
- [ ] `Ctrl+Alt+=` more opaque — alpha grows (saturates at 255).

## 6. Configuration

- [ ] `linerule.exe config edit` opens `%EDITOR%` (or notepad) with the
      file (creates parent dir + default file if missing).
- [ ] Edit `bar_color = { r=255, g=128, b=0, a=255 }` and re-launch
      `linerule.exe run`. Bar is now orange.
- [ ] Insert `unknown_key = 1` and restart — the binary exits with a
      `miette` diagnostic showing the offending line / span.

## 7. DPI scaling

- [ ] Set Windows scaling to 150% (Settings → System → Display).
      Restart `linerule.exe`. Bar visually aligns to physical mouse
      pixels with no offset (we do logical-px arithmetic in
      `linerule-core::render` and convert at the platform boundary).

## 8. Multi-monitor

- [ ] On a 2-monitor setup, place the cursor on the secondary monitor
      before launching. Overlay attaches to that monitor.
- [ ] Move the cursor to the primary monitor — overlay STAYS on the
      original (multi-monitor follow is v0.2; this is expected).

## 9. Expected non-features (NOT bugs in v0.1)

- [ ] Exclusive-fullscreen games / video players hide the overlay
      while they own the screen. OS-level behaviour, document.
- [ ] Netflix / DRM-protected content blanks the overlay during
      playback. OS-level behaviour, document.
- [ ] Cursor moving to a different monitor leaves the overlay on the
      original one. v0.2 multi-monitor follow is in scope of a later
      ADR.

## 10. Smoke test inside the dev container

These run on the WSL2 / Linux side and gate every commit / push:

- [ ] `just lint` passes (fmt + typos + strict-code + shear + clippy).
- [ ] `just test` passes (75/75 tests).
- [ ] `just coverage` passes (branch coverage 100% on pure crates).
- [ ] `just build-windows` produces a fresh `dist/linerule.exe`.

## 11. Release ceremony

- [ ] `just release-dry` shows the expected version bump + CHANGELOG.
- [ ] release-plz auto-opens a release PR; merging triggers a tag
      which triggers cargo-dist; a GitHub Release is published with
      `linerule.exe` artifact + SHA256 checksums + a PowerShell
      installer script.

## Known runtime fragilities — to confirm on first real run

The compile boundary verifies type / ABI shapes; some runtime
properties cannot be statically known and need a first-run signal:

- The `Win32 SetWindowLongPtrW(GWL_EXSTYLE)` flow uses a re-read after
  the call to disambiguate "previous value was 0" from "error". If the
  re-read also returns 0 the binary aborts cleanly with
  `RunError::ClickThrough`.
- `wgpu::Instance::default()` will probe DX12 first (fast on Win11)
  and fall back to Vulkan if DX12 is unavailable. If neither works the
  binary exits with `RunError::Renderer("request_adapter: …")`.
- The `global-hotkey` crate registers `Ctrl+Alt+R/H/[/]/-/=` system-
  wide. If any of these are already bound by another running app
  (e.g. a recording tool's `Ctrl+Alt+R`), `RunError::Hotkey` is
  returned with the offending chord — change in `config.toml` and
  re-run.
