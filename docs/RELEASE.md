# Release

This is the only release runbook. Stable releases target Windows 11 x64 and
ARM64 and are blocked unless every automated gate and hardware evidence item
passes.

## Required assets

```text
linerule.msixbundle
linerule.appinstaller
linerule-portable-x64.zip
linerule-portable-arm64.zip
linerule-sbom.cdx.json
linerule-source.spdx
SHA256SUMS.txt
```

Nightly contains unsigned x64/ARM64 Portable files and a bundle with the separate
`P4suta.linerule.Nightly` identity. Stable App Installer assets use
`releases/latest/download/...` names.

## One-time secrets

The `release-please` environment contains
`RELEASE_PLEASE_CLIENT_ID` and `RELEASE_PLEASE_APP_PRIVATE_KEY`.
The approval-protected `release` environment contains the SSL.com eSigner
values `ES_USERNAME`, `ES_PASSWORD`, `CREDENTIAL_ID`, and `ES_TOTP_SECRET`.

The GitHub App needs Contents and Pull requests read/write access. Its token is
required so release PR and tag events trigger downstream workflows.

## Cut a release

1. Merge the release-please PR after `ci-required` succeeds.
2. On the resulting tag commit, dispatch `hardware-validation.yml` from an
   interactive self-hosted Windows 11 x64 and ARM64 runner pair labeled
   `linerule-hardware`. Both refuse fewer than two monitors or identical
   effective DPI values.
3. Run or rerun `release-assets.yml` for the tag, then review its native ARM64,
   UIA, install/update, GPU/WARP, mixed-DPI, and High Contrast evidence.
4. Approve the `release` environment.
5. The workflow requires both `ci-required` and `hardware-required`, runs the
   mise-pinned release check, signs each PE, builds and
   signs the MSIX bundle, verifies both signatures, generates SBOMs and
   checksums, uploads everything to a draft, adds provenance, and only then
   publishes the immutable release.

Publishing without valid PE and bundle signatures is forbidden. To test cloud
signing, dispatch the workflow with `tag=main` and `publish=false`; do not create
throwaway immutable tags.

## Verify an asset

```powershell
Get-AuthenticodeSignature .\linerule.exe
Get-AuthenticodeSignature .\linerule.msixbundle
Get-FileHash -Algorithm SHA256 .\linerule.msixbundle
mise exec aqua:cli/cli -- gh attestation verify .\linerule.msixbundle --repo P4suta/linerule-rs
```

Both signature statuses must be `Valid`, checksums must match
`SHA256SUMS.txt`, and the attestation must resolve to the expected tag and
workflow.
