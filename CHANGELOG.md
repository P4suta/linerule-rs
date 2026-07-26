# Changelog

## [0.6.1](https://github.com/P4suta/linerule-rs/compare/v0.6.0...v0.6.1) (2026-07-26)


### Bug Fixes

* harden release workflow orchestration ([#176](https://github.com/P4suta/linerule-rs/issues/176)) ([3471573](https://github.com/P4suta/linerule-rs/commit/3471573063eb1ec85f72c015e95ef8d75fa88abc))

## [0.6.0](https://github.com/P4suta/linerule-rs/compare/v0.5.0...v0.6.0) (2026-07-26)


### Features

* complete linerule 0.6 release ([#169](https://github.com/P4suta/linerule-rs/issues/169)) ([a5aab83](https://github.com/P4suta/linerule-rs/commit/a5aab83e84bf3564a746a4b713507008eb7af5d8))


### Bug Fixes

* remove self-hosted release dependency ([#175](https://github.com/P4suta/linerule-rs/issues/175)) ([ee26332](https://github.com/P4suta/linerule-rs/commit/ee263320b202a179201cbf0216f2fd0bcda99749))


### Build System

* **deps:** Bump actions/attest from 4.1.1 to 4.2.0 ([#155](https://github.com/P4suta/linerule-rs/issues/155)) ([94013a1](https://github.com/P4suta/linerule-rs/commit/94013a155775a944bc43794e1335afe9871be081))
* **deps:** Bump actions/checkout from 7.0.0 to 7.0.1 ([#164](https://github.com/P4suta/linerule-rs/issues/164)) ([6d49bc8](https://github.com/P4suta/linerule-rs/commit/6d49bc8c545c5fd5aec4370809143383870df783))
* **deps:** Bump anyhow from 1.0.103 to 1.0.104 ([#159](https://github.com/P4suta/linerule-rs/issues/159)) ([5f6e397](https://github.com/P4suta/linerule-rs/commit/5f6e397e379eb88cb9f2a4b4920027c40bcf73cd))
* **deps:** Bump clap from 4.6.1 to 4.6.3 ([#167](https://github.com/P4suta/linerule-rs/issues/167)) ([d5a40ab](https://github.com/P4suta/linerule-rs/commit/d5a40ab3a6e1074d10804edf631d74910a96d8e3))
* **deps:** Bump crate-ci/typos from 1.47.2 to 1.48.0 ([#146](https://github.com/P4suta/linerule-rs/issues/146)) ([dc2a826](https://github.com/P4suta/linerule-rs/commit/dc2a82643de4b702e7fa602e06c2108bddec4067))
* **deps:** Bump docker/build-push-action from 7.2.0 to 7.3.0 ([#153](https://github.com/P4suta/linerule-rs/issues/153)) ([0d3a228](https://github.com/P4suta/linerule-rs/commit/0d3a22827b926e7437072258d6cc2878f1452bff))
* **deps:** Bump docker/login-action from 4.2.0 to 4.4.0 ([#148](https://github.com/P4suta/linerule-rs/issues/148)) ([7310b78](https://github.com/P4suta/linerule-rs/commit/7310b782c75268a78f0a51109bda8a2f674ea219))
* **deps:** Bump docker/login-action from 4.4.0 to 4.5.1 ([#171](https://github.com/P4suta/linerule-rs/issues/171)) ([cac79d5](https://github.com/P4suta/linerule-rs/commit/cac79d55a0d6d13f0d479982c05564fbfa93acf2))
* **deps:** Bump docker/metadata-action from 6.1.0 to 6.2.0 ([#147](https://github.com/P4suta/linerule-rs/issues/147)) ([ce61ec9](https://github.com/P4suta/linerule-rs/commit/ce61ec9cd3de9b1a9e4f7f91101101e4e21a3a19))
* **deps:** Bump docker/setup-buildx-action from 4.1.0 to 4.2.0 ([#151](https://github.com/P4suta/linerule-rs/issues/151)) ([d3fbfb0](https://github.com/P4suta/linerule-rs/commit/d3fbfb06fdefa8842f3c454a7e722ed36f14563b))
* **deps:** Bump ossf/scorecard-action from 2.4.3 to 2.4.4 ([#173](https://github.com/P4suta/linerule-rs/issues/173)) ([bce93c2](https://github.com/P4suta/linerule-rs/commit/bce93c254d57874013a50a650df90c9f47047bd2))
* **deps:** Bump serde_json from 1.0.150 to 1.0.151 in the serde group ([#165](https://github.com/P4suta/linerule-rs/issues/165)) ([006b1a9](https://github.com/P4suta/linerule-rs/commit/006b1a9b70410550d8272b1a8294840b3bac943e))
* **deps:** Bump taiki-e/install-action from 2.82.5 to 2.82.8 ([#150](https://github.com/P4suta/linerule-rs/issues/150)) ([30df437](https://github.com/P4suta/linerule-rs/commit/30df437044c11fab3b71bbcc6f9cf5ccecd69c6a))

## [0.5.0](https://github.com/P4suta/linerule-rs/compare/v0.4.1...v0.5.0) (2026-07-01)


### Features

* add dev/nightly/stable build channels with version stamping ([#136](https://github.com/P4suta/linerule-rs/issues/136)) ([86fd01b](https://github.com/P4suta/linerule-rs/commit/86fd01b946def85b788ace65f5878f7a32be58e6))
* add vibrancy (saturation + contrast) pass to the backdrop blur ([#112](https://github.com/P4suta/linerule-rs/issues/112)) ([40fafb5](https://github.com/P4suta/linerule-rs/commit/40fafb5b0e6ee97ae439b8cf1d943b74bbb47c15))
* adjustable blur amount with perceptual steps, drop tint-brightness knob ([#108](https://github.com/P4suta/linerule-rs/issues/108)) ([99f4988](https://github.com/P4suta/linerule-rs/commit/99f498825bce59bd9c94347ab0c94d66517785a0))
* **app:** embed an application icon in the executable ([#145](https://github.com/P4suta/linerule-rs/issues/145)) ([17fea06](https://github.com/P4suta/linerule-rs/commit/17fea0659fd2d75636c84001fe957e40c900b120))
* **dev:** add verify --scenario chord injection with state assertions ([#128](https://github.com/P4suta/linerule-rs/issues/128)) ([0d6329f](https://github.com/P4suta/linerule-rs/commit/0d6329f89b16747fb8bd2a02451665c622d195b6))
* **dev:** make native Windows a first-class dev path ([#127](https://github.com/P4suta/linerule-rs/issues/127)) ([aaad879](https://github.com/P4suta/linerule-rs/commit/aaad8795b55523fcf74a6b2ba3226bcd5bb393c9))
* make Blur a pure backdrop blur, drop the darkening tint ([#111](https://github.com/P4suta/linerule-rs/issues/111)) ([160b747](https://github.com/P4suta/linerule-rs/commit/160b74700482bdde0784b836bc4e4c10f23a7d9d))
* surround effect variations (white/blur) + WinRT composition backend ([#106](https://github.com/P4suta/linerule-rs/issues/106)) ([26d2cea](https://github.com/P4suta/linerule-rs/commit/26d2cea0e592a86d07f9b93edae8a6f41dc6dd54))
* two-tier HUD, transition channels, and a single on/off entry point ([#123](https://github.com/P4suta/linerule-rs/issues/123)) ([d4e4c64](https://github.com/P4suta/linerule-rs/commit/d4e4c645c1169a9cd9b669bddbf510dff08b20d1))
* WinRT-only composition backend with working backdrop blur ([#107](https://github.com/P4suta/linerule-rs/issues/107)) ([09fdac1](https://github.com/P4suta/linerule-rs/commit/09fdac198f633455109093b912bfe428f4c0a7e1))


### Bug Fixes

* **dev:** chown the node_modules volume so bun install works as dev ([#120](https://github.com/P4suta/linerule-rs/issues/120)) ([94796fd](https://github.com/P4suta/linerule-rs/commit/94796fdc8b5250902840c00e6fbe3209f0255ced))
* **renderer:** keep HUD above overlay dim via dedicated sub-roots ([#92](https://github.com/P4suta/linerule-rs/issues/92)) ([46c3035](https://github.com/P4suta/linerule-rs/commit/46c3035b70ebe7b9e08d6ef6ba65ee19cdf9bb11))


### Code Refactoring

* collapse hud_frame row boilerplate into a RowCursor ([#115](https://github.com/P4suta/linerule-rs/issues/115)) ([5eba98e](https://github.com/P4suta/linerule-rs/commit/5eba98e83087b1ce38da3e2d0e0ec114b1dcf08d))
* dedup map_hr and parse blur env once into BlurConfig ([#116](https://github.com/P4suta/linerule-rs/issues/116)) ([48cf381](https://github.com/P4suta/linerule-rs/commit/48cf3811f0130f1b0b0694d2d1037f40bec6f81c))
* **dev:** move node_modules from named volume to bind mount ([#121](https://github.com/P4suta/linerule-rs/issues/121)) ([a614c4c](https://github.com/P4suta/linerule-rs/commit/a614c4cabd33af125914addb5ef6dbb0d205176c))
* group OverlayWndState fields into Renderers and Hotkeys ([#117](https://github.com/P4suta/linerule-rs/issues/117)) ([933893b](https://github.com/P4suta/linerule-rs/commit/933893b5528c2113d2031d5565c5ba36d8c123d1))


### Documentation

* fix BlurAmount public→private intra-doc links breaking cargo doc ([#109](https://github.com/P4suta/linerule-rs/issues/109)) ([dc682f8](https://github.com/P4suta/linerule-rs/commit/dc682f8eb12b1ab8171494d32feb408bf0c167f8))
* generate doc artifacts, fix doc tooling, wire drift detection ([#114](https://github.com/P4suta/linerule-rs/issues/114)) ([9959c69](https://github.com/P4suta/linerule-rs/commit/9959c69de6f395ce812c010304a7ee1d939e0a46))
* slim comments/docs to terse English across the codebase ([#139](https://github.com/P4suta/linerule-rs/issues/139)) ([75f8a61](https://github.com/P4suta/linerule-rs/commit/75f8a61e436a48ffd346262ab5881a9ed3b86ae0))
* sweep linerule-core/linerule-app comments to terse English ([#118](https://github.com/P4suta/linerule-rs/issues/118)) ([456b279](https://github.com/P4suta/linerule-rs/commit/456b279ab6da35818258b77a04907e7529b1fdbe))
* sweep linerule-platform-windows comments to terse English ([#119](https://github.com/P4suta/linerule-rs/issues/119)) ([7669d90](https://github.com/P4suta/linerule-rs/commit/7669d90fee4515a078ee8ed14b1a39487e3a782d))


### Build System

* **deps:** Bump actions/cache from 5.0.5 to 6.1.0 ([#134](https://github.com/P4suta/linerule-rs/issues/134)) ([4c2ae72](https://github.com/P4suta/linerule-rs/commit/4c2ae7285757b0fc0be44325796de26f368f50bf))
* **deps:** Bump actions/checkout from 6.0.2 to 6.0.3 ([#105](https://github.com/P4suta/linerule-rs/issues/105)) ([49b5858](https://github.com/P4suta/linerule-rs/commit/49b5858e087a5d1ae4dcbf4bee2727cd9566ac9d))
* **deps:** Bump actions/checkout from 6.0.3 to 7.0.0 ([#130](https://github.com/P4suta/linerule-rs/issues/130)) ([cbb6c69](https://github.com/P4suta/linerule-rs/commit/cbb6c6989c13efe82584d7960178b202ffd4c851))
* **deps:** Bump bitflags from 2.11.1 to 2.13.0 ([#104](https://github.com/P4suta/linerule-rs/issues/104)) ([aebc8c2](https://github.com/P4suta/linerule-rs/commit/aebc8c211ce06752c1033893e08f0156d62abe7d))
* **deps:** Bump crate-ci/typos from 1.46.3 to 1.47.0 ([#94](https://github.com/P4suta/linerule-rs/issues/94)) ([33a2e9a](https://github.com/P4suta/linerule-rs/commit/33a2e9af9fb58333b2ee096c63d0fd080f8f455c))
* **deps:** Bump crate-ci/typos from 1.47.0 to 1.47.2 ([#103](https://github.com/P4suta/linerule-rs/issues/103)) ([b440e9f](https://github.com/P4suta/linerule-rs/commit/b440e9f621d0b1039bc0cef7462cb7fd6b1b555c))
* **deps:** Bump docker/login-action from 3.5.0 to 4.2.0 ([#86](https://github.com/P4suta/linerule-rs/issues/86)) ([2aa5f2b](https://github.com/P4suta/linerule-rs/commit/2aa5f2b580c8f6f4eaf7c0608f08c85eaecae050))
* **deps:** Bump docker/setup-buildx-action from 3.11.1 to 4.1.0 ([#95](https://github.com/P4suta/linerule-rs/issues/95)) ([157f4ee](https://github.com/P4suta/linerule-rs/commit/157f4eea88901a5f80f5abf72b9f1d36e8b4dc3b))
* **deps:** Bump insta from 1.47.2 to 1.48.0 ([#125](https://github.com/P4suta/linerule-rs/issues/125)) ([155388e](https://github.com/P4suta/linerule-rs/commit/155388eb846fe69426edd9810e1ef2aa6402380c))
* **deps:** Bump rust from 1.95-bookworm to 1.96-bookworm ([#93](https://github.com/P4suta/linerule-rs/issues/93)) ([da11a83](https://github.com/P4suta/linerule-rs/commit/da11a83b3c03c1fe56c97057bdcfe325a994b8bb))
* **deps:** Bump taiki-e/install-action from 2.79.2 to 2.79.6 ([#82](https://github.com/P4suta/linerule-rs/issues/82)) ([51372ae](https://github.com/P4suta/linerule-rs/commit/51372ae7471aef58c511fe16eabc8a3245751f3f))
* **deps:** Bump taiki-e/install-action from 2.79.6 to 2.81.2 ([#98](https://github.com/P4suta/linerule-rs/issues/98)) ([f7a1a10](https://github.com/P4suta/linerule-rs/commit/f7a1a10196981ff244efb411b7a33f139697ce0a))
* **deps:** Bump taiki-e/install-action from 2.81.2 to 2.81.7 ([#102](https://github.com/P4suta/linerule-rs/issues/102)) ([630cf6b](https://github.com/P4suta/linerule-rs/commit/630cf6b5ecf1a177332d21b0152486b720c3da01))
* **deps:** Bump taiki-e/install-action from 2.81.7 to 2.82.0 ([#124](https://github.com/P4suta/linerule-rs/issues/124)) ([503dfb8](https://github.com/P4suta/linerule-rs/commit/503dfb851f1a13c2c488fe7e215667058f1fa78c))
* **deps:** Bump taiki-e/install-action from 2.82.0 to 2.82.2 ([#129](https://github.com/P4suta/linerule-rs/issues/129)) ([b7c4f40](https://github.com/P4suta/linerule-rs/commit/b7c4f408bd2bbc27c1407ded87a9001bac584298))
* **deps:** Bump taiki-e/install-action from 2.82.2 to 2.82.5 ([#132](https://github.com/P4suta/linerule-rs/issues/132)) ([b394369](https://github.com/P4suta/linerule-rs/commit/b394369ac40394e075cfd924d8a7fad6b512b7ab))
* **deps:** Bump the commitlint group with 2 updates ([#97](https://github.com/P4suta/linerule-rs/issues/97)) ([aea28b9](https://github.com/P4suta/linerule-rs/commit/aea28b93c1c7227441e5ddacdec2144890fdacbf))
* **deps:** Bump uuid from 1.23.1 to 1.23.2 ([#99](https://github.com/P4suta/linerule-rs/issues/99)) ([52cd5e2](https://github.com/P4suta/linerule-rs/commit/52cd5e25db2dacd571651d888a335395efa75074))
* **deps:** Bump uuid from 1.23.2 to 1.23.3 ([#126](https://github.com/P4suta/linerule-rs/issues/126)) ([8dd9520](https://github.com/P4suta/linerule-rs/commit/8dd9520394c1ea694faf7f4d21b304c36c4440c3))
* **deps:** Bump uuid from 1.23.3 to 1.23.4 ([#135](https://github.com/P4suta/linerule-rs/issues/135)) ([3d8cbeb](https://github.com/P4suta/linerule-rs/commit/3d8cbebedc8c2b540cd932617da13581b11a356b))


### Continuous Integration

* codify branch & tag protection as rulesets + ci-required gate ([#137](https://github.com/P4suta/linerule-rs/issues/137)) ([da8dc01](https://github.com/P4suta/linerule-rs/commit/da8dc0195d704eeeffde9888d2a2e8c95a8cbfcc))
* enable merge queue and auto-merge all dependabot bumps ([#100](https://github.com/P4suta/linerule-rs/issues/100)) ([bdd4322](https://github.com/P4suta/linerule-rs/commit/bdd432289f1a777396cf13f22d925c78810aacdc))
* gate cargo doc -D warnings on PRs ([#110](https://github.com/P4suta/linerule-rs/issues/110)) ([e768519](https://github.com/P4suta/linerule-rs/commit/e7685197e9da9a0de4682ce307d388323a03a8ca))
* gate release-assets behind release environment ([#131](https://github.com/P4suta/linerule-rs/issues/131)) ([1d362f6](https://github.com/P4suta/linerule-rs/commit/1d362f6395b3276a48542d2959add877ad93e65e))
* **release:** authenticate release-please as a GitHub App ([#141](https://github.com/P4suta/linerule-rs/issues/141)) ([94618e2](https://github.com/P4suta/linerule-rs/commit/94618e2c547192e22e81fde1c9c04b8a03c1e94e))
* **release:** Authenticode signing + attestation + governance docs ([#140](https://github.com/P4suta/linerule-rs/issues/140)) ([072ba5b](https://github.com/P4suta/linerule-rs/commit/072ba5b130679cf8eb1d1bd833d2456363bb3c30))
* **release:** backport find-my-files release-please fixes ([#144](https://github.com/P4suta/linerule-rs/issues/144)) ([3f754b7](https://github.com/P4suta/linerule-rs/commit/3f754b748c6530dad44bd9f5b3bf7f00ea78c503))
* restore --squash to dependabot auto-merge (no merge queue available) ([#101](https://github.com/P4suta/linerule-rs/issues/101)) ([7094b1c](https://github.com/P4suta/linerule-rs/commit/7094b1c75d90db80e185aea6761d669671eee080))
* **typos:** exclude generated CHANGELOG.md from spell check ([#143](https://github.com/P4suta/linerule-rs/issues/143)) ([31ece23](https://github.com/P4suta/linerule-rs/commit/31ece23278f47aecc942a79d6747f8e90f2006c5))

## [0.4.1](https://github.com/P4suta/linerule-rs/compare/v0.4.0...v0.4.1) (2026-05-24)


### Documentation

* **mutation:** bump baseline to 288 caught ([#45](https://github.com/P4suta/linerule-rs/issues/45) device-lost helpers) ([#88](https://github.com/P4suta/linerule-rs/issues/88)) ([8d1db86](https://github.com/P4suta/linerule-rs/commit/8d1db867990fb6958d743c81a1de8c99a413b17f))


### Build System

* **deps:** Bump serde_json from 1.0.149 to 1.0.150 in the serde group across 1 directory ([#84](https://github.com/P4suta/linerule-rs/issues/84)) ([94cec8d](https://github.com/P4suta/linerule-rs/commit/94cec8d51cddea49e2da4ffc4da5edf70427d9ee))

## [0.4.0](https://github.com/P4suta/linerule-rs/compare/v0.3.0...v0.4.0) (2026-05-24)


### Features

* **platform-windows:** follow active monitor for HUD panel placement ([#46](https://github.com/P4suta/linerule-rs/issues/46)) ([#78](https://github.com/P4suta/linerule-rs/issues/78)) ([382518d](https://github.com/P4suta/linerule-rs/commit/382518d85220bdeeedbd650d2cbd55ac5221e117))
* **platform-windows:** handle WM_DPICHANGED for runtime DPI switch ([#44](https://github.com/P4suta/linerule-rs/issues/44)) ([#79](https://github.com/P4suta/linerule-rs/issues/79)) ([2044a35](https://github.com/P4suta/linerule-rs/commit/2044a3567097201c3c2e2ead27249384892cfd86))
* **platform-windows:** rebuild renderer pipeline on device-lost HRESULT ([#45](https://github.com/P4suta/linerule-rs/issues/45)) ([#80](https://github.com/P4suta/linerule-rs/issues/80)) ([ad0f61d](https://github.com/P4suta/linerule-rs/commit/ad0f61d5a1a6be3a515551106ca38d3d9eb48a54))
* **platform-windows:** wire HUD opacity via IDCompositionVisual3::SetOpacity2 ([#47](https://github.com/P4suta/linerule-rs/issues/47)) ([#76](https://github.com/P4suta/linerule-rs/issues/76)) ([c3a82e1](https://github.com/P4suta/linerule-rs/commit/c3a82e1b82cd911888678541a6155abd28729bee))

## [0.3.0](https://github.com/P4suta/linerule-rs/compare/v0.2.2...v0.3.0) (2026-05-24)


### ⚠ BREAKING CHANGES

* portable な exe-dir ログに戻し、dist-dev/PDB を撤廃 (Phase J slim-down) ([#58](https://github.com/P4suta/linerule-rs/issues/58))

### Features

* **app:** --initial-mode flag + slit smoke + commit/ShowWindow lint (Phase γ) ([#64](https://github.com/P4suta/linerule-rs/issues/64)) ([571d374](https://github.com/P4suta/linerule-rs/commit/571d374246dcfd71985eb6a307556bdf478598ea))
* **app:** layout-independent hotkeys + HUD help + opaque background + repeat (Phase ζ) ([#67](https://github.com/P4suta/linerule-rs/issues/67)) ([4ea9e11](https://github.com/P4suta/linerule-rs/commit/4ea9e1156892ad0db1f8049eec410aa2252f0265))
* **app:** Phase H PR-E — consume AppError::class() via HUD notification (ADR-0013) ([#72](https://github.com/P4suta/linerule-rs/issues/72)) ([b65885a](https://github.com/P4suta/linerule-rs/commit/b65885a3edae4734b0f461b7d8817f8b47624224))
* **core:** mode-aware indicator bar (18×4 / 4×18, cs parity) ([#68](https://github.com/P4suta/linerule-rs/issues/68)) ([5b5168e](https://github.com/P4suta/linerule-rs/commit/5b5168e4bb888861a8c3977d9e0cb918046947eb))
* **platform-windows, core:** HUD telemetry pipeline (p99 / drops / stalls) ([#71](https://github.com/P4suta/linerule-rs/issues/71)) ([34d614f](https://github.com/P4suta/linerule-rs/commit/34d614f836b445dd6198e01809d4b96c14e9c6a8))
* **platform-windows:** re-assert HWND_TOPMOST on foreground change (ADR-0012) ([#70](https://github.com/P4suta/linerule-rs/issues/70)) ([443d68d](https://github.com/P4suta/linerule-rs/commit/443d68dca38cfdf395cb109e856ecb838e0af3b7))


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


### Documentation

* **mutation:** bump baseline to 283 caught (cs-port residual cleanup) ([#75](https://github.com/P4suta/linerule-rs/issues/75)) ([68b99e4](https://github.com/P4suta/linerule-rs/commit/68b99e414635c3e5fa57b59da4491e766842f59e))
* **roadmap:** draft Phase η plan after cs-port residual cleanup ([#74](https://github.com/P4suta/linerule-rs/issues/74)) ([bc52049](https://github.com/P4suta/linerule-rs/commit/bc52049b92475e729b331dac7b8ee096e1eeb7b3))


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
