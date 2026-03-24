# syntax=docker/dockerfile:1
#
# Multi-stage build for ferrite-server + dashboard.
# Serves the Dioxus WASM dashboard as static files from the server.

# ── Stage 1: Dependency caching with cargo-chef ──────────────────────

FROM rust:1.84-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 2: Build server + dashboard ────────────────────────────────

FROM chef AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev curl \
    && rm -rf /var/lib/apt/lists/*

# Install sccache for compile caching
RUN curl -fsSL https://github.com/mozilla/sccache/releases/download/v0.9.1/sccache-v0.9.1-x86_64-unknown-linux-musl.tar.gz \
    | tar xz --strip-components=1 -C /usr/local/bin/ sccache-v0.9.1-x86_64-unknown-linux-musl/sccache \
    && chmod +x /usr/local/bin/sccache

ENV RUSTC_WRAPPER=/usr/local/bin/sccache
ENV SCCACHE_DIR=/tmp/sccache

# Install wasm target + dx CLI for dashboard build
RUN rustup target add wasm32-unknown-unknown
RUN --mount=type=cache,target=/tmp/sccache \
    cargo install dioxus-cli@0.6.3 --locked

# Cook dependencies from recipe (cached layer)
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/tmp/sccache \
    cargo chef cook --release --recipe-path recipe.json -p ferrite-server

# Copy full source
COPY . .

# Build server
RUN --mount=type=cache,target=/tmp/sccache \
    cargo build -p ferrite-server --release

# Build dashboard (WASM)
RUN --mount=type=cache,target=/tmp/sccache \
    cd ferrite-dashboard && dx build --release

# ── Stage 3: Minimal runtime ────────────────────────────────────────

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates binutils \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash ferrite
USER ferrite
WORKDIR /home/ferrite

# Copy server binary
COPY --from=builder --chown=ferrite:ferrite \
    /app/target/release/ferrite-server ./ferrite-server

# Copy dashboard static files
COPY --from=builder --chown=ferrite:ferrite \
    /app/target/dx/ferrite-dashboard/release/web/public/ ./dashboard/

# Create data directories
RUN mkdir -p data elfs

EXPOSE 4000

ENV RUST_LOG=info

ENTRYPOINT ["./ferrite-server"]
CMD ["--http", "0.0.0.0:4000", "--db", "./data/ferrite.db", "--elf-dir", "./elfs", "--static-dir", "./dashboard"]
