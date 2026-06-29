# Troubleshooting

Symptom-to-fix field guide. Architecture is canonical in the [`docs/adr/`](adr/) ADRs; the user entry point is the [README](../README.md).

## Where to look first

1. **Logs** — `events.jsonl.YYYY-MM-DD`, written daily in the **same folder** as `linerule.exe` (portable design, ADR-0011). tracing JSON Lines.
2. **Crash dumps** — on panic, `crash-<run_id>-<unix_ms>.json` in the same folder.
3. **`linerule diagnostics`** — CLI to read logs/dumps without launching the app.
   ```
   linerule diagnostics --data-dir          # absolute path of the log folder
   linerule diagnostics --recent-events 50  # last 50 of today's events.jsonl
   linerule diagnostics --last-crash        # pretty-print the latest crash-*.json
   linerule diagnostics --dry-run           # check output path only (no write)
   ```
   `linerule version` reports the version (include it in bug reports).
4. **`just doctor` / `just doctor-native`** — for *dev environment* problems (toolchain mismatch, missing hooks) rather than *app* problems.

Justfile log recipes for developers:

```
just logs-tail subsystem=wnd_proc  # follow, filtered by subsystem
just logs-pretty                    # pretty-print all
just crash-list                     # list crash dumps
just crash-latest                   # latest crash dump
```

## Environment variables

`Blur` look can be overridden via env vars without a rebuild. Set them in the shell that launches `linerule.exe`.

| Variable | Default | Range | Effect |
|---|---|---|---|
| `LINERULE_BLUR_SATURATION` | `0.70` | `[0, 1]` | Post-blur saturation. `0.5` = source. |
| `LINERULE_BLUR_CONTRAST` | `0.15` | `[-1, 1]` | Post-blur contrast. `0` = source. |
| `LINERULE_BLUR_HOST` | (unset) | `0` / `1` | `1` switches backdrop capture to `CreateHostBackdropBrush`. Use to compare when the background looks flat. |
| `LINERULE_NATIVE` | (unset) | `0` / `1` | Dev. Forces `just` into native mode on hosts that have Docker (README "Native Windows development"). |

> Removed: the old Win32 DirectComposition backend and `LINERULE_COMPOSITOR` were deleted in ADR-0016. The composition backend is WinRT `Windows.UI.Composition` only.

## Symptom -> cause -> fix

| Symptom | Cause | Fix |
|---|---|---|
| Hotkeys do nothing | Another app already `RegisterHotKey`'d the same `Ctrl+Alt+*`, or the IME/keyboard layout garbles the VK | Quit the conflicting app. Thickness/opacity use Arrow keys because OEM-key (`[` `]` `=` `-`) VKs garble under JIS x ENG IME (see README). Check `events.jsonl` for registration failures. |
| Overlay not visible | `Mode::Off` (initial state) | Show with `Ctrl+Alt+H`. While Off, axis/thickness/opacity/effect keys do nothing and the HUD shows a "Overlay is off — Ctrl+Alt+H to show" toast (by design). |
| Adjust keys inert while Off | By design (prevents silent changes to a hidden overlay) | Show with `Ctrl+Alt+H` first, then adjust. |
| `Blur` looks flat/solid | Backdrop sampling not taking effect; appearance is GPU/compositor dependent | Set `LINERULE_BLUR_HOST=1` to compare `CreateBackdropBrush` vs `CreateHostBackdropBrush`. Tune with `LINERULE_BLUR_SATURATION` / `LINERULE_BLUR_CONTRAST`. |
| Opacity keys do nothing during `Blur` | By design. Opacity is inert under `Blur`; `Ctrl+Alt+→/←` instead adjust **blur sigma** (default ≈9px, range ≈2–64px) | In the full panel the Opacity row shows `Blur: N px`, the current blur amount. |
| SmartScreen warning at launch | Binary is unsigned | Verify the publisher, then "More info" -> "Run". If unsure, build it yourself (README "Cross-compile check", "Native Windows development"). |
| HUD fades out on its own | By design. The HUD yields and fades when a slit or the cursor approaches it | Move the cursor away to restore. `Ctrl+Alt+K` opens the full panel explicitly. |

## Errors and severity

Runtime errors are classified by `LineruleError` / `Severity` in `linerule-core` (`crates/linerule-core/src/diagnostics`) and recorded in `events.jsonl` and `crash-*.json`. Panic/error strategy: ADR-0007, ADR-0008. HUD notifications from the app: ADR-0012, ADR-0013.

## Reporting a bug

Follow the [bug_report template](../.github/ISSUE_TEMPLATE/bug_report.yml): include `linerule version` output, your Windows version, and an excerpt of `linerule diagnostics --recent-events` (or the `crash-*.json`). Report vulnerabilities privately per [.github/SECURITY.md](../.github/SECURITY.md), not in a public issue.

## See also

- [README](../README.md) — install, hotkey reference, FAQ.
- [CONTRIBUTING](../CONTRIBUTING.md) — dev environment and `just doctor`.
- [docs/adr/0002-architecture-principles.md](adr/0002-architecture-principles.md) — design invariants.
