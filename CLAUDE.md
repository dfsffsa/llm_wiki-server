# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`llm_wiki-server` is an integration repository that adds HTTP services + CLI + Web deployment to the upstream [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki) desktop application.

- **upstream/** — Official source as a git submodule (read-only)
- **overlay/** — Custom implementations layered on top
- **Wiki data** — NOT in this repo; stored separately in `~/overseas-github/llm_wiki_projects/<name>/`

## Common Commands

```bash
# Clone with submodules
git clone --recurse-submodules git@github.com:dfsffsa/llm_wiki-server.git

# One-time setup
git submodule update --init --recursive
npm install --prefix upstream

# Build everything (patch → UI → server → CLI)
./scripts/build-all.sh

# Build components individually
VITE_BACKEND=http VITE_API_TOKEN=your-token ./scripts/build-web.sh
cargo build --release --manifest-path overlay/server/Cargo.toml
./scripts/build-cli.sh

# Run server
export LLM_WIKI_PROJECT=~/overseas-github/llm_wiki_projects/YourProject
export LLM_WIKI_API_TOKEN=your-secret
export LLM_WIKI_CONFIG=overlay/config/server.example.json
export LLM_WIKI_STATIC=upstream/dist
./overlay/server/target/release/llm-wiki-server

# CLI commands (kebab-case arguments)
./scripts/llm-wiki search "query" --project "$LLM_WIKI_PROJECT"
./scripts/llm-wiki rescan --project "$LLM_WIKI_PROJECT" --json
./scripts/llm-wiki ingest source.md --project "$LLM_WIKI_PROJECT"

# Upgrade upstream
./scripts/sync-upstream.sh v0.4.20
./scripts/apply-patches.sh
./scripts/build-all.sh

# Tests
./scripts/e2e-full.sh           # Full pipeline: patch → build → CLI → HTTP API
./scripts/e2e-local.sh           # Local headless (no Docker)
npm run test:mocks --prefix upstream  # Upstream unit tests
```

## Architecture

```
llm_wiki-server/
├── upstream/           # Git submodule → official llm_wiki (v0.4.20)
│   ├── src/            # React 19 frontend + Tauri v2 backend
│   └── dist/           # Vite build output (static UI)
├── overlay/            # 100% custom code
│   ├── server/         # Headless HTTP server (Rust + tiny_http)
│   ├── cli/
│   │   ├── rust/       # CLI binary: search, rescan, ingest
│   │   └── node/       # Wraps upstream ingest.ts via shims
│   ├── web/            # HTTP React adapter (Vite alias replacements)
│   ├── crates/llm-wiki-common/  # Shared Rust lib (search, rescan, project)
│   └── patches/        # Applied to upstream at build time
├── scripts/             # Build, test, sync scripts
└── docker/             # Containerized deployment
```

### Key Design Principles

| Rule | Meaning |
|------|---------|
| Upstream zero-commit | Customizations go in `overlay/` + patch files |
| Patches at build time | `apply-patches.sh` modifies local upstream copy; official repo unchanged |
| HTTP UI read-only | Write operations via CLI (`ingest`, `reindex`), not HTTP |
| Chat via server proxy | Browser doesn't call LLM directly; server proxies to avoid CORS |

### HTTP API Routes

| Method | Path | Handler |
|--------|------|---------|
| GET | `/api/v1/health` | Health check (no auth) |
| GET | `/api/v1/projects` | List projects |
| GET | `/api/v1/projects/{id}/files` | File tree |
| GET | `/api/v1/projects/{id}/files/content` | Read file |
| POST | `/api/v1/projects/{id}/search` | Keyword search |
| GET | `/api/v1/projects/{id}/graph` | Wikilink graph |
| GET | `/api/v1/runtime-config` | LLM config summary |
| POST | `/api/v1/projects/{id}/chat` | SSE streaming chat (non-JSON route) |

### Web Adapter (HTTP Mode)

When `VITE_BACKEND=http`, Vite aliases redirect upstream imports to overlay:

| Upstream Import | Replaced With |
|-----------------|---------------|
| `@/commands/fs` | `overlay/web/commands/fs.ts` |
| `@/lib/search` | `overlay/web/lib/search.ts` |
| `@/lib/llm-client` | `overlay/web/lib/llm-client.ts` |

**Critical:** Overlay aliases must be BEFORE the generic `@` alias in `vite.config.ts`, otherwise HTTP builds will use upstream's Tauri `fs.ts` and `bootstrapHttpProject()` returns null.

## Configuration

| Variable | Purpose |
|----------|---------|
| `LLM_WIKI_PROJECT` | Wiki project root (contains `wiki/`, `raw/`, `.llm-wiki/`) |
| `LLM_WIKI_API_TOKEN` | API auth token (must match VITE_API_TOKEN at build time) |
| `LLM_WIKI_CONFIG` | JSON config file with llmConfig, projects[], etc. |
| `LLM_WIKI_STATIC` | Static UI directory (typically `upstream/dist`) |
| `LLM_WIKI_BIND` | Server listen address (default `127.0.0.1:8080`) |

## Wiki Project Structure

Wiki data lives outside this repo at `~/overseas-github/llm_wiki_projects/<Name>/`:

```
<project>/
├── purpose.md              # Project goals (read by LLM during ingest)
├── schema.md               # Wiki page type conventions
├── raw/sources/            # Raw materials (immutable input)
├── wiki/                   # LLM-generated knowledge pages
│   ├── index.md, log.md
│   ├── sources/, entities/, concepts/
├── .llm-wiki/
│   ├── project.json        # UUID
│   ├── ingest-queue.json
│   ├── chats/              # Desktop chat persistence
│   └── lancedb/            # Vector index
```

## Development Workflows

### Adding new materials to a wiki

1. Place source files in `<project>/raw/sources/`
2. Run `ingest-batch.sh` or individual `llm-wiki ingest`
3. Optionally rebuild vector index: `llm-wiki reindex --vectors`
4. Sync to server: `rsync` the project directory

### Upstream upgrade process

```bash
cd upstream && git reset --hard
./scripts/sync-upstream.sh vX.Y.Z
./scripts/apply-patches.sh
# If patch conflicts: manually merge, then:
# cd upstream && git diff > ../overlay/patches/0002-http-ui-bootstrap.patch
./scripts/build-all.sh
git add upstream overlay/patches
git commit -m "chore: bump upstream to vX.Y.Z"
```

## Related Documentation

- [README.md](README.md) — Project overview
- [docs/代码结构总览.md](docs/代码结构总览.md) — Detailed architecture diagrams
- [docs/开发与测试.md](docs/开发与测试.md) — Build, test, and FAQ
- [docs/上游同步.md](docs/上游同步.md) — Submodule sync principles
- [docs/日常运维.md](docs/日常运维.md) — Daily operations
