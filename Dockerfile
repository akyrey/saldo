# syntax=docker/dockerfile:1
# Saldo: one image with the server (`saldo`) and the importer (`saldo-import`).
# Phase 4 adds a Node stage that builds web/ into the server.

# --- chef: cargo-chef caches dependency builds between source changes ---
FROM rust:1.100-slim-trixie AS chef
RUN cargo install cargo-chef --locked --version 0.1.73
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# SQL is checked against the committed .sqlx/ data; no database at build time.
ENV SQLX_OFFLINE=true
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked --bin saldo --bin saldo-import

# --- runtime: slim Debian, non-root, data in /data ---
FROM debian:trixie-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin saldo \
    && mkdir -p /data \
    && chown saldo:saldo /data
COPY --from=builder /app/target/release/saldo /app/target/release/saldo-import /usr/local/bin/
USER saldo
ENV SALDO_DATABASE_URL=sqlite:///data/saldo.db \
    SALDO_DATA_DIR=/data \
    SALDO_BIND=0.0.0.0:8080 \
    SALDO_LOG=info
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["saldo"]
CMD ["serve"]
