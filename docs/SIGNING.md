# Code Signing — Authenticode for the distributed EXE

Runbook for Authenticode-signing `linerule.exe` via **SSL.com eSigner** (cloud HSM signing).
Rationale and rejected alternatives: [ADR-0017](adr/0017-release-signing-and-attestation.md).

## Status

Signing is **non-blocking**. With the four eSigner secrets set, `.github/workflows/release-assets.yml`
signs each `vX.Y.Z` tag's binary. Without them, the tag ships **unsigned with a `::warning::`** and the
workflow still succeeds. Signing can be toggled without CI changes.

Only the **first-party PE `linerule.exe`** is signed (single-binary distribution, ADR-0011).

## Background (why this setup)

- **Individuals in Japan** cannot apply for the individual tier of Azure Artifact Signing (formerly Trusted
  Signing) — US/CA/EU/UK only.
- **Since 2024-03, even EV no longer grants instant SmartScreen trust** (Microsoft policy change). This app
  ships no kernel driver, so EV adds little. Individual Validation (IV), obtainable under a personal name,
  suffices.
- SmartScreen is reputation-based. Signing **may still warn on first runs** until download history builds;
  its immediate effect is replacing "unknown publisher" with the author's **name** in file properties.

## Enable (also for renewal/reissue)

### A. Get the certificate (SSL.com)

1. Create an account at [SSL.com](https://www.ssl.com/).
2. Buy a **Code Signing** certificate: **Individual Validation (IV) with eSigner (cloud signing)**, not the
   USB-token variant. ~$130–250/year.

> The same IV certificate (same author) already obtained for find-my-files can be reused. Skip new
> acquisition and just register the four secrets from D in this repo.

### B. Identity check (IV validation)

3. Government ID + identity verification (documents/video). No company registration required. Confirmed
   obtainable by Japanese individuals/sole proprietors.

### C. Configure eSigner for automated signing

4. From the SSL.com dashboard, record:
   - the certificate's **Credential ID**
   - the automated-signing **TOTP (2FA) secret** (Base32)
   - account **username / password**

### D. Add four GitHub Secrets

5. Repo → Settings → Secrets and variables → Actions → New repository secret:

   | Secret | Value |
   |---|---|
   | `ES_USERNAME` | SSL.com username |
   | `ES_PASSWORD` | SSL.com password |
   | `CREDENTIAL_ID` | certificate Credential ID |
   | `ES_TOTP_SECRET` | eSigner automated-signing TOTP secret (Base32) |

   The next `vX.Y.Z` tag (or workflow_dispatch) sets `HAVE_SIGNING` to `true` and signing runs.

## Signing smoke test (never touch immutable — critical)

This repo has GitHub **immutable releases ON** (ADR-0014). A published tag cannot be removed, so verify only
via **`workflow_dispatch` with `tag=main` / `publish=false`**. That path runs build → sign → verify and
stops — no Release, tag, or attestation.

- **Without secrets**: confirm the signing step is skipped, emits `::warning::`, and the workflow does not
  fail (non-blocking wiring).
- **With secrets**: confirm "sign staged binary" and "verify signature" pass and
  `signed: linerule-main-win-x64.exe - CN=<name>` prints (real-signature check). `publish=false` keeps
  immutable clean.

> Do not cut throwaway tags — immutable cannot remove a published tag. The `publish=false` smoke covers
> signature verification.

## Local check

Get a real release EXE and on Windows:

```powershell
signtool verify /pa /v linerule-vX.Y.Z-win-x64.exe        # → Successfully verified
Get-AuthenticodeSignature linerule-vX.Y.Z-win-x64.exe      # → Status: Valid
```

EXE properties → "Digital Signatures" tab shows the name and timestamp.

## Renewal (expiry)

- Per CA/Browser Forum rules, publicly-trusted code-signing certs last **~460 days (~15 months) max**.
  Renew at SSL.com before expiry.
- Update the corresponding secret **only if Credential ID / TOTP changes** on renewal.

## Troubleshooting

- **`hash needs to be scanned first before submitting for signing`**: the SSL.com pre-signing malware
  blocker forbids `batch_sign` on an unscanned hash. `release-assets.yml` is wired with
  `malware_block: "true"` for inline scan.
- **verify reports `NotSigned`**: `batch_sign` ignores `override`, so signed files must be written to an
  explicit `output_path` (else copy-back picks up the unsigned original). Already wired.
- **`Get-AuthenticodeSignature` returns `UnknownError`**: public-trust chain unresolved. Inspect via
  `signtool verify /pa`.
- **SmartScreen warning on first launch**: expected (shallow reputation); clears as downloads accumulate.

## See also

- [ADR-0017 — Release signing and attestation](adr/0017-release-signing-and-attestation.md)
- [SUPPLY_CHAIN.md](SUPPLY_CHAIN.md)
- Implementation: `.github/workflows/release-assets.yml`
