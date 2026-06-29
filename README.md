# linerule-rs

A Rust reading ruler for Windows. A transparent click-through overlay draws a horizontal or vertical slit to guide the eye.

> Pre-1.0. End-user setup is this section; developer setup is [Quick start](#quick-start). For problems see [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).

## Install and use

No build required. No installer, no registry — **drop a single executable** and run.

**Requirements**

- Windows 10 / 11 (x64)
- A GPU supporting WinRT Composition + Direct2D (effectively any current machine). `Blur` appearance depends on the GPU / compositor.
- No extra runtime — a single native `linerule.exe`; no `.NET` or Visual C++ redistributable.

**Get it**

1. Download `linerule-vX.Y.Z-win-x64.exe` from [GitHub Releases](https://github.com/P4suta/linerule-rs/releases) (built per tag by `.github/workflows/release-assets.yml`; ADR-0010 / ADR-0011).
2. Releases are Authenticode-signed when signing secrets are set ([docs/SIGNING.md](docs/SIGNING.md)). Unsigned builds, or builds with little reputation, may trigger SmartScreen on first launch; verify the publisher, then "More info" → "Run anyway". To check integrity and provenance see [Verify artifacts](#verify-artifacts).

**Launch and operate**

1. Double-click `linerule.exe`. The full panel with a hotkey guide shows for a few seconds, then collapses to a small persistent chip at the top-right.
2. `Ctrl+Alt+H` turns the slit on (initial state `Off`). Adjust with the keys below.
3. `Ctrl+Alt+K` reopens the full panel (Mode / Thickness / Opacity / Effect / Refresh Hz and the hotkey list) at any time.

**Portable use**

- Drop `linerule.exe` in any folder. No install step.
- No settings persistence yet (exit resets to defaults). Tune per machine via the env vars below.
- The runtime log `events.jsonl.YYYY-MM-DD` and any crash dump `crash-*.json` are written to **the same folder as the exe** (portable by design, ADR-0011).

**Uninstall**

Delete `linerule.exe`. It writes no registry keys, startup entries, scheduled tasks, or anything under `%AppData%`. To drop logs too, delete the folder's `events.jsonl.*` / `crash-*.json`.

## Hotkeys

| Key | Action |
|---|---|
| `Ctrl+Alt+H` | Toggle on / off (restores the last active mode) |
| `Ctrl+Alt+R` | Switch axis (Horizontal ⇄ Vertical) |
| `Ctrl+Alt+E` | Cycle surround effect (Dim → White → Blur → Dim) |
| `Ctrl+Alt+↑` / `Ctrl+Alt+↓` | Slit thickness ± (hold to repeat) |
| `Ctrl+Alt+→` / `Ctrl+Alt+←` | Opacity ± (during `Blur`, adjusts **blur amount σ**) |
| `Ctrl+Alt+K` | Toggle HUD (chip ⇄ full panel) |
| `Ctrl+Alt+Q` | Quit |

## FAQ

- **Admin rights?** No. Runs as a normal, unelevated user.
- **Settings saved?** No, currently volatile (defaults each launch). Tune `Blur` feel via the env vars below.
- **Multi-monitor?** Yes. The overlay spans the whole virtual screen.
- **Hotkeys not firing / Blur looks flat.** Known IME/keyboard-layout and GPU-dependent cases; see [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).
- **Want to contribute / report a bug.** See [CONTRIBUTING.md](CONTRIBUTING.md) and the issue templates. Report vulnerabilities privately per [.github/SECURITY.md](.github/SECURITY.md).

### Behavior notes

`Off` is the only hidden state. Pressing axis/thickness/opacity/effect keys while off changes nothing and shows the toast "Overlay is off — Ctrl+Alt+H to show" (no surprise changes to an invisible overlay).

Bump keys (thickness, opacity/blur amount) repeat on hold. Axis switch, on/off, effect cycle, and quit are one-press-one-fire to avoid accidental repeats. These use Arrow keys rather than OEM keys (`[`/`]`/`=`/`-`) because IME / keyboard layout can mangle OEM-key VKs so `RegisterHotKey` misses the capture (reproduced on JIS keyboard × ENG IME).

All changes use a sub-200ms ease-out transition; held repeats retarget into one continuous motion. Dim ⇄ White cross-fades the mask RGB; flat ⇄ Blur changes brush type (sprite-pool rebuild) and so soft-cuts via a master envelope.

**HUD is two-stage**: normally just a tiny chip at the top-right, which only points to the full panel — `Ctrl+Alt+K` when shown, `Off · Ctrl+Alt+H` when off (axis/effect/values are visible on screen, so the chip omits them). The full panel (Mode / Thickness / Opacity / Effect / Refresh Hz and hotkey list) shows briefly at startup, then collapses to the chip; reopen it with `Ctrl+Alt+K`. During `Blur` the Opacity row becomes `Blur: N px`. The hotkey list shows **only keys that act right now** — off shows just on/off, HUD toggle, and quit; shown lists all keys. The HUD fades out when the slit or cursor nears it. The overlay spans the whole virtual screen on multi-monitor.

### Blur effect and composition backend

The composition backend is WinRT `Windows.UI.Composition` only (the old Win32 DirectComposition backend and the `LINERULE_COMPOSITOR` env var were removed, ADR-0016). `Blur` renders continuously via WinRT backdrop blur. No tint is layered, but pure blur looks flat on real hardware, so the pipeline appends D2D Saturation and Contrast after the blur for a frosted-glass feel (`backdrop → GaussianBlur → Saturation → Contrast`). Override strength without rebuilding via `LINERULE_BLUR_SATURATION` (`[0,1]`, default 0.70, 0.5 = original) and `LINERULE_BLUR_CONTRAST` (`[-1,1]`, default 0.15, 0 = original). Blur amount (Gaussian σ) is set by `Ctrl+Alt+Right/Left` (default ≈9px, range ≈2–64px); steps scale σ geometrically per the Weber–Fechner law so each tap feels roughly equal at any strength. Blur appearance depends on the GPU / compositor, so verify on hardware. If backdrop sampling fails and it looks like a flat color, set `LINERULE_BLUR_HOST=1` to switch backdrop source (`CreateBackdropBrush` ↔ `CreateHostBackdropBrush`) for comparison.

## Layout

| Crate | Role |
|---|---|
| `linerule-core` | Pure logic: ADTs / reducer / render / chord parser / hold FSM / tick pipeline. `#![forbid(unsafe_code)]` |
| `linerule-platform-windows` | Win32 / COM layer over WinRT Composition + Direct2D + DXGI + D3D11. `#![cfg(windows)]` |
| `linerule-app` | Single-binary `linerule.exe` entry point. `windows_subsystem = "windows"` + subcommands for GUI / CLI |
| `xtask` | Build automation: `lint` / `dep-graph` / `ci` |

Dependency direction is one-way: `linerule-app → linerule-platform-windows → linerule-core`, enforced by `cargo xtask dep-graph`.

## Quick start

### Dev environment

Two paths: **Docker** (host needs only Docker and [`just`](https://github.com/casey/just); all Rust tools — `cargo`, `cargo-xwin`, `cargo-deny`, `cargo-nextest`, `cargo-machete`, `cargo-llvm-cov`, `cargo-audit`, `cargo-sort`, `typos`, `taplo`, `biome`, `yamlfmt`, `lefthook`, `actionlint`, `commitlint` — live in the container) or **native Windows** (see below). `just` auto-detects which.

```bash
just bootstrap      # one-shot: docker build + git hooks + xwin sysroot prefetch + doctor
```

`just bootstrap` does:

1. **Pull the dev image** — `docker compose pull` for `ghcr.io/p4suta/linerule-rs-dev:latest`, falling back to `docker compose build` (the fallback pulls `GITHUB_TOKEN` from `gh auth token` to dodge cargo-binstall's api.github.com rate limit). CI (`.github/workflows/dev-image.yml`) refreshes the image weekly and on Dockerfile changes, so a pull is usually ~30s.
2. `docker compose up -d dev` — a persistent container that speeds up later `just <recipe>`.
3. `lefthook install` — installs pre-commit / commit-msg / pre-push hooks into `.git/hooks/`.
4. `bun install` — commitlint for the commit-msg hook ([Bun](https://bun.sh/), not npm).
5. `just doctor` — checks all tools.

The Windows cross-compile MSVC CRT / Windows SDK (~500 MB) is baked into the dev image, so the first `just cross-check` passes immediately.

`just doctor` reports which tool is broken.

#### Native Windows dev (no Docker)

Docker is the reproducible cross-platform path, but this is a Windows-only app, so **developing natively on a Windows host is a first-class path** (overlay rendering can't run in a Linux container). `just` auto-detects mode: `INSIDE_CONTAINER=1` means in-container; no Docker means native; Docker present means via docker. To force native on a host with Docker, use `LINERULE_NATIVE=1 just <recipe>`.

Host tools come from [mise](https://mise.jdx.dev/) (pinned in the repo-root `mise.toml`):

```bash
mise install        # cargo-nextest / cargo-deny / biome / yamlfmt etc.
just bootstrap      # native: rustup component + git hooks + doctor-native
just doctor-native  # checks required tools (mold/clang/xwin excluded on native)
```

Native-only strengths:

```bash
just run                    # actually renders the overlay (impossible in a container)
just verify                 # GUI smoke: launch for a few seconds, judge health from events.jsonl
just verify-blur            # launch Horizontal + Blur and verify WinRT backdrop blur
just verify-scenario        # inject Ctrl+Alt chords via SendInput and assert state transitions
just publish-windows-native # native-build the shippable linerule.exe
```

`just verify` shares the same judgment logic as the CI release build (`cargo xtask verify`): after launch it reads `events.jsonl` and confirms 0 ERROR, no `tick processing failed`, no crash dump, and that `Win32 message loop exited` was reached (a nonzero exit from headless-only WinRT teardown is allowed; real hardware exits 0).

> Line endings: the repo forces LF via `.gitattributes` (rustfmt / biome / taplo / yamlfmt all assume LF). A Windows clone made before this file existed should run `git add --renormalize . && git checkout .` once to normalize the working tree to LF so `just fmt` / `just lint` / hooks pass.

#### Speed numbers (measured)

| Setup | Fresh clone → cross-check passing |
|---|---|
| Build without token (cargo-binstall hits api.github.com 60/h → 403 → 120s × N retry → source fallback, then 7m xwin download) | **~20 min** |
| Build with token (cargo-binstall prebuilt, xwin sysroot baked into image) | **~2.4 min** |
| ghcr.io pull (CI builds and pushes; ~1.7 GB pull including xwin sysroot) | **~30 s** |

### Build, test, lint

```bash
just build          # cargo build --workspace --all-targets
just test           # cargo nextest run --workspace
just lint           # fmt + clippy + cargo-deny + typos + actionlint + cargo-machete + dep-graph
just run            # cargo run -p linerule-app (Windows host only)
```

### Cross-compile check

Use `cargo-xwin` to catch Windows-target type/syntax drift on Linux:

```bash
just cross-check        # cargo xwin check --target x86_64-pc-windows-msvc --workspace
just publish-windows-cross  # iteration cross-build (not shippable)
```

The shippable `linerule.exe` is produced only on the CI windows-latest runner (to avoid ABI / SEH accidents).

### Artifacts

Each tag push attaches the following to the GitHub Release (`.github/workflows/release-assets.yml`; ADR-0010 / ADR-0011 / ADR-0014 / ADR-0017):

- `linerule-vX.Y.Z-win-x64.exe` — native Windows build of `cargo build --release -p linerule-app` (one binary; no PDB or `dist-dev` profile). Authenticode-signed when signing secrets are set.
- `linerule-vX.Y.Z-sbom.cdx.json` — CycloneDX 1.6 JSON SBOM from `cargo-sbom`; scanners read `linerule-app`'s dependency closure directly.
- `SHA256SUMS.txt` — SHA-256 of the EXE and SBOM.

The EXE and `SHA256SUMS.txt` carry a keyless build-provenance attestation; the SBOM carries an SBOM attestation (Sigstore, OIDC, no stored secret; ADR-0017).

### Verify artifacts

```bash
# integrity (tamper check)
sha256sum -c SHA256SUMS.txt
# Windows: (Get-FileHash -Algorithm SHA256 linerule-vX.Y.Z-win-x64.exe).Hash

# provenance (which commit / workflow built it)
gh attestation verify linerule-vX.Y.Z-win-x64.exe --repo P4suta/linerule-rs

# Authenticode signature (when signed, Windows)
# Get-AuthenticodeSignature linerule-vX.Y.Z-win-x64.exe
```

See [docs/SUPPLY_CHAIN.md](docs/SUPPLY_CHAIN.md) (supply chain and verification) and [docs/SIGNING.md](docs/SIGNING.md) (signing runbook).

### Logs and crash dumps

At runtime, tracing JSON Lines stream to `events.jsonl.YYYY-MM-DD` next to `linerule.exe` (portable, ADR-0011). On panic, `crash-<run_id>-<unix_ms>.json` is written alongside.

```bash
just logs-tail subsystem=wnd_proc  # filter by subsystem
just logs-pretty                    # pretty-print all
just crash-list                     # list crash dumps
just crash-latest                   # latest crash dump
```

## Library API overview

The block below is auto-synced by `cargo rdme` from the crate-level doc in `crates/linerule-core/src/lib.rs`. Do not edit it by hand (regenerate with `just docs`).

<!-- cargo-rdme start -->

linerule-core

Pure logic layer: ADTs, reducer, render, parser, FSM. `#![forbid(unsafe_code)]`
bans `unsafe` outright; nondeterminism (time, randomness, I/O) is passed in
by the caller as arguments.

#### Modules

- [`anim`] — integer-endpoint timed transitions (`Transition<T>`) and easing
- [`color`] — `Rgba` / `Opacity` / `DimLevel` / `Thickness` / `BlurAmount` and perceptual curves
- [`config`] — `UserConfig` tree (`OverlayConfig` / `HudConfig` / ...)
- [`diagnostics`] — `LineruleError` / `Severity`
- [`geometry`] — coordinate-space-tagged `Point<S>` / `ScreenRect<S>`
- [`input`] — chord parser / hold FSM / tick pipeline / HUD fade / hotkey map
- [`render`] — `OverlayFrame` ADT and the pure `render::frame`
- [`state`] — `State` / `OverlayAction` / `StateDelta` and `state::reduce::apply`

#### Short public paths

Key types are re-exported here, so consumers write short paths like
`linerule_core::Rgba` / `linerule_core::frame(...)`. Internal code uses the
long paths (`linerule_core::color::rgba::Rgba`), leaving room to refactor.

#### Dependency direction

`linerule-app` → `linerule-platform-windows` → `linerule-core`. This crate
depends on no other linerule-rs crate.

<!-- cargo-rdme end -->

## Module tree and dependency graph

- [`docs/modules/`](docs/modules/) — `cargo modules structure` output per crate (auto-generated)
- [`docs/dep-graph.svg`](docs/dep-graph.svg) — workspace dependency graph (`cargo depgraph`, auto-generated)

Regenerate all with `just docs`. The `lefthook` pre-commit detects drift and blocks committing stale output.

## Design and ops docs

Design decisions are recorded as ADRs in [`docs/adr/`](docs/adr/). For merge-blocker principles (one-way dependencies, RAII, exhaustive match, localized unsafe) see [`docs/adr/0002-architecture-principles.md`](docs/adr/0002-architecture-principles.md).

## License

Dual-licensed under MIT or Apache-2.0, at your option.

- [`LICENSE-MIT`](LICENSE-MIT)
- [`LICENSE-APACHE`](LICENSE-APACHE)
