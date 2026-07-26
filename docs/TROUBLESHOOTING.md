# Troubleshooting

Keep this file limited to shipped behavior.

## A shortcut does not work

Open Shortcut settings… from the tray. linerule highlights duplicate,
unparsable, modifierless, and externally occupied assignments. Saving is
transactional: a failed registration leaves the previous set active.

## Blur is unavailable

linerule tries the hardware D3D device first and then WARP. If backdrop blur
alone is unavailable, the ruler temporarily uses Dim and reports the
degradation. After repeated device loss, hide and show the ruler to retry the
graphics pipeline. There are no environment-variable rendering overrides.

## Settings were reset

A malformed settings file is renamed with a timestamp and defaults are loaded.
linerule reports the quarantine location. A settings file with a newer schema
is preserved byte-for-byte and opened read-only.

Use `linerule diagnostics --data-dir` to locate settings, logs, and crash
reports. Logs are retained for seven days and only the five newest crash reports
are kept.

## Report a defect

Include `linerule version`, the relevant diagnostic output, Windows 11 version,
CPU architecture, display scale/layout, and whether the renderer reported WARP
or a degraded effect. Do not attach private screen contents or secrets.
