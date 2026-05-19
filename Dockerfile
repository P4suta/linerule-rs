# Dev image for linerule-rs. Ships every tool the Justfile recipes invoke,
# so host machines need nothing beyond Docker (matches linerule-cs's
# "Docker-only" stance — see ADR-0001).

FROM rust:1.95-bookworm AS dev

ARG USER_UID=1000
ARG USER_GID=1000
ARG NODE_MAJOR=22

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:/usr/local/rustup/bin:$PATH

# Base packages: build tooling for cargo-xwin, jq for log-tail recipes,
# curl/ca-certificates for fetching tarballs/.debs, git for cargo-deny,
# graphviz for `cargo depgraph` → SVG rendering.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        clang \
        lld \
        cmake \
        pkg-config \
        ca-certificates \
        curl \
        git \
        gnupg \
        graphviz \
        jq \
        unzip \
        sudo \
    && rm -rf /var/lib/apt/lists/*

# Node 22 LTS for commitlint.
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain extras.
RUN rustup target add x86_64-pc-windows-msvc \
    && rustup component add rustfmt clippy rust-src

# cargo-installed tooling — single source of truth for "what runs in CI".
RUN cargo install --locked --root /usr/local \
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

# lefthook (.deb release).
RUN ARCH="$(dpkg --print-architecture)" \
    && curl -fsSL -o /tmp/lefthook.deb \
        "https://github.com/evilmartians/lefthook/releases/latest/download/lefthook_${ARCH}.deb" \
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

# Match host UID so bind-mounted files don't end up root-owned.
RUN groupadd --gid ${USER_GID} dev \
    && useradd --uid ${USER_UID} --gid ${USER_GID} -m dev \
    && echo "dev ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

USER dev
ENV INSIDE_CONTAINER=1

WORKDIR /workspace
CMD ["bash"]
