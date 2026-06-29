# linerule-rs

A Rust reading ruler for Windows (reading-aid overlay). A transparent, click-through window draws a horizontal/vertical slit on screen to help track your gaze.

Live controls:

- `Ctrl+Alt+H`: ON/OFF toggle (Off ⇄ last active mode). Dedicated to show/hide only
- `Ctrl+Alt+R`: axis toggle (Horizontal ⇄ Vertical)
- `Ctrl+Alt+E`: surround effect cycle (Dim → White → Blur → Dim)
- `Ctrl+Alt+Up` / `Ctrl+Alt+Down`: slit thickness ± (hold for continuous adjust)
- `Ctrl+Alt+Right` / `Ctrl+Alt+Left`: opacity ± (hold for continuous adjust). Under the `Blur`
  effect opacity is inert; instead this changes the **blur amount (Gaussian σ, px)**
- `Ctrl+Alt+K`: HUD toggle (resident chip ⇄ full panel)
- `Ctrl+Alt+Q`: quit

"Hidden" is only `Mode::Off`. Pressing an adjust key while Off changes no settings;
the HUD shows an "Overlay is off — Ctrl+Alt+H to show" toast.

Bump actions (thickness, opacity/blur) auto-repeat on hold. Axis toggle, ON/OFF, effect cycle,
and quit fire once per press to prevent accidental repeats. Arrow keys are used instead of OEM
keys (`[`/`]`/`=`/`-`) because Windows IME / keyboard layout mangles the VK of OEM keys so
`RegisterHotKey` misses the capture (reproduced on JIS keyboard × ENG IME).

Each transition is a sub-200ms ease-out (continuous adjust during a hold merges into one motion
via retarget). Dim ⇄ White is an RGB crossfade of the mask color; flat ⇄ Blur changes the brush
kind (sprite-pool rebuild), so a master-envelope soft cut covers it.

**The HUD is two-tier**: normally only a tiny resident chip in the top-right (its sole purpose is
to lead into the full panel — `Ctrl+Alt+K` while shown, `Off · Ctrl+Alt+H` while Off). For the
first few seconds after launch it shows the full panel, then auto-folds into the chip. `Ctrl+Alt+K`
expands the full panel anytime (Mode / Thickness / Opacity / Effect / Refresh Hz and a hotkey list;
under `Blur` the Opacity row reads `Blur: N px`). The hotkey list shows **only the keys that mean
something right now** (while Off: the 3 of On/Off, HUD toggle, quit). When the slit or cursor nears
the HUD it yields and fades out. On multi-monitor the overlay spans the whole virtual screen.

### Blur effect and composition backend

The composition backend is the single WinRT `Windows.UI.Composition` (ADR 0016). `Blur` renders
continuously via WinRT backdrop blur. Pure blur alone looks flat, so a D2D effect chain that raises
saturation and contrast is appended for a frosted-glass feel
(`backdrop → GaussianBlur → Saturation → Contrast`). Strength is overridable without a rebuild via
the env vars `LINERULE_BLUR_SATURATION` (`[0,1]`, default 0.70, 0.5 = original) /
`LINERULE_BLUR_CONTRAST` (`[-1,1]`, default 0.15, 0 = original). Blur amount (Gaussian σ) is adjusted
with `Ctrl+Alt+Right/Left` (default ≈9px, range ≈2–64px); the step moves σ geometrically per the
Weber–Fechner law so each tap feels like a constant change. The look depends on the real GPU /
compositor, so verify on hardware. If backdrop sampling fails and it looks like a flat color, set
`LINERULE_BLUR_HOST=1` to switch the backdrop acquisition method
(`CreateBackdropBrush` ↔ `CreateHostBackdropBrush`) for comparison.

## Structure

| Crate | Role |
|---|---|
| `linerule-core` | Pure logic layer. ADT / reducer / render / chord parser / hold FSM / tick pipeline. `#![forbid(unsafe_code)]` |
| `linerule-platform-windows` | Win32 / COM implementation layer. Drives WinRT Composition + Direct2D + DXGI + D3D11 directly. `#![cfg(windows)]` |
| `linerule-app` | Entry point of the single binary `linerule.exe`. `windows_subsystem = "windows"` + subcommands switch GUI / CLI |
| `xtask` | Build automation. `lint` / `dep-graph` / `ci` |

Dependencies flow one way: `linerule-app → linerule-platform-windows → linerule-core`. Verified mechanically with `cargo xtask dep-graph`.

## Quick start

### Dev environment

Two paths: **Docker** (host needs only Docker and [`just`](https://github.com/casey/just); the Rust tools (`cargo`, `cargo-xwin`, `cargo-deny`, `cargo-nextest`, `cargo-machete`, `cargo-llvm-cov`, `cargo-audit`, `cargo-sort`, `typos`, `taplo`, `biome`, `yamlfmt`, `lefthook`, `actionlint`, `commitlint`) live in the container) or **native Windows** (see below). `just` auto-detects.

```bash
just bootstrap      # one-shot setup: docker build + git hooks + xwin sysroot prefetch + doctor
```

What `just bootstrap` does:

1. **Pull the dev image** — `docker compose pull` for `ghcr.io/p4suta/linerule-rs-dev:latest`, falling back to `docker compose build` if absent (picking up `GITHUB_TOKEN` from `gh auth token` to dodge the cargo-binstall api.github.com rate limit). CI (`.github/workflows/dev-image.yml`) refreshes it weekly + on Dockerfile changes, so it's usually ~30s
2. `docker compose up -d dev` — a persistent container that speeds up later `just <recipe>`
3. `lefthook install` — places pre-commit / commit-msg / pre-push hooks in `.git/hooks/`
4. `bun install` — installs the commitlint the commit-msg hook uses ([Bun](https://bun.sh/))
5. `just doctor` — connectivity check of all tools

The MSVC CRT / Windows SDK (~500 MB) for Windows cross-compilation is baked into the dev image, so the first `just cross-check` passes immediately.

If stuck, `just doctor` shows which tool is broken.

#### Native Windows dev (no Docker)

Since this is a Windows-only app, **developing on a Windows host without Docker is the first-class path** (real overlay rendering is impossible in a Linux container). `just` auto-detects the run mode: `INSIDE_CONTAINER=1` means in-container, no Docker means **native**, with Docker means via docker. To force native on a Docker-equipped host, use `LINERULE_NATIVE=1 just <recipe>`.

Provision host tools with [mise](https://mise.jdx.dev/) (pinned in `mise.toml`):

```bash
mise install        # bulk-install cargo-nextest / cargo-deny / biome / yamlfmt etc.
just bootstrap      # auto-detects native: rustup component + git hooks + doctor-native
just doctor-native  # connectivity check of required tools (mold/clang/xwin are out of scope for native)
```

Strengths only native gives you:

```bash
just run                    # the overlay actually renders (impossible in a container)
just verify                 # GUI smoke: launch for a few seconds and judge health via events.jsonl
just verify-blur            # launch with Horizontal + Blur and verify WinRT backdrop-blur
just verify-scenario        # inject Ctrl+Alt chords via SendInput and assert state transitions
just publish-windows-native # native-build the shippable linerule.exe
```

`just verify` shares the same judgment logic as CI's release-build (`cargo xtask verify`) — after launch it reads `events.jsonl` and confirms 0 ERROR / no `tick processing failed` / no crash dump / `Win32 message loop exited` reached (a nonzero exit from headless-specific WinRT teardown is tolerated; on real hardware it exits 0).

> Line endings: `.gitattributes` enforces LF (rustfmt / biome / taplo / yamlfmt assume LF). An old Windows clone can align its working tree to LF once with `git add --renormalize . && git checkout .`, after which `just fmt` / `just lint` / the commit hook pass.

#### Why it's fast (measured)

| Setup | Fresh clone → cross-check passing |
|---|---|
| Build without a token (cargo-binstall hits api.github.com 60/h → 403 → 120s × N retry → source fallback, then 7m xwin download) | **~20 min** |
| Build with a token (cargo-binstall uses prebuilt, xwin sysroot baked into image) | **~2.4 min** |
| ghcr.io pull (CI builds and pushes, ~1.7 GB pull including xwin sysroot) | **~30 sec** |

### Build / test / lint

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

The shippable `linerule.exe` is produced only from CI's windows-latest runner (to avoid ABI / SEH mishaps).

### Artifacts

Each tag push attaches the following to the GitHub Release (`.github/workflows/release-assets.yml`, ADR-0010 / ADR-0011).

- `linerule-vX.Y.Z-win-x64.exe` — native Windows build of `cargo build --release -p linerule-app`
- `linerule-vX.Y.Z-sbom.cdx.json` — CycloneDX 1.6 SBOM generated by `cargo-sbom` (dependency closure of `linerule-app`)

### Logs and crash dumps

Emits tracing JSON Lines to `events.jsonl.YYYY-MM-DD` alongside `linerule.exe` (portable operation, ADR-0011). On panic, a `crash-<run_id>-<unix_ms>.json` lands in the same directory.

```bash
just logs-tail subsystem=wnd_proc  # subsystem filter
just logs-pretty                    # pretty-print everything
just crash-list                     # list crash dumps
just crash-latest                   # latest crash dump
```

## Library API overview

The block below is auto-synced from the crate-level doc of `crates/linerule-core/src/lib.rs` via `cargo rdme`. Do not hand-edit (regenerate with `just docs`).

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

## Module tree / dependency graph

- [`docs/modules/`](docs/modules/) — `cargo modules structure` output per crate (auto-generated)
- [`docs/dep-graph.svg`](docs/dep-graph.svg) — workspace dependency graph (`cargo depgraph` auto-generated)

Update with `just docs`. The `lefthook` pre-commit detects drift and blocks committing stale generated artifacts.

## Design / operations docs

Design decisions are recorded as ADRs in [`docs/adr/`](docs/adr/). For merge-blocker principles like one-way dependencies / RAII / exhaustive match / unsafe localization, see [`docs/adr/0002-architecture-principles.md`](docs/adr/0002-architecture-principles.md).

## License

Dual-licensed MIT or Apache-2.0 (your choice).

- [`LICENSE-MIT`](LICENSE-MIT)
- [`LICENSE-APACHE`](LICENSE-APACHE)
