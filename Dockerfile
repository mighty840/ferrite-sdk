# syntax=docker/dockerfile:1
#
# Two-stage build for ferrite-server + Dioxus WASM dashboard.

# ── Stage 1: Build ───────────────────────────────────────────────────

FROM rust:1.84-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown
RUN cargo install dioxus-cli@0.6.3 --locked

WORKDIR /app
COPY . .

RUN cargo build -p ferrite-server --release
RUN cd ferrite-dashboard && dx build --release

# ── Stage 2: Runtime ─────────────────────────────────────────────────

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates binutils \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash ferrite
USER ferrite
WORKDIR /home/ferrite

COPY --from=builder --chown=ferrite:ferrite \
    /app/target/release/ferrite-server ./ferrite-server

COPY --from=builder --chown=ferrite:ferrite \
    /app/target/dx/ferrite-dashboard/release/web/public/ ./dashboard/

COPY --from=builder --chown=ferrite:ferrite \
    /app/docker/entrypoint.sh ./entrypoint.sh

RUN mkdir -p data elfs

EXPOSE 4000
ENV RUST_LOG=info

ENTRYPOINT ["./entrypoint.sh"]
