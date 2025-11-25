FROM hexpm/elixir:1.18.4-erlang-27.3.4-debian-trixie-20251117

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

# Install Rust 1.88.0 with wasm32-wasip2 target
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain  1.88.0 --target wasm32-wasip2
ENV PATH="/root/.cargo/bin:${PATH}"
RUN wget -qO- https://apt.llvm.org/llvm.sh | bash -s -- 18

# Install just, wkg, and wash using cargo-binstall for faster installation
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall -y just wkg wash

# Install Bun
RUN curl -fsSL https://bun.sh/install | bash -s -- bun-v1.3.3
ENV PATH="/root/.bun/bin:${PATH}"

# Set working directory
WORKDIR /app

# Copy dependency files first for better caching
COPY mix.exs mix.lock ./
COPY package.json bun.lock bunfig.toml ./

# Copy all source files
COPY . .

# Install Elixir dependencies
RUN mix local.hex --force && \
    mix local.rebar --force && \
    mix deps.get

# Install npm dependencies
RUN --mount=type=secret,id=npm_token,env=npm_token bun install --frozen-lockfile --production --verbose

# Build WASM components
RUN mix build

# Default command - can be overridden to run publish or other commands
CMD ["bash"]
