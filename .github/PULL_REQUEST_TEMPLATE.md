## Summary

<!-- What does this change, and why? -->

## Checklist

- [ ] PR title follows [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `perf:`, `docs:`, …) — squash-merge uses it as the commit and feeds the release notes
- [ ] `just lint` and `just test` pass
- [ ] If you have a Windows host: `just verify` passes (GUI smoke)
- [ ] If the CLI surface / module structure / dependency graph changed: ran `just docs` and committed the regenerated output
- [ ] No hand-edits to generated artifacts (the `cargo-rdme` block in `README.md`, `docs/modules/`, `docs/dep-graph.svg`)
- [ ] Architectural change? Added or updated the relevant ADR in `docs/adr/`

See [CONTRIBUTING.md](../CONTRIBUTING.md).
