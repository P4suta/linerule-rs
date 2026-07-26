# Security Policy

## Reporting a vulnerability

Please report security vulnerabilities **privately** via
[GitHub Security Advisories](https://github.com/P4suta/linerule-rs/security/advisories/new).
**Do not open a public issue for a vulnerability.**

We aim to acknowledge a report within a few days and to ship a fix or mitigation
as quickly as the severity warrants.

## Supported versions

linerule-rs is pre-1.0; only the latest release receives security fixes.

| Version | Supported |
| ------- | --------- |
| latest  | ✅        |
| older   | ❌        |

## Scope

linerule-rs is a local-only desktop overlay. It performs no application network
I/O and opens no listening sockets. It reads its versioned settings and writes
bounded diagnostics under MSIX LocalState or the portable `data/` directory.

In-scope examples:

- Memory-safety issues reachable through the `unsafe` Win32 / COM FFI in
  `linerule-platform-windows` (enforced by the mise-pinned `just policy` gate).
- A crash or panic path that can be triggered by an unprivileged caller in a way
  that is not already a documented, accepted behavior.

Out of scope:

- Attacks that require a local attacker who already controls the same interactive
  desktop session (they can already do anything the user can).
- The deliberate, documented local settings and diagnostic behavior.
