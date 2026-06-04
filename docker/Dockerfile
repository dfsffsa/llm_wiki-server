# LLM Wiki Server — multi-stage build (Phase 1+)
#
# Stage 1: build upstream UI
FROM node:22-bookworm AS ui-builder
WORKDIR /build
COPY upstream/package.json upstream/package-lock.json ./upstream/
RUN npm ci --prefix upstream
COPY upstream/ ./upstream/
RUN npm run build --prefix upstream

# Stage 2: build overlay server (when implemented)
FROM rust:1-bookworm AS server-builder
WORKDIR /build
COPY overlay/server/Cargo.toml overlay/server/Cargo.lock* overlay/server/
COPY overlay/server/src overlay/server/src
# Phase 1: uncomment when server links upstream core
# COPY upstream/src-tauri/Cargo.toml upstream/src-tauri/Cargo.lock upstream/src-tauri/
RUN cargo build --release --manifest-path overlay/server/Cargo.toml || true

# Stage 3: runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=ui-builder /build/upstream/dist ./dist
COPY --from=server-builder /build/overlay/server/target/release/llm-wiki-server /usr/local/bin/llm-wiki-server
EXPOSE 8080
ENV LLM_WIKI_BIND=0.0.0.0:8080
CMD ["llm-wiki-server"]
