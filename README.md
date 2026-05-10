# linerule

A digital reading ruler — frameless, transparent, click-through,
always-on-top desktop overlay that follows the cursor. Designed for
extended reading sessions in any application: Kindle for PC, browser
e-readers, PDF viewers, 青空文庫 vertical text.

> Status: pre-0.1.0 — under active development. The MVP targets
> Windows 10/11; macOS / Linux are slated for v0.2+.

## Modes

- **Bar** — single horizontal translucent bar follows the cursor Y.
- **Mask** (typoscope) — top and bottom are dimmed; only a horizontal
  slit at the cursor's Y is unmasked.
- **Vertical** — rotated 90° for 縦書き / 青空文庫 reading.

Toggle with `Ctrl+Alt+R` (cycle), `Ctrl+Alt+H` (visible toggle),
`Ctrl+Alt+[` `]` (thickness), `Ctrl+Alt+-` `=` (opacity).

## Build

Every cargo invocation runs in Docker (ADR-0005). The host needs only
Docker, `just`, and git.

```sh
just build           # debug build
just test            # nextest
just lint            # fmt-check + clippy + typos + strict-code + shear
just coverage        # cargo-llvm-cov, fail under 100% branches
just build-windows   # cross-compile to x86_64-pc-windows-msvc via cargo-xwin
just hooks           # install lefthook git hooks
```

## License

Dual-licensed under [Apache-2.0](LICENSE-APACHE) and [MIT](LICENSE-MIT)
at the user's choice.
