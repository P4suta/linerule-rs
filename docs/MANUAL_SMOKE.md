# Pre-Release Manual Smoke Checklist

Human checks on real Windows hardware for what CI (`just verify` / `verify-blur` / `verify-scenario`) cannot cover: on-screen appearance and artifact trust.

## 0. Pre-gate (committed state)

- [ ] `just lint` green (fmt / clippy / deny / typos / actionlint / machete / dep-graph)
- [ ] `just test` green
- [ ] `just verify` green on hardware (events.jsonl health check)
- [ ] `just verify-blur` green (WinRT backdrop-blur)
- [ ] `just verify-scenario` green (Ctrl+Alt chord injection asserts state transitions)

## 1. Artifact trust (after asset build)

Verify signatures via the `workflow_dispatch` `tag=main` / `publish=false` smoke (does not touch immutable releases) or against real release assets ([SIGNING.md](SIGNING.md) / [SUPPLY_CHAIN.md](SUPPLY_CHAIN.md)).

- [ ] Signature: `Get-AuthenticodeSignature linerule-vX.Y.Z-win-x64.exe` reports `Status: Valid` (skip when intentionally shipping unsigned without secrets; note this in release notes)
- [ ] Checksum: `sha256sum -c SHA256SUMS.txt` matches (or `Get-FileHash` on Windows)
- [ ] Provenance: `gh attestation verify linerule-vX.Y.Z-win-x64.exe --repo P4suta/linerule-rs` succeeds
- [ ] Release contains all three: EXE, SBOM, `SHA256SUMS.txt`

## 2. Launch and hotkeys (hardware)

Double-click the distributed EXE:

- [ ] Full panel shows for a few seconds, then auto-collapses to the top-right chip
- [ ] `Ctrl+Alt+H` shows the slit; pressing again hides it (returns to prior mode)
- [ ] `Ctrl+Alt+R` toggles axis H ⇄ V
- [ ] `Ctrl+Alt+E` cycles effect Dim → White → Blur (Blur appearance is hardware-dependent)
- [ ] `Ctrl+Alt+↑/↓` changes thickness; `Ctrl+Alt+→/←` changes opacity (blur σ during Blur). Hold for continuous adjustment
- [ ] `Ctrl+Alt+K` toggles chip ⇄ full panel
- [ ] `Ctrl+Alt+Q` exits (no leftover process)
- [ ] If possible, confirm it spans the full virtual screen on multi-monitor

## 3. Log check (after exit)

- [ ] No `ERROR` lines in `events.jsonl.YYYY-MM-DD` beside the EXE
- [ ] No `crash-*.json` generated
- [ ] `linerule diagnostics --recent-events 50` shows no anomalies

## Result

- [ ] **PASS** — signature/checksum/provenance verified; launch, hotkeys, and logs normal
- [ ] **FAIL** — record the failed step + `events.jsonl` excerpt + screenshot, and abort the release
