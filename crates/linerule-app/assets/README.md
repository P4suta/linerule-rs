# App icon assets

- `linerule.png` — source artwork, 1024×1024 RGBA.
- `linerule.ico` — multi-resolution icon (16/24/32/48/64/128/256 px) generated
  from `linerule.png`, embedded into `linerule.exe` via `app.rc` + `build.rs`.

The artwork is the "ruler" glyph from [Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons)
(Microsoft, MIT licensed — compatible with this project's MIT OR Apache-2.0).

Regenerate the `.ico` after changing the source:

```sh
magick assets/linerule.png -background none \
  -define icon:auto-resize=256,128,64,48,32,24,16 assets/linerule.ico
```
