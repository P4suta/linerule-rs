# Dev image for linerule-rs. Ships every tool the Justfile recipes invoke,
# so host machines need nothing beyond Docker.
#
# Speed-tuning policy (Rust builds are famously slow; bake the standard
# accelerators in):
#   - cargo-binstall pulls prebuilt binaries instead of `cargo install --locked`
#     compiling each tool from source (cuts ~20 min to ~30 s).
#   - mold linker via clang shaves 30–70 % off link time on native dev builds.
#     Wired up via .cargo/config.toml for x86_64-unknown-linux-gnu.
#   - BuildKit cache mounts retain the cargo registry / git index between
#     builds so Cargo.lock churn doesn't re-download every crate.

# syntax=docker/dockerfile:1.7

FROM rust:1.95-bookworm AS dev

ARG USER_UID=1000
ARG USER_GID=1000
ARG NODE_MAJOR=22

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:/usr/local/rustup/bin:$PATH

# Base packages: build tooling for cargo-xwin, mold linker for native linking,
# jq for log-tail recipes, curl/ca-certificates for fetching tarballs/.debs,
# git for cargo-deny, graphviz for `cargo depgraph` → SVG rendering.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        clang \
        cmake \
        ca-certificates \
        curl \
        git \
        gnupg \
        graphviz \
        jq \
        lld \
        mold \
        pkg-config \
        sudo \
        unzip \
    && rm -rf /var/lib/apt/lists/*

# Node 22 LTS for commitlint.
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain extras.
RUN rustup target add x86_64-pc-windows-msvc \
    && rustup component add rustfmt clippy rust-src

# cargo-binstall: single-binary release, downloads prebuilt binaries for
# subsequent cargo subcommands. Vastly faster than `cargo install --locked`.
RUN curl -fsSL https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
    | bash

# Rust tooling — prefer prebuilt binaries via cargo-binstall, fall back
# automatically if no binary release is published for a given crate.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo binstall --no-confirm --no-symlinks \
        cargo-xwin \
        cargo-deny \
        cargo-audit \
        cargo-llvm-cov \
        cargo-nextest \
        cargo-machete \
        cargo-sort \
        cargo-rdme \
        cargo-modules \
        cargo-depgraph \
        just \
        taplo-cli \
        typos-cli

# actionlint (binary release).
RUN curl -fsSL https://raw.githubusercontent.com/rhysd/actionlint/main/scripts/download-actionlint.bash \
    | bash -s -- latest /usr/local/bin

# lefthook (.deb release). The asset name embeds the version, so resolve the
# latest tag via the GitHub API first.
RUN ARCH="$(dpkg --print-architecture)" \
    && LEFTHOOK_VERSION="$(curl -fsSL https://api.github.com/repos/evilmartians/lefthook/releases/latest \
        | sed -n 's/.*\"tag_name\": *\"v\([^\"]*\)\".*/\1/p')" \
    && curl -fsSL -o /tmp/lefthook.deb \
        "https://github.com/evilmartians/lefthook/releases/download/v${LEFTHOOK_VERSION}/lefthook_${LEFTHOOK_VERSION}_${ARCH}.deb" \
    && dpkg -i /tmp/lefthook.deb \
    && rm /tmp/lefthook.deb

# biome (Rust-backed formatter, JSON). Single binary, no Node required.
RUN ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64) BIOME_ARCH="linux-x64" ;; \
        arm64) BIOME_ARCH="linux-arm64" ;; \
        *) echo "unsupported arch: $ARCH" >&2 && exit 1 ;; \
    esac \
    && curl -fsSL "https://github.com/biomejs/biome/releases/latest/download/biome-${BIOME_ARCH}" \
        -o /usr/local/bin/biome \
    && chmod +x /usr/local/bin/biome

# yamlfmt (Go single binary, formatter for YAML).
RUN ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64) YAMLFMT_ARCH="Linux_x86_64" ;; \
        arm64) YAMLFMT_ARCH="Linux_arm64" ;; \
        *) echo "unsupported arch: $ARCH" >&2 && exit 1 ;; \
    esac \
    && YAMLFMT_VERSION="0.13.0" \
    && curl -fsSL "https://github.com/google/yamlfmt/releases/download/v${YAMLFMT_VERSION}/yamlfmt_${YAMLFMT_VERSION}_${YAMLFMT_ARCH}.tar.gz" \
        | tar xz -C /usr/local/bin yamlfmt

# Match host UID so bind-mounted files don't end up root-owned. Also chown
# CARGO_HOME / RUSTUP_HOME so named volumes mounted under them (cargo-registry
# / cargo-git in compose.yml) inherit dev ownership instead of root, which
# would break `cargo` writes from the dev user.
RUN groupadd --gid ${USER_GID} dev \
    && useradd --uid ${USER_UID} --gid ${USER_GID} -m dev \
    && echo "dev ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers \
    && chown -R dev:dev /usr/local/cargo /usr/local/rustup \
    && mkdir -p /home/dev/.cache/cargo-xwin \
    && chown -R dev:dev /home/dev/.cache

USER dev
ENV INSIDE_CONTAINER=1

WORKDIR /workspace
CMD ["bash"]
