# LLM Wiki Server — multi-stage build (Phase 1+)
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
FROM rust:1-bookworm AS server-builder
WORKDIR /build
COPY overlay/server/Cargo.toml overlay/server/Cargo.lock* overlay/server/
COPY overlay/server/src overlay/server/src
RUN cargo build --release --manifest-path overlay/server/Cargo.toml

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
ENV LLM_WIKI_STATIC=/app/dist
CMD ["llm-wiki-server"]