# linerule

A click-through reading-ruler overlay for Windows 11. linerule starts hidden,
lives in the notification area, and draws only while the ruler or a short
status guide is visible.

<!-- x-release-please-start-version -->
Version 0.6.1 supports Windows 11 build 26100 or newer on x64 and ARM64.
<!-- x-release-please-end -->

## Install

Stable releases contain:

- `linerule.msixbundle` and `linerule.appinstaller`
- separate x64 and ARM64 portable ZIPs
- CycloneDX and SPDX SBOMs
- `SHA256SUMS.txt`

The installed build uses MSIX LocalState. A portable ZIP includes the
`linerule.portable` marker and keeps settings, seven days of logs, and the five
newest crash reports under its own `data/` directory.

## Use

The ruler always starts Off. Left-click the tray icon to show or hide it.
Right-click exposes only Show/Hide, Shortcut settings…, and Exit. The first
launch shows the guide for five seconds; `Ctrl+Alt+K` opens it again.

Default shortcuts:

| Shortcut | Action |
|---|---|
| `Ctrl+Alt+H` | Show or hide |
| `Ctrl+Alt+R` | Horizontal / vertical |
| `Ctrl+Alt+E` | Dim / white / blur |
| `Ctrl+Alt+Up` / `Down` | Thickness |
| `Ctrl+Alt+Right` / `Left` | Opacity or blur amount |
| `Ctrl+Alt+K` | Full guide |
| `Ctrl+Alt+Q` | Exit |

Shortcut changes are validated and registered as one transaction. Invalid,
duplicate, modifierless, or externally occupied shortcuts are not partially
applied.

## Command line

```text
linerule
linerule settings
linerule diagnostics [--data-dir | --last-crash | --recent-events N]
linerule version
```

## Develop

The complete compiler and tool environment is pinned by `mise.toml`.

```text
mise install
mise exec just --command "just bootstrap"
mise exec just --command "just build test lint"
mise exec just --command "just test-cargo"
```

The last command is the standard parallel Cargo compatibility gate; nextest is
the primary runner. Unsafe Rust is allowed only below
`crates/linerule-platform-windows/src/win32_ffi/` and is checked by
`mise exec just --command "just policy"`.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the merge gates and
[docs/RELEASE.md](docs/RELEASE.md) for release operations. Architecture
decisions are retained under [docs/adr](docs/adr).

## Security and license

Report vulnerabilities using [.github/SECURITY.md](.github/SECURITY.md).
The source is available under MIT or Apache-2.0, at your option; see
[`LICENSES/`](LICENSES/).
