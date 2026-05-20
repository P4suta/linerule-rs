# Changelog

## [0.3.0](https://github.com/P4suta/linerule-rs/compare/v0.2.2...v0.3.0) (2026-05-20)


### ⚠ BREAKING CHANGES

* portable な exe-dir ログに戻し、dist-dev/PDB を撤廃 (Phase J slim-down) ([#58](https://github.com/P4suta/linerule-rs/issues/58))

### Features

* **app:** --initial-mode flag + slit smoke + commit/ShowWindow lint (Phase γ) ([#64](https://github.com/P4suta/linerule-rs/issues/64)) ([571d374](https://github.com/P4suta/linerule-rs/commit/571d374246dcfd71985eb6a307556bdf478598ea))
* **app:** layout-independent hotkeys + HUD help + opaque background + repeat (Phase ζ) ([#67](https://github.com/P4suta/linerule-rs/issues/67)) ([4ea9e11](https://github.com/P4suta/linerule-rs/commit/4ea9e1156892ad0db1f8049eec410aa2252f0265))


### Bug Fixes

* **app:** capture recent tracing events into crash dumps ([#52](https://github.com/P4suta/linerule-rs/issues/52)) ([b4871ef](https://github.com/P4suta/linerule-rs/commit/b4871ef3cf0f77fdcf1eb22eb6fed7f23382e9ee))
* **app:** diagnostics CLI flags + debug_assertions invariants + ADR-0009 ([#54](https://github.com/P4suta/linerule-rs/issues/54)) ([7aa335a](https://github.com/P4suta/linerule-rs/commit/7aa335a9618ab3e8b9c2eaa5392969134d7ab783))
* **app:** propagate run_id into tracing root span ([#49](https://github.com/P4suta/linerule-rs/issues/49)) ([b5e61a1](https://github.com/P4suta/linerule-rs/commit/b5e61a19f2a900912955aeb485cce1abcfd48ae3))
* **ci:** add debug-build job with PDB artifact upload (profile.dist-dev) ([#48](https://github.com/P4suta/linerule-rs/issues/48)) ([c29b7e4](https://github.com/P4suta/linerule-rs/commit/c29b7e4d01b6d50e2f600b7bb28ca5becdab25eb))
* **core:** introduce ErrorClass and AppError aggregator ([#50](https://github.com/P4suta/linerule-rs/issues/50)) ([3d73fce](https://github.com/P4suta/linerule-rs/commit/3d73fce6765235cbb49ace0f4b7a3c5b431ac94d))
* **core:** introduce HudNotification ADT for HUD toast / conflict display ([#53](https://github.com/P4suta/linerule-rs/issues/53)) ([cd66d2a](https://github.com/P4suta/linerule-rs/commit/cd66d2a49cad3a7fad011a91958a671f6508bbd8))
* **docs:** add README badges, Quick Links, and repo About sidebar metadata ([#55](https://github.com/P4suta/linerule-rs/issues/55)) ([9bf2b7a](https://github.com/P4suta/linerule-rs/commit/9bf2b7abb2253441dfa48c57392de8b0c1b1ee4a))
* **docs:** drop redundant explicit link target on ChordSpec ([#38](https://github.com/P4suta/linerule-rs/issues/38)) ([ec10a48](https://github.com/P4suta/linerule-rs/commit/ec10a48e03bde6953142711a748c7f93ac76c50a))
* **hooks:** document lefthook v2 pre-push @{push} skip behavior ([#51](https://github.com/P4suta/linerule-rs/issues/51)) ([fca6f95](https://github.com/P4suta/linerule-rs/commit/fca6f958996552cf06435f94d2130ddc1ba4cb8c))
* **phase-ef:** wire WM_HOTKEY + WM_APP_TICK to tick pipeline ([#40](https://github.com/P4suta/linerule-rs/issues/40)) ([4b3a11b](https://github.com/P4suta/linerule-rs/commit/4b3a11b88e5d8e7d47c6c45bbb51f4d8fbb5e9fd))
* **phase-g:** HUD ADT + rustdoc pre-push + refresh Hz (groundwork) ([#41](https://github.com/P4suta/linerule-rs/issues/41)) ([60b7e51](https://github.com/P4suta/linerule-rs/commit/60b7e5148d5341b796cacdd38a477e0ea4660d8f))
* **phase-g:** wire DWrite HUD rendering + pre-commit lint ([#42](https://github.com/P4suta/linerule-rs/issues/42)) ([41eb6ab](https://github.com/P4suta/linerule-rs/commit/41eb6abe943b2adc1895e0b321006788b2c51fad))
* **phase-h:** multi-monitor + DPI awareness + README polish ([#43](https://github.com/P4suta/linerule-rs/issues/43)) ([d0419e7](https://github.com/P4suta/linerule-rs/commit/d0419e77cdae3de5f3c54cbc7625271cbdd2efb8))
* **platform-windows:** ensure HUD commit + ShowWindow for DComp overlay visibility ([#60](https://github.com/P4suta/linerule-rs/issues/60)) ([bbf74a8](https://github.com/P4suta/linerule-rs/commit/bbf74a820c2462618f56939612f633f54beb526e))
* **platform-windows:** remove redundant D2D BeginDraw/EndDraw inside DComp surface session ([#59](https://github.com/P4suta/linerule-rs/issues/59)) ([fb40c25](https://github.com/P4suta/linerule-rs/commit/fb40c25abfeda88348c290cb71d5f21c0d2d0374))
* **platform-windows:** use ID2D1DeviceContext for DComp BeginDraw + enforce via clippy ([#57](https://github.com/P4suta/linerule-rs/issues/57)) ([89b3230](https://github.com/P4suta/linerule-rs/commit/89b323012d7b1a793a56fdad63ccf9aa6c783e6f))


### Code Refactoring

* portable な exe-dir ログに戻し、dist-dev/PDB を撤廃 (Phase J slim-down) ([#58](https://github.com/P4suta/linerule-rs/issues/58)) ([a2f4fbc](https://github.com/P4suta/linerule-rs/commit/a2f4fbc64c8bc950810a6cacb53c426b320888f1))


### Continuous Integration

* **app:** add --duration-ms auto-quit + Windows GUI smoke test (Phase α) ([#61](https://github.com/P4suta/linerule-rs/issues/61)) ([9ef5220](https://github.com/P4suta/linerule-rs/commit/9ef5220abc4ecdf190f52445cf53767a5e9c6a62))

## [0.2.2](https://github.com/P4suta/linerule-rs/compare/v0.2.1...v0.2.2) (2026-05-20)


### Bug Fixes

* **docs:** bump actions/deploy-pages v4.0.5 → v5.0.0 ([#25](https://github.com/P4suta/linerule-rs/issues/25)) ([4888407](https://github.com/P4suta/linerule-rs/commit/4888407772862786b1221af027226d786bd2d5ed))

## [0.2.1](https://github.com/P4suta/linerule-rs/compare/v0.2.0...v0.2.1) (2026-05-20)


### Bug Fixes

* **ci:** docs needs setup-mold; release-please workspace config ([#20](https://github.com/P4suta/linerule-rs/issues/20)) ([e749455](https://github.com/P4suta/linerule-rs/commit/e7494554661c4f18249ccef119692c5d8eaf83eb))
* **docs:** drop nightly-only rustdoc flags ([#22](https://github.com/P4suta/linerule-rs/issues/22)) ([3b22feb](https://github.com/P4suta/linerule-rs/commit/3b22febd5b4688668e25abdb88be7a475729fc28))


### Build System

* **deps:** bump cargo_metadata 0.18 → 0.23 + dependabot auto-merge ([#21](https://github.com/P4suta/linerule-rs/issues/21)) ([712873a](https://github.com/P4suta/linerule-rs/commit/712873a41b1c49486eaf0314439c78adc63f88ad))
* **deps:** Bump directories from 5.0.1 to 6.0.0 ([#18](https://github.com/P4suta/linerule-rs/issues/18)) ([f420408](https://github.com/P4suta/linerule-rs/commit/f420408779779acb9d37932d7723448ec04def56))
* **deps:** Bump docker/metadata-action from 5.8.0 to 6.0.0 ([#12](https://github.com/P4suta/linerule-rs/issues/12)) ([c0ee96b](https://github.com/P4suta/linerule-rs/commit/c0ee96b3cbfd3c8b1f2a97d57bb6dccc75476ca3))
* **deps:** Bump googleapis/release-please-action from 4.2.0 to 5.0.0 ([#14](https://github.com/P4suta/linerule-rs/issues/14)) ([35343d8](https://github.com/P4suta/linerule-rs/commit/35343d87b937a1fb5fb3ae4c291d2b214cc773a5))
* **deps:** Bump Swatinem/rust-cache ([#15](https://github.com/P4suta/linerule-rs/issues/15)) ([f8d7276](https://github.com/P4suta/linerule-rs/commit/f8d72766f0cc2d654eaaf6b9c0be3de6325a2cc0))
* **deps:** Bump the windows group across 1 directory with 3 updates ([#17](https://github.com/P4suta/linerule-rs/issues/17)) ([93d8637](https://github.com/P4suta/linerule-rs/commit/93d8637a3bd053fcfee6b860c1029e13f5b8e49b))
* **deps:** bump windows 0.60 → 0.62 + numerics 0.1 → 0.3 ([#24](https://github.com/P4suta/linerule-rs/issues/24)) ([3a1e540](https://github.com/P4suta/linerule-rs/commit/3a1e54004227192a21f455673ef94108488cf568))
