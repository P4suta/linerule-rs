# 0017 — Release signing, checksums, and attestation (supply-chain hardening)

**Status:** Accepted (2026-06-24). Extends (does not replace) the asset flow in [[0014-immutable-release-asset-flow]]. Naming, SBOM, and draft→publish logic are inherited from ADR-0010/0014.

**Amendment (2026-07-01):** signing is no longer non-blocking for a real release. A `publish=true` run now **hard-fails at the irreversible boundary when the binary is unsigned** (ports find-my-files #127); a `publish=false` smoke test still builds unsigned. SBOM attestation migrated `actions/attest-sbom` → `actions/attest` (upstream deprecation, find-my-files #123). The sections below are amended in place.

**See also:** [[0010-release-assets-workflow]] / [[0011-phase-j-slim-down]] / [[0014-immutable-release-asset-flow]]. Runbooks: docs/SIGNING.md, docs/SUPPLY_CHAIN.md, docs/MANUAL_SMOKE.md.

## Context

ADR-0014 shipped immutable-release asset distribution (EXE + SBOM), but the artifacts lack **integrity / authenticity / provenance**:

- Unsigned, so SmartScreen reports "unknown publisher" and users cannot verify the source.
- No checksums to detect tampered downloads.
- No machine-verifiable provenance (which workflow built from which commit).

Also, **immutable releases are ON** for this repo (ADR-0014, permanent owner policy). Published releases cannot be removed, so cutting a tag just to test signing is irreversible — there is no place to iterate on signing.

Sister project find-my-files already solved this; we port its setup (SSL.com eSigner / keyless attestation / `publish=false` signing smoke).

## Decision

**Add non-blocking Authenticode signing, SHA256SUMS, and keyless attestation to `release-assets.yml`, plus a signing-smoke path that does not pollute immutable releases.**

### Signing (Authenticode)

- Use **SSL.com eSigner + a personal IV certificate**. Provider rationale follows find-my-files ADR-0020:
  - Azure Artifact Signing is **not available to individuals resident in Japan** (US/CA/EU/UK only).
  - **EV no longer grants instant SmartScreen trust since 2024-03** (Microsoft change). This app ships no kernel driver, so EV has little benefit; **IV (issued under a personal name) suffices**.
  - eSigner does **unattended CI signing** via cloud HSM + TOTP. The existing find-my-files IV certificate is reusable.
- **Publishing requires a signature** (amended 2026-07-01): `HAVE_SIGNING` is derived from the presence of `ES_USERNAME` / `CREDENTIAL_ID`. On a `publish=true` run with no signing secrets, the workflow **hard-fails before creating the Release** ("require a signature before publishing" step) rather than shipping unsigned. Only a `publish=false` smoke test builds unsigned (and publishes nothing), emitting `::warning::`.
- Signs **only the single first-party PE `linerule.exe`** (ADR-0011 single-binary distribution): collect into a staging dir, `batch_sign` → write back from explicit `output_path` → **hard-verify** with `Get-AuthenticodeSignature` (never ship unsigned when signing was requested — fail loud, don't silently succeed).
- Signing lives in a **CI-only YAML step**, not xtask: it is secrets- and Action-bound CI-specific work, not the portable release-step logic xtask owns.

### Checksums and attestation

- Attach `SHA256SUMS.txt` (EXE + SBOM). Written as lowercase hex, bare filenames, LF line endings for `sha256sum -c` compatibility.
- Keyless Sigstore attestation (`actions/attest-build-provenance` + `actions/attest` for the SBOM — the latter replaced the deprecated `actions/attest-sbom`), signed with the workflow OIDC token so **no stored secret is needed**. Grant the job `id-token: write` / `attestations: write` (top-level lowered to `contents: read`, write limited to job scope). Attests the final signed bytes.

### Signing smoke without polluting immutable releases

- Add a **`publish` boolean (default true)** to `workflow_dispatch`; gate downstream steps with env `PUBLISH = (event != workflow_dispatch) || inputs.publish`.
- **Two-layer gating**: sign + verify are gated only on `HAVE_SIGNING` (independent of PUBLISH); checksum, draft, upload, attest, and publish are gated on `PUBLISH`. A `publish=true` run additionally hard-fails if `HAVE_SIGNING` is false (see amendment). Running with `tag=main` / `publish=false` stops after build → sign → verify, verifying real signing while creating **no Release, tag, or attestation**.
- No throwaway-tag workflow (immutable, cannot be removed).

### Out of scope

Public OpenSSF Scorecard and weekly cargo-audit workflows are **not added here** (cargo-audit/deny already run as CI gates). Adopt them under a separate ADR if needed.

## Consequences

| Item | Before (ADR-0014) | After (this ADR) |
|---|---|---|
| Signing | none | Authenticode (SSL.com eSigner / IV); required for `publish=true` (hard-fails unsigned), optional for `publish=false` smoke |
| Assets | EXE + SBOM | EXE + SBOM + **SHA256SUMS.txt** |
| Provenance | none | keyless build-provenance + SBOM attestation |
| Permissions | top-level `contents: write` | top-level `contents: read` + 3 job-scoped writes |
| Signing test venue | none (only by cutting a tag) | `workflow_dispatch publish=false` smoke, no immutable pollution |
| tag push behavior | draft→upload→publish | unchanged (+ signing/checksum/attest inserted) |

## Verification

- Static: `actionlint` on `release-assets.yml`. Confirm `attest-*` permissions and subject-path match, and SHA256SUMS LF endings.
- Signing smoke (no immutable pollution): run `workflow_dispatch` with `tag=main` / `publish=false`.
  - No secrets → signing skipped + `::warning::`, no Release created.
  - Secrets present → sign/verify green, `signed: ... CN=<name>`, no Release created.
  - No secrets + `publish=true` → hard-fails at "require a signature before publishing" (no unsigned release is ever cut).
- Real release: tag push → draft → upload EXE+SBOM+SHA256SUMS → attest → publish. Confirm 3 assets via `gh release view <tag>`, `gh attestation verify <exe> --repo P4suta/linerule-rs` succeeds, and `sha256sum -c SHA256SUMS.txt` matches.

## Open questions / Followup

- Public Scorecard / weekly audit adoption decided in a separate ADR.
- IV certificates last ~460 days max (CA/Browser Forum rules). Manage expiry via the renewal procedure in docs/SIGNING.md.
- Per-predicate-type `gh attestation verify` (CycloneDX) is documented in README/SUPPLY_CHAIN; revisit if requirements change.
