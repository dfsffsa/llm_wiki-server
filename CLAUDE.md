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
./scripts/e2e-auth.sh            # Cookie-auth path: register → verify-email → login → me → conversations → usage-limit → logout
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
│   ├── auth/           # llm-wiki-auth crate: sessions, password hashing, rate limits, per-user history + usage quotas
│   ├── cli/
│   │   ├── rust/       # CLI binary: search, rescan, ingest, reindex, preprocess, vector
│   │   └── node/       # Wraps upstream ingest.ts via shims (only Node still required)
│   ├── web/            # HTTP React adapter (Vite alias replacements)
│   ├── eval/           # RAG eval: ingest_check / rag_eval / generate_test_cases (v1 + v2 schemas)
│   ├── static/         # Public landing pages: auth HTML (login/register/reset), /lite/ QA page, pricing, docs
│   ├── embedding/      # Embedding exploration scripts (no API keys)
│   ├── crates/llm-wiki-common/  # Shared Rust lib (search, rescan, project, vector/LanceDB, hybrid fusion)
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
| Chat in pure Rust | `/chat` streams via reqwest directly to the LLM — no Node subprocess. Only `ingest` still shells out to Node TS (upstream ingest.ts is Zustand-coupled) |
| Hybrid search, degrade-safe | `/search` does keyword + vector (RRF); vector side is fully fault-tolerant — no embedding config / unreachable endpoint / missing LanceDB table all fall back to keyword-only with HTTP 200, never 500 |

### HTTP API Routes

| Method | Path | Handler |
|--------|------|---------|
| GET | `/api/v1/health` | Health check (no auth); `?deep=true` pings auth DB for readiness |
| GET | `/metrics` | Prometheus text metrics (no auth): request/chat/error counters, in-flight gauges |
| GET | `/api/v1/projects` | List projects |
| GET | `/api/v1/projects/{id}/files` | File tree |
| GET | `/api/v1/projects/{id}/files/content` | Read file |
| POST | `/api/v1/projects/{id}/search` | Hybrid search (keyword + vector via RRF; degrades to keyword-only) |
| GET | `/api/v1/projects/{id}/graph` | Wikilink graph |
| GET | `/api/v1/runtime-config` | LLM config summary |
| POST | `/api/v1/projects/{id}/chat` | SSE streaming chat (non-JSON route, Rust reqwest, no Node) |

Multi-user routes — active only when `LLM_WIKI_AUTH_DB` is set (cookie/bearer auth):

| Method | Path | Handler |
|--------|------|---------|
| POST | `/auth/register` | Register user → sends verification email (403 if `LLM_WIKI_DISABLE_REGISTRATION`) |
| GET | `/auth/verify-email` | Verify registration email (`?token=`; redirects). **Login blocked until verified** |
| POST | `/auth/login` | Login → sets `session` cookie (fails 403 if email unverified) |
| POST | `/auth/logout` | Clears session cookie |
| GET | `/auth/me` | Current user (cookie required) |
| POST | `/auth/change-email` | Request email change (sends token to new address) |
| POST | `/auth/confirm-email-change` | Confirm email change via token |
| POST | `/auth/forgot-password` | Send reset email (needs SMTP config) |
| POST | `/auth/reset-password` | Reset password via emailed token |
| GET | `/api/v1/conversations` | Per-user chat history — list / create |
| POST | `/api/v1/conversations` | Create conversation |
| DELETE | `/api/v1/conversations/{id}` | Delete conversation |
| GET/POST | `/api/v1/conversations/{id}/messages` | List / send chat messages |

Billing routes — active only when `billing` config block is present (Waffo Pancake, see [docs/付款-Waffo-Pancake.md](./docs/付款-Waffo-Pancake.md)):

| Method | Path | Handler |
|--------|------|---------|
| POST | `/api/v1/billing/checkout` | Create Waffo checkout session (cookie auth; `{plan:"pro"}` → `{checkoutUrl}`; 409 if already subscribed) |
| POST | `/api/v1/billing/webhook` | Waffo webhook (RSA-SHA256 verified, raw body, idempotent) → updates per-user plan |

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
| `LLM_WIKI_CONFIG` | JSON config file with llmConfig, embeddingConfig, smtp, projects[], etc. |
| `LLM_WIKI_STATIC` | Static UI directory (typically `upstream/dist`) |
| `LLM_WIKI_BIND` | Server listen address (default `127.0.0.1:8080`) |
| `LLM_WIKI_AUTH_DB` | SQLite path for auth/history/usage. Unset → multi-user mode off |
| `LLM_WIKI_REQUIRE_LOGIN` | Force cookie/bearer auth (reject anonymous even if `allowUnauthenticated:true`) |
| `LLM_WIKI_DISABLE_REGISTRATION` | Close public `/auth/register` (403) — invite-only |
| `LLM_WIKI_DAILY_CHAT_LIMIT` | Per-user daily chat quota (cookie auth only; default 50) |
| `LLM_WIKI_DRAIN_SECS` | Graceful-shutdown drain window before forcing exit (default 15) |
| `LLM_WIKI_ADMIN_EMAIL` | Email auto-marked `is_admin` on registration |
| `LLM_WIKI_SESSION_TTL_DAYS` | Session cookie lifetime (default 30) |
| `LLM_WIKI_PUBLIC_LANDING_DIR` | Public landing page dir (login/register/reset HTML; default `overlay/static/`) |
| `RUST_LOG` | tracing log level (default `info`; e.g. `debug,llm_wiki_server=trace`) |
| `billing` config block | Waffo Pancake: `waffoMerchantId`/`waffoPrivateKey`/`proProductId`/`webhookPublicKey`/`environment`/`freeTierDailyLimit`(3)/`proTierDailyLimit`(10000)/`checkoutSuccessUrl`/`language`. Present → per-plan chat quota + checkout/webhook routes; absent → global `LLM_WIKI_DAILY_CHAT_LIMIT` unchanged |
| `WAFFO_MERCHANT_ID` / `WAFFO_PRIVATE_KEY` / `WAFFO_PRO_PRODUCT_ID` / `WAFFO_WEBHOOK_PUBLIC_KEY` | Waffo credentials, injected via `deploy-ecs.sh` sed into `server.local.json` (chmod 600). Unset `${VAR}` → billing fail-closed with a warn log |

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

## Build Outputs & Scripts

After `build-all.sh`, you get three categories of artifacts (all `.gitignore`d):

| Artifact | Size | Purpose | How to invoke |
|----------|------|---------|---------------|
| `overlay/server/target/release/llm-wiki-server` | ~1.1 MB | HTTP read-only server (Rust + tiny_http + reqwest); does hybrid search + chat streaming in-process | direct: `./llm-wiki-server`; or via systemd |
| `overlay/cli/rust/target/release/llm-wiki` | ~50 MB | CLI: search / rescan / ingest / reindex (Rust, **links lancedb + arrow + datafusion** via llm-wiki-common) | `./scripts/llm-wiki <subcmd>` |
| `upstream/dist/` | few MB | Vite-built React UI (HTTP or desktop mode) | static-served by server at `/` and `/lite/` |
| `musl` variant of CLI + server | same sizes | static-pie linked, runs on any x86_64 Linux (incl. low-spec ECS) | under `target/x86_64-unknown-linux-musl/release/` |

`scripts/llm-wiki` is a 364-byte bash wrapper: it sets `LLM_WIKI_REPO=$PWD` and `exec`s the Rust binary, so CLI subcommands (`ingest`, `search`, `reindex`) all go through it. The Rust binary for `ingest` is itself a thin shim that drives `node <tsx cli.mjs> overlay/cli/node/src/cmd-ingest.ts` (TypeScript) which calls upstream's `autoIngest()`. **Only `ingest` still shells out to Node** — chat and search are now pure Rust (reqwest / LanceDB).

## RAG Evaluation (`overlay/eval`)

Three-layer eval of wiki quality (Python, run from the dev machine against a live server + wiki project):

| Script | Purpose |
|--------|---------|
| `ingest_check.py` | Layer 1 — schema compliance, structure, wikilink density, coverage of generated wiki pages |
| `rag_eval.py` | Layers 2–3 — retrieval Recall@K/MRR against `expected_sources`, chat answer + citation quality |
| `generate_test_cases.py` | LLM-assisted test-case generation (`--mode auto` / `hybrid`), emits v1 or v2 schema |
| `llm_judge.py` | **LLM-as-Judge** content eval: role A (Extractor) pulls claims from source, role B (Evaluator) scores wiki page coverage. Dual-model via `--config-a`/`--config-b` (`overlay/config/llm.judge.{a,b}.json`); retry+backoff, hallucination grading, incremental rerun, `--auto-fix` via `judge/repairer.py` |
| `auto_fix.py` | Ingest auto-repair pipeline (`fixers/frontmatter.py`, `fixers/wikilink.py`); `--dry-run` preview, `--budget N` cap, backups to `fix_backups/`, regression re-check |
| `rerun_failed.py` / `run_auto_fix.py` | Re-run only failed cases / batch auto-fix driver |
| `scripts/run_eval.sh` | One-shot: health check → ingest_check → rag_eval. `[project] [mode] [--fix]`. **Needs server up on :8080** |

```bash
./overlay/eval/scripts/run_eval.sh ParentingBooks            # full (ingest + retrieval + chat)
./overlay/eval/scripts/run_eval.sh ParentingBooks all --fix  # ...then auto-repair ingest issues
python overlay/eval/rag_eval.py --project <path> --mode retrieval
python overlay/eval/generate_test_cases.py --project <path> --mode auto --schema v2
python overlay/eval/llm_judge.py --project <path> --config-a overlay/config/llm.judge.a.json --config-b overlay/config/llm.judge.b.json --sample 20
python -m unittest discover -s overlay/eval/tests            # P0 + judge + auto-fix regression tests
```

Test cases live in `overlay/eval/test_cases/*.json` — **v1** (`expected_sources: [...]`) vs **v2** (`expected_sources: {must:[], should:[]}`, from July 2026). **Historical caveat:** eval metric numbers were unreliable before the v2 work — `recall_at_k` was actually MRR. See `overlay/eval/AUDIT_2026-06-23.md`; the v2 schema + match helpers (`match_at_k`, `matched_patterns`) fix that. Don't trust pre-July results for decisions.

## Deployment

Two scripts in `scripts/`, both driven by `SSH_HOST` / `SSH_PORT` / `SSH_CONFIG` env:

| Script | Scope | Use when | Time (incremental) |
|--------|-------|----------|--------------------|
| `scripts/deploy-ecs.sh` | full: binary + dist + node_modules + `overlay/static/` + `server.local.json` + systemd unit + restart. `SKIP_SYSTEMD=1` deploys files + writes the unit to the repo dir only (no `/etc`, no start) — for users without sudo | first deploy, new machine, systemd/config change | 1–5 min |
| `scripts/sync-artifacts.sh` | incremental: binary + dist + node_modules + `overlay/static/` (no systemd/config) | routine iteration after local rebuild | ~10s |

Both accept a server-side `LLM_API_KEY` env (read at deploy time, injected into `server.local.json` via `sed` + `chmod 600`); `server.local.json` itself is gitignored via `*.local.json`. **Never hardcode the key in either script.**

**Remote does NOT need `git clone` or `npm ci`** — both scripts rsync everything the runtime needs (binaries, `upstream/dist/`, `upstream/src/` for `@/` alias resolution, `node_modules/`, config). The dev machine is the single source of truth; the remote only consumes artifacts over SSH. Remote requirements: Node.js + systemd. No Rust toolchain, no npm, no protoc, no git repo.

| Remote needs | Remote does NOT need |
|--------------|---------------------|
| Node.js 20+ (for the **ingest** tsx subprocess only — chat is now pure Rust) | git (no `git clone` / `git pull` on remote) |
| systemd (service management) | Rust toolchain / cargo |
| `rsync` target dir `/root/llm_wiki-server/` | npm / npx (node_modules rsynced from dev machine) |
| LLM API reachable (for chat + ingest) | protoc / lancedb build deps |

> **Chat no longer needs Node on the remote.** Since the reqwest rewrite, `/chat` streams entirely in the Rust server. Node is still required only for `ingest` (which wraps upstream's Zustand-coupled `ingest.ts`). If you only deploy the read-only + chat server without ingest, Node can be dropped.

Typical flow on a low-spec ECS (1.6 GB RAM, can't compile locally):

```bash
# 本地：交叉编译 musl 静态二进制 + 构建 UI
cargo build --release --target x86_64-unknown-linux-musl --manifest-path overlay/server/Cargo.toml
cargo build --release --target x86_64-unknown-linux-musl --manifest-path overlay/cli/rust/Cargo.toml
VITE_BACKEND=http VITE_API_TOKEN="$VITE_API_TOKEN" ./scripts/build-web.sh

# 本地：增量同步产物（~10s 增量 / 500MB 首次）
SSH_HOST=root@47.103.39.152 SSH_PORT=22022 ./scripts/sync-artifacts.sh

# 远端：重启服务（仅二进制或 dist 变化时需要）
ssh -p 22022 root@47.103.39.152 'systemctl restart llm-wiki-server'
```

The dev-machine (`wanghuacun`) is the build machine; remote ECS (47.103.39.152) only consumes artifacts over SSH. **Code lives in git on the dev machine, artifacts live on disk on both** (the 50 MB CLI binary never enters the git repo).

> **部署实战记录（2026-08-02）**：完整跑通**双服务器**——ecs99（阿里云 47.103.39.152:22022，`/root`，root）服务 `cn.sship.online`，ecs199（腾讯 170.106.132.196，`/home/li/code/personal`，li 用户 sudo 需密码，用 `SKIP_SYSTEMD=1` 手动装 unit）服务 `glb.sship.online`；Resend 邮件（域名 sship.online 已验证）；火山 Ark LLM（`deepseek-v4-flash`，`apiMode` 必须 `chat_completions`）。`deploy-ecs.sh` 新增 `SMTP_PASS`/`SERVER_AUTH_DB`/`CONFIG_LOCAL`/`SKIP_SYSTEMD` 参数，且 rsync `overlay/static`。**`server.example.json` 是纯模板勿直接部署**（见 [docs/部署实战与排错记录-2026-08-02.md](./docs/部署实战与排错记录-2026-08-02.md)）。

## Patched Submodule Architecture

`upstream/` is a git submodule pointing at `https://github.com/nashsu/llm_wiki.git`. We can't (and don't) push to it. Customizations live in `overlay/patches/0002-http-ui-bootstrap.patch`.

```
[clean v0.4.20 (9712d43)]   ← submodule pointer (stable, in git)
            ↓
   scripts/apply-patches.sh   ← called by build-all.sh automatically
            ↓
[patches applied working tree]  ← git status shows "dirty" upstream (EXPECTED, do not commit)
            ↓
   ./scripts/build-web.sh / build-cli.sh
            ↓
        up-to-date dist/ + binaries
```

**Do NOT commit the upstream submodule pointer change.** The pointer stays at clean `9712d43`; the dirty working tree is the post-`apply-patches.sh` state. `build-all.sh` and `apply-patches.sh` re-derive it on every fresh clone.

If a patch fails to apply after an upstream bump: manually merge the conflict, then regenerate the patch with `cd upstream && git diff > ../overlay/patches/0002-http-ui-bootstrap.patch`.

## Common Pitfalls

These will cost you hours if you don't know about them. Full list in [docs/部署-低配ECS一键脚本.md §7](./docs/部署-低配ECS一键脚本.md).

| Pitfall | What goes wrong | Fix |
|---------|-----------------|-----|
| Vite alias order | `vite.config.ts` `resolve.alias` is **prefix-matched in array order**. Generic `@` before `@/commands/fs` → HTTP mode silently falls back to Tauri, `bootstrapHttpProject()` returns null, search/chat 404 | Put specific overlay aliases first; use **array** form, not object spread. See `vite.config.ts` comment |
| `tsx` is devDep | `overlay/cli/node` drives the **ingest** subprocess via `node <tsx cli.mjs>`. `npm ci --omit=dev` → `Cannot find module 'tsx'`. (Chat used to need this too, but no longer — it's pure Rust now.) | Don't `--omit=dev` for `overlay/cli/node` (ingest only) |
| `upstream/node_modules` needed for ingest | `cmd-ingest.ts` imports `zustand` / `@milkdown/...` via `@/` paths. Node resolves `node_modules` upward, so install to `upstream/`, not `overlay/cli/node/` | Deploy script `npm ci --omit=dev` for `upstream/` (only needed if you run ingest on the remote) |
| protoc missing at build time | `cargo build` fails `Could not find protoc` — `lancedb` (now a `llm-wiki-common` dep) needs it via `prost-build` at compile time. Affects **every** server/CLI build (both link common) | `protoc --version` ≥ 3.21; on Debian `apt install protobuf-compiler`. Remote doesn't compile, so it's a dev-machine-only requirement |
| Port 8080 occupied (searxng etc.) | Server fails to start, no clear log | `ss -ltnp | grep 8080`; use `SERVER_PORT=8081` |
| SSH 22022 only | Aliyun ECS often closes 22, opens 22022 only | `SSH_PORT=22022` for `sync-artifacts.sh` / `deploy-ecs.sh`. Don't pollute global `~/.ssh/config`; use a per-host config fragment + `SSH_CONFIG=` env |
| `--delete` on `upstream/dist/` | Vite emits hashed chunk files; old ones pile up forever | `sync-artifacts.sh` uses `--delete` for `dist/` (intentional) |
| musl not actually static | `file` says `dynamically linked` or `ldd` lists glibc deps | Check `.cargo/config.toml` has `linker = "musl-gcc"` and `--target x86_64-unknown-linux-musl` is passed |
| `upstream/src` not on server | ingest subprocess fails to import `@/lib/llm-client` (only relevant if you run ingest on the remote; chat is unaffected) | `deploy-ecs.sh` rsyncs `upstream/src` (with package.json + tsconfig.json for tsx path resolution) |

## Quick Onboarding Checklist (fresh machine / fresh LLM session)

If you're reading this cold, verify in order:

- [ ] `git submodule status` — `upstream` should show `9712d43` (clean)
- [ ] `cargo --version && node --version && rustup target list --installed | grep musl` — all three present
- [ ] `protoc --version` — needs ≥ 3.21 (lancedb requirement)
- [ ] `./scripts/build-all.sh` — should build without errors; output in `upstream/dist/` and `overlay/*/target/release/`
- [ ] `./overlay/server/target/release/llm-wiki-server` and `./scripts/llm-wiki --help` — both run
- [ ] (Deploy context) `ssh -p 22022 root@47.103.39.152 'systemctl is-active llm-wiki-server'` — should print `active`
- [ ] (Deploy context) `curl -sS -H 'Authorization: Bearer minmax2.7' http://127.0.0.1:8081/api/v1/health` — should return `{"ok":true,...}`

If any of these fail, the corresponding `docs/` section is the next place to look.

## Related Documentation

- [README.md](README.md) — Project overview
- [文档指引.md](文档指引.md) — Global task-navigation entry (four modules)
- [docs/代码结构总览.md](docs/代码结构总览.md) — Detailed architecture diagrams
- [docs/开发与测试.md](docs/开发与测试.md) — Build, test, and FAQ
- [docs/上游同步.md](docs/上游同步.md) — Submodule sync principles
- [docs/ingest-flow-analysis.md](docs/ingest-flow-analysis.md) — Ingest call chain (Rust CLI → Node shim → upstream `autoIngest`)
- [overlay/eval/README.md](overlay/eval/README.md) — RAG eval system (three layers, test-case formats, commands)
- [overlay/static/lite/README.md](overlay/static/lite/README.md) — `/lite/` minimal QA page
- [docs/日常运维.md](docs/日常运维.md) — Daily operations
- [docs/邮件配置-SMTP-Resend.md](./docs/邮件配置-SMTP-Resend.md) — **SMTP email** (Resend signup, SPF/DKIM/DMARC, smtp config, troubleshooting)
- [docs/付款-Waffo-Pancake.md](./docs/付款-Waffo-Pancake.md) — **Waffo 付款** (Pro 订阅: Dashboard 准备、billing 配置、webhook、e2e、排错、开发经验与坑、上线检查清单)
- [docs/备份与恢复.md](./docs/备份与恢复.md) — **Backup & recovery** (auth DB hot backup, wiki rsync, restore drill)
- [docs/远端服务器ingest.md](./docs/远端服务器ingest.md) — **Remote ingest runbook** (do ingest on ECS, agent-friendly quick start)
- [docs/部署-低配ECS一键脚本.md](./docs/部署-低配ECS一键脚本.md) — **Low-spec ECS runbook** (deploy-ecs.sh / sync-artifacts.sh, pitfalls, ssh config)
- [docs/低配机交叉编译CLI.md](./docs/低配机交叉编译CLI.md) — musl cross-compile details
- [docs/部署实战与排错记录-2026-08-02.md](./docs/部署实战与排错记录-2026-08-02.md) — **部署实战**（双服务器、Resend 邮件、Ark LLM、免 sudo musl、排错速查）
- [docs/部署指引.md](./docs/部署指引.md) — Deployment options
- [docs/文档索引.md](./docs/文档索引.md) — Full doc index
