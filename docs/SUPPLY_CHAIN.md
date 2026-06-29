# Supply Chain and Provenance

Lets users mechanically verify that a distributed binary was built by untampered CI from a specific commit of this repo. For code signing (Authenticode) see [SIGNING.md](SIGNING.md); for design decisions see [ADR-0017](adr/0017-release-signing-and-attestation.md).

## For users: verifying a download

`release-assets.yml` (tag-driven) issues GitHub-native keyless attestation. There is **no private key**; the workflow's OIDC token signs via Sigstore (Fulcio/Rekor). Verification needs only `gh`.

```bash
# 1) Checksum (tamper detection)
sha256sum -c SHA256SUMS.txt
#   Windows:
#   (Get-FileHash -Algorithm SHA256 linerule-vX.Y.Z-win-x64.exe).Hash

# 2) Build provenance (commit / workflow / runner)
gh attestation verify linerule-vX.Y.Z-win-x64.exe --repo P4suta/linerule-rs

# 3) SBOM bound to the same EXE (CycloneDX predicate)
gh attestation verify linerule-vX.Y.Z-win-x64.exe --repo P4suta/linerule-rs \
  --predicate-type https://cyclonedx.org/bom

# 4) Authenticode signature (when signing is enabled) — Windows
#   signtool verify /pa /v linerule-vX.Y.Z-win-x64.exe
#   Get-AuthenticodeSignature linerule-vX.Y.Z-win-x64.exe
```

Success means the artifact digest matches an attestation issued by `release-assets.yml` in `P4suta/linerule-rs`. Release assets:

| Asset | Contents |
|---|---|
| `linerule-vX.Y.Z-win-x64.exe` | Single native EXE (Authenticode-signed when secrets are set) |
| `linerule-vX.Y.Z-sbom.cdx.json` | SBOM (CycloneDX 1.6, `cargo-sbom`, `linerule-app` dependency closure) |
| `SHA256SUMS.txt` | SHA-256 of the EXE and SBOM |

The EXE and `SHA256SUMS.txt` carry a build-provenance attestation; the SBOM carries an SBOM attestation (shown in the repo **Attestations** tab).

## Dependency and build management

| Concern | Mechanism |
|---|---|
| Dependency lock | `Cargo.lock` (committed). Release sets `CARGO_NET_LOCKED=true` to forbid silent re-resolve |
| Vulnerabilities | `cargo-audit` (RustSec) / `cargo-deny` (advisories), gated in CI (`ci.yml` / `just lint`) |
| License / source | `cargo-deny` (bans / licenses / sources; unknown registries denied) |
| Auto-update | Dependabot (cargo / github-actions / npm / docker, weekly) |
| Action pinning | Third-party actions pinned to **40-char commit SHA** (+ `# vX.Y.Z` comment); Dependabot updates SHA and comment; `actionlint` verifies |
| Reproducibility | Rust is deterministic by default. Shipping profile in `Cargo.toml [profile.release]` (lto=fat / panic=abort / strip / codegen-units=1) |

## For maintainers: first attested release runbook

Attestation/SBOM steps fire only on publish. **Dry-run the OIDC/permission path before a real tag:**

1. Run `workflow_dispatch` with `tag=main` / `publish=false`; confirm build → (sign) → verify is green (no release/tag/attestation created; doubles as the signing smoke test).
2. For production, merge the release-please PR → push tag → `release-assets.yml` auto draft → upload → attest → publish.
3. Confirm `gh attestation verify <exe> --repo P4suta/linerule-rs` succeeds, the **Attestations** tab shows provenance + SBOM entries, and the release has all three of EXE / SBOM / `SHA256SUMS.txt`.

### Notes

- **SBOM tooling is CI/release only** (not in the `mise.toml` dev loop). `cargo install cargo-sbom` is version-pinned at release time. Format: **CycloneDX 1.6**.
- Public OpenSSF Scorecard and a weekly cargo-audit workflow are **not yet adopted** (cargo-audit/deny already run as CI gates). Adopting them is a separate ADR decision.
