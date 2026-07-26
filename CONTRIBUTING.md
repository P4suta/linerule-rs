# Contributing

linerule is a Windows 11 reading ruler. Keep changes within that scope and
preserve the dependency direction:

```text
linerule-app → linerule-platform-windows → linerule-core
```

## Setup

```text
mise install
mise exec just --command "just bootstrap"
```

Use native Windows for rendering and UI Automation work.

## Required checks

```text
mise exec just --command "just test lint test-cargo policy"
```

Before a stable tag, run
`mise exec just --command "just release-check --artifacts dist"`. Hardware, UI
Automation, install/update, signing, and ARM64 results must also be present; a
local compile is not release evidence.

Do not introduce test serialization, ignored errors, production
`unwrap`/`expect`/`panic!`, or unsafe code outside `win32_ffi`.

Use Conventional Commit titles. Pull requests are squash-merged. Security
reports belong in the private channel described by
[.github/SECURITY.md](.github/SECURITY.md).

Contributions are licensed under MIT or Apache-2.0, at your option.
