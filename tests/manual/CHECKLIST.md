# Release pre-flight — manual verification (Windows)

Run through this checklist on Windows before tagging a release.

## Build

- [ ] `just build-windows` — produces
      `target/x86_64-pc-windows-msvc/release/linerule.exe` (single
      ~3-6 MB binary, no extra DLLs).
- [ ] `linerule.exe --version` prints the workspace version.
- [ ] `linerule.exe --help` lists `run` / `config show` / `config path`
      / `config edit`.

## Overlay basics

- [ ] `linerule.exe run` launches; no Dock / tray icon, no taskbar entry.
- [ ] Overlay window is invisible by default until `Ctrl+Alt+H`
      toggles visibility (or use `Ctrl+Alt+R` to enter Bar mode).
- [ ] `Ctrl+Alt+R` cycles `Bar → Mask → Vertical → Off → Bar` —
      each visible mode renders the expected geometry.

## Click-through (the load-bearing invariant)

- [ ] With Bar mode active, click anywhere on the screen.
      The click reaches the window UNDER the overlay; the overlay
      itself does not absorb it.
- [ ] Drag-select text in a browser. Selection works through the
      overlay.
- [ ] Right-click on the desktop. Context menu opens through the
      overlay.

## Cursor follow

- [ ] Move the mouse vertically in Bar mode — bar follows Y at every
      mouse position, no perceptible lag.
- [ ] Switch to Vertical mode (`Ctrl+Alt+R` × 2 from Bar) — vertical
      bar follows X.
- [ ] Switch to Mask mode — slit follows Y, screen above + below stays
      dimmed.

## Hotkeys

- [ ] `Ctrl+Alt+[` thinner — bar / slit shrinks.
- [ ] `Ctrl+Alt+]` thicker — bar / slit grows. Saturates at
      `Thickness::MAX_PX = 512`.
- [ ] `Ctrl+Alt+-` less opaque — bar / mask alpha decreases.
- [ ] `Ctrl+Alt+=` more opaque — saturates at 255.

## Configuration

- [ ] `linerule.exe config path` prints
      `%APPDATA%\linerule\config.toml`.
- [ ] `linerule.exe config edit` opens `%EDITOR%` (or notepad) with the
      file (creates parent dir + default file if missing).
- [ ] Edit `bar_color = { r=255, g=128, b=0, a=255 }` in `config.toml`
      and restart `linerule.exe run`. Bar is now orange.
- [ ] Insert `unknown_key = 1` and restart — process exits with a
      `miette` diagnostic showing the offending line / span.

## DPI scaling

- [ ] Set Windows scaling to 150% (Settings → System → Display).
      Restart `linerule.exe`. Bar visually aligns to physical mouse
      pixels with no offset.

## Multi-monitor

- [ ] On a 2-monitor setup, place the cursor on the secondary monitor
      before launching `linerule.exe run`. Overlay attaches to that
      monitor.
- [ ] Move the cursor to the primary monitor — overlay STAYS on the
      original (multi-monitor follow is v0.2; document that behaviour).

## Expected non-features (NOT bugs in v0.1)

- [ ] Exclusive-fullscreen games / video players hide the overlay
      while they own the screen — OS-level behaviour, document.
- [ ] Netflix / DRM-protected content blanks the overlay during
      playback — OS-level behaviour, document.
- [ ] Cursor moving to a different monitor leaves the overlay on the
      original one — v0.2 multi-monitor follow is in scope of a later
      ADR.

## Smoke test before push

- [ ] `just ci` passes (lint + build + test + test-doc + deny + audit
      + coverage).
- [ ] `just dist-plan` shows the expected target list
      (`x86_64-pc-windows-msvc` only at v0.1).

## Release ceremony

- [ ] `just release-dry` shows the expected version bump + CHANGELOG
      entries.
- [ ] release-plz auto-opened a release PR, merge → triggers tag →
      cargo-dist runs → GH Release with `linerule.exe` artifact + SHA256.
