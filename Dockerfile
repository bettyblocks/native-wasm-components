FROM debian:trixie-slim AS components

# Avoid prompts from apt
ENV DEBIAN_FRONTEND=noninteractive

# Install basic dependencies
RUN apt-get update && apt-get install -y \
    wget \
    curl \
    wget \
    git \
    build-essential \
    libssl-dev \
    ca-certificates \
    unzip \
    gnupg \
    lsb-release \
    && rm -rf /var/lib/apt/lists/*

# Install Rust 1.92.0 with wasm32-wasip2 target
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain  1.92.0 --profile minimal --target wasm32-wasip2
ENV PATH="/root/.cargo/bin:${PATH}"
RUN wget -qO- https://apt.llvm.org/llvm.sh | bash -s -- 18

# Install just and wkg using cargo-binstall for faster installation
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall -y just wkg

# Install wash 2.x from the official script. cargo binstall resolves an older
# wash that still requires a wasmcloud.toml for `wash wit fetch`; the new build
# flow (wash wit fetch + wash build --skip-fetch + .wash/config.yml) needs 2.x.
RUN curl -fsSL https://wasmcloud.com/sh | INSTALL_DIR=/usr/local/bin bash \
    && wash --version

WORKDIR /app

COPY . .

RUN just build
RUN just clean

FROM oven/bun:1.3-alpine

WORKDIR /app
COPY . .
COPY --from=components /app/functions /app/functions

RUN --mount=type=secret,id=npm_token,env=npm_token bun install --frozen-lockfile --production --verbose

CMD ["bash"]
