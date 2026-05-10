# syntax=docker/dockerfile:1.7
# linerule dev / CI container.
# Every cargo / clippy / nextest invocation runs in this image (ADR-0005).
#
# Build-time speed: the cargo-tools layer uses cargo-binstall, which
# fetches precompiled binaries from GitHub releases. The legacy
# `cargo install` path took ~15 min from cold; binstall is ~2 min.
# See ADR-0008 for the full CI-speed strategy.

ARG RUST_VERSION=1.95.0

########################################################################
# toolchain — Rust stable + system deps + sccache + mold
########################################################################
FROM rust:${RUST_VERSION}-bookworm AS toolchain

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && \
    apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
        libxcb1-dev \
        libxkbcommon-dev \
        libwayland-dev \
        clang \
        mold \
        curl \
        git \
        ca-certificates \
        unzip \
        xz-utils \
        locales \
    && sed -i -e 's/# \(en_US.UTF-8 UTF-8\)/\1/' /etc/locale.gen \
    && sed -i -e 's/# \(ja_JP.UTF-8 UTF-8\)/\1/' /etc/locale.gen \
    && locale-gen

ENV LANG=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8 \
    RUSTUP_PERMIT_COPY_RENAME=1

# mold linker for fast Linux builds (matches .cargo/config.toml).
RUN mkdir -p /root/.cargo && printf '%s\n' \
    '[target.x86_64-unknown-linux-gnu]' \
    'linker = "clang"' \
    'rustflags = ["-C", "link-arg=-fuse-ld=mold"]' \
    > /root/.cargo/config.toml

# cargo-binstall — precompiled-binary installer that drops the
# tool-install layer time by an order of magnitude. Install via
# upstream's official script.
RUN curl -fsSL https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

########################################################################
# cargo-tools — Rust dev utilities, all binstalled
########################################################################
FROM toolchain AS cargo-tools

# Bulk binstall — order chosen so cargo-deny / cargo-audit (heaviest
# transitive trees) come first; if any single tool fails, binstall
# falls back to source `cargo install` for *that one* tool only.
RUN cargo binstall --no-confirm --locked --no-symlinks \
        cargo-nextest \
        cargo-llvm-cov \
        cargo-deny \
        cargo-audit \
        cargo-shear \
        cargo-semver-checks \
        cargo-insta \
        cargo-dist \
        cargo-xwin \
        cargo-edit \
        committed \
        typos-cli \
        bacon \
        release-plz \
        git-cliff \
        cargo-bolero \
        sccache

# just (task runner) — upstream provides an install script.
ARG JUST_VERSION=1.50.0
RUN curl -fsSL https://just.systems/install.sh | bash -s -- --to /usr/local/bin --tag ${JUST_VERSION}

# lefthook (pre-commit manager).
ARG LEFTHOOK_VERSION=2.1.6
RUN curl -fsSL \
    "https://github.com/evilmartians/lefthook/releases/download/v${LEFTHOOK_VERSION}/lefthook_${LEFTHOOK_VERSION}_Linux_x86_64.gz" \
    | gunzip > /usr/local/bin/lefthook \
    && chmod +x /usr/local/bin/lefthook

########################################################################
# dev — interactive contributor image
########################################################################
FROM toolchain AS dev

COPY --from=cargo-tools /usr/local/cargo/bin/ /usr/local/cargo/bin/
COPY --from=cargo-tools /usr/local/bin/      /usr/local/bin/

# Need the Windows MSVC target installed for `cargo xwin build` to find
# the rustc target spec.
RUN rustup target add x86_64-pc-windows-msvc

ENV CARGO_HOME=/workspace/.cargo \
    CARGO_TARGET_DIR=/workspace/target \
    RUSTC_WRAPPER=sccache \
    SCCACHE_DIR=/workspace/.sccache \
    SCCACHE_CACHE_SIZE=10G \
    CARGO_INCREMENTAL=0 \
    RUST_BACKTRACE=1

# Pre-create cache mount targets so :ro / :cached bind mounts don't
# block the named volume mounts at runtime.
RUN mkdir -p /workspace/target /workspace/.cargo /workspace/.xwin-cache /workspace/.sccache

WORKDIR /workspace
CMD ["bash"]

########################################################################
# ci — same as dev; named separately so CI pins an explicit target
########################################################################
FROM dev AS ci
