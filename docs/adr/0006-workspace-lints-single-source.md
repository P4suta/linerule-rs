# 0006. `[workspace.lints]` is the single source of truth; per-crate `[lints.clippy]` overrides forbidden

- Status: accepted
- Date: 2026-05-10
- Deciders: @P4suta
- Tags: linting, infra

## Context

Cargo's lint inheritance has a known sharp edge
([rust-lang/cargo#12697][cargo-12697]): a crate cannot combine
`[lints]workspace = true` with a per-crate `[lints.clippy]` table to
*also* set carve-outs. Mixing the two silently overrides the
workspace-level configuration in subtle ways.

Additionally, passing `-W clippy::<group>` on the CLI re-enables the
*entire* group at command-line priority, which overrides per-lint
allow carve-outs in `[workspace.lints.clippy]`. This is the
single-most-common way teams accidentally undo their own lint policy.

[cargo-12697]: https://github.com/rust-lang/cargo/issues/12697

## Decision

- `[workspace.lints.{rust, rustdoc, clippy}]` in the workspace
  `Cargo.toml` is the **single source of truth** for every lint and
  carve-out. Each crate enables it via:

  ```toml
  [lints]
  workspace = true
  ```

  No per-crate `[lints.clippy]` overrides. No per-crate `[lints.rust]`
  overrides.

- The Justfile's `clippy` recipe passes only `-D warnings` on the CLI
  surface — never `-W clippy::pedantic` etc. The lint group enablement
  lives in `[workspace.lints.clippy]` with explicit
  `priority = -1` so the carve-outs below override individual lints.

- When a single crate genuinely needs an exception (e.g. `unsafe` in
  `linerule-platform`), it goes through the strict-code Rust xtask
  (`cargo run -p xtask -- strict-code`, source in
  `crates/xtask/src/strict_code.rs`), *not* through `#[allow(...)]`
  and *not* through per-crate `[lints]` overrides. The gate naturally
  scopes by path, is fully type-checked, and is auditable.

- `#[allow(...)]` attributes in source code are forbidden by
  `scripts/strict-code.sh`. When a lint genuinely needs to be
  silenced inline, use `#[expect(lint_name, reason = "...")]` (Rust
  1.81+) — which self-removes when the underlying issue is fixed.

## Consequences

**Becomes easier**
- One file owns the entire lint policy (`Cargo.toml`).
- A code reviewer can answer "is X enabled?" with a single grep.
- Carve-outs are visible to everyone — there are no per-crate hidden
  override that quietly differs.

**Becomes harder**
- Genuine per-crate exceptions take more care: either the
  `strict-code` grep gets a path-scoped exception, or the call site
  uses `#[expect(... reason = "...")]` (which the grep allows).

## Alternatives considered

- **Per-crate `[lints.clippy]` for carve-outs** — silently overrides
  the workspace-level group enablement (cargo#12697). Rejected.
- **CLI `-W clippy::<group>` instead of `[workspace.lints]`** —
  overrides per-lint allow carve-outs. Rejected.
- **Looser policy + reviewer discipline** — invariably erodes; the
  carve-outs accumulate. Rejected per
  `feedback_no_warning_suppression` and
  `feedback_workspace_lints_single_source_of_truth`.

## References

- Plan file: `/home/yasunobu/.claude/plans/velvet-finding-hennessy.md`
- `Cargo.toml` `[workspace.lints]`
- `scripts/strict-code.sh`
- [rust-lang/cargo#12697](https://github.com/rust-lang/cargo/issues/12697)
