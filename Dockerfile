# QorePlatform server image — qore-server (Enterprise, BUSL-1.1).
# Self-hosted control plane: serves the SPA and the HTTP/SSE bridge.

# --- Stage 1: build the web SPA ---
FROM node:22-bookworm-slim AS web
WORKDIR /app
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY . .
RUN pnpm build

# --- Stage 2: build the server binary ---
FROM rust:1-bookworm AS server
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake clang pkg-config libdbus-1-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY src-tauri ./src-tauri
RUN cargo build --release --manifest-path src-tauri/Cargo.toml -p qore-server

# --- Stage 3: runtime ---
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libstdc++6 libdbus-1-3 && rm -rf /var/lib/apt/lists/*
RUN useradd --system --create-home --uid 10001 qore
WORKDIR /app
COPY --from=server /build/src-tauri/target/release/qore-server /usr/local/bin/qore-server
COPY --from=web /app/dist /app/web
USER qore
ENV QORE_SERVER_HOST=0.0.0.0 \
    QORE_SERVER_PORT=8088 \
    QORE_SERVER_WEB_DIR=/app/web \
    QORE_VAULT_FILE=/data/vault.enc \
    QOREDB_CONFIG_DIR=/data
EXPOSE 8088
VOLUME ["/data"]
CMD ["qore-server"]
