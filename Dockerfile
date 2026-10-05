# syntax=docker/dockerfile:1
# ============================================================
# Dockerfile — Sophia OSS 一発起動用（ソースからビルド）
# docker compose up --build で DB + API(+SPA) を起動する想定
# ============================================================

# --- Frontend SPA export ---
FROM node:20-bookworm-slim AS frontend-builder
WORKDIR /app/frontend
# ホスト由来の npm_config_*（例: devdir）が BuildKit 経由で混入すると npm ci が失敗するため無効化
ENV npm_config_devdir= \
    NPM_CONFIG_DEVDIR=
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
COPY scripts/toggle_export_routes.sh /app/scripts/toggle_export_routes.sh
# Docker イメージ内では restore（git checkout）不要。out/ だけ成果物として使う。
RUN chmod +x /app/scripts/toggle_export_routes.sh \
 && /app/scripts/toggle_export_routes.sh apply \
 && NEXT_OUTPUT=export npm run build

# --- Rust backend ---
FROM rust:1.98-bookworm AS rust-builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev python3 python3-pip \
 && pip3 install --no-cache-dir --break-system-packages reportlab \
 && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src ./src
COPY migrations ./migrations
COPY static ./static
COPY fonts ./fonts
COPY scripts/pdf ./scripts/pdf
COPY askama.toml ./

# sqlx は実行時クエリのみのためビルド時 DB 不要
RUN --mount=type=cache,target=/app/target,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo build --release && cp target/release/sophia /app/sophia

# --- Runtime ---
FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 python3 python3-pip \
 && pip3 install --no-cache-dir --break-system-packages reportlab \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -r -s /bin/false appuser \
 && mkdir -p /app/data /app/logs /app/media /app/credentials

COPY --from=rust-builder /app/sophia /usr/local/bin/sophia
COPY --from=rust-builder /app/migrations /app/migrations
COPY --from=rust-builder /app/static /app/static
COPY --from=rust-builder /app/fonts /app/fonts
COPY --from=rust-builder /app/scripts/pdf /app/scripts/pdf
COPY --from=frontend-builder /app/frontend/out /app/frontend/out

RUN chown -R appuser:appuser /app
USER appuser

ENV PORT=8111
EXPOSE 8111
CMD ["sophia", "serve"]
