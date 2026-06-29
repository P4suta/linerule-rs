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

linerule-rs is a local-only desktop overlay. It performs no network I/O, opens no
listening sockets, and reads no user files; it renders a click-through overlay and
writes its own diagnostic logs next to the executable. The attack surface is
therefore small.

In-scope examples:

- Memory-safety issues reachable through the `unsafe` Win32 / COM FFI in
  `linerule-platform-windows` (the `unsafe` is localized per ADR-0003).
- A crash or panic path that can be triggered by an unprivileged caller in a way
  that is not already a documented, accepted behavior.

Out of scope:

- Attacks that require a local attacker who already controls the same interactive
  desktop session (they can already do anything the user can).
- The deliberate, documented behaviors in the README (e.g. logs are written next
  to the portable executable).
