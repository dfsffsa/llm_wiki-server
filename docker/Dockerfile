# LLM Wiki Server — multi-stage build (Phase 1+)
#
# 产出：llm-wiki-server 二进制 + upstream/dist 静态 UI
# 说明见 docker/README.md
#
# Stage 1: build upstream UI (HTTP read-only overlay)
FROM node:22-bookworm AS ui-builder
WORKDIR /build
COPY upstream/package.json upstream/package-lock.json ./upstream/
RUN npm ci --prefix upstream
COPY upstream/ ./upstream/
COPY overlay/web ./overlay/web
COPY overlay/patches ./overlay/patches
COPY scripts/apply-patches.sh scripts/build-web.sh ./scripts/
RUN chmod +x ./scripts/apply-patches.sh ./scripts/build-web.sh && ./scripts/apply-patches.sh
ENV VITE_BACKEND=http
ARG VITE_API_TOKEN=""
ENV VITE_API_TOKEN=${VITE_API_TOKEN}
RUN ./scripts/build-web.sh

# Stage 2: build overlay server
# protobuf-compiler is required at build time: llm-wiki-common links lancedb,
# whose transitive dep lance-encoding runs a prost-build script that invokes
# `protoc`. Without it `cargo build` fails with "Could not find protoc".
FROM rust:1-bookworm AS server-builder
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
ENV PROTOC=/usr/bin/protoc
COPY overlay/crates/llm-wiki-common/Cargo.toml overlay/crates/llm-wiki-common/Cargo.lock* overlay/crates/llm-wiki-common/
COPY overlay/crates/llm-wiki-common/src overlay/crates/llm-wiki-common/src
COPY overlay/server/Cargo.toml overlay/server/Cargo.lock overlay/server/
COPY overlay/server/src overlay/server/src
RUN cargo build --release --manifest-path overlay/server/Cargo.toml

# Stage 3: runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=ui-builder /build/upstream/dist ./dist
COPY --from=server-builder /build/overlay/server/target/release/llm-wiki-server /usr/local/bin/llm-wiki-server
EXPOSE 8080
ENV LLM_WIKI_BIND=0.0.0.0:8080
ENV LLM_WIKI_STATIC=/app/dist
# Liveness probe: process up + health endpoint responds. Readiness depth
# (DB/LLM reachability) is a separate concern handled by /health itself.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/api/v1/health || exit 1
CMD ["llm-wiki-server"]