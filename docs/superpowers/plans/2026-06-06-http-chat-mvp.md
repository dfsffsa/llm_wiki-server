# HTTP Chat MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:executing-plans (or subagent-driven-development) task-by-task.

**Goal:** Enable RAG Chat on the HTTP read-only UI by proxying LLM calls through `llm-wiki-server` (fixes CORS + keeps API keys server-side).

**Architecture:** Browser runs existing `ChatPanel` RAG (search/read via HTTP APIs). `streamChat` is aliased to `overlay/web/lib/llm-client.ts`, which POSTs to `/api/v1/projects/{id}/chat` (SSE). Server spawns `cmd-llm-stream.ts` (reuses upstream `streamChat` + `LLM_WIKI_CONFIG`). Chat history uses localStorage.

**Tech Stack:** Rust `tiny_http` SSE passthrough, Node `tsx`, upstream TypeScript `streamChat`.

---

### Task 1: Node LLM stream script

**Files:**
- Create: `overlay/cli/node/src/cmd-llm-stream.ts`

- Read JSON stdin: `{ "messages": ChatMessage[] }`
- Load `--config` via `loadConfigFile` + `hydrateStoresFromConfig`
- Call `streamChat`, write SSE to stdout: `data: {"event":"token","data":"..."}\n\n`
- Events: `token`, `reasoning`, `done`, `error`

### Task 2: Server chat + runtime-config API

**Files:**
- Create: `overlay/server/src/api/chat.rs`
- Create: `overlay/server/src/api/runtime.rs`
- Modify: `overlay/server/src/api/mod.rs`
- Modify: `overlay/server/src/server.rs` (SSE response path)
- Modify: `overlay/server/src/state.rs` (`config_path()`)

- `GET /api/v1/runtime-config` — sanitized `llmConfig` (no apiKey), `chatEnabled`
- `POST /api/v1/projects/{id}/chat` — spawn node, pipe SSE stdout to client
- Require `LLM_WIKI_CONFIG` with valid `llmConfig`

### Task 3: HTTP frontend adapters

**Files:**
- Create: `overlay/web/lib/llm-client.ts`
- Create: `overlay/web/lib/persist.ts` (chat → localStorage)
- Modify: `overlay/web/backend-client.ts` (`getRuntimeConfig`)
- Modify: `overlay/web/commands/fs.ts` (bootstrap loads runtime config)
- Modify: `upstream/vite.config.ts` (aliases)
- Modify: `overlay/patches/0002-http-ui-bootstrap.patch`

### Task 4: Docs + verify

- Update `docs/新项目指引.md`, `overlay/web/README.md`, `开发与测试.md` Q5
- Rebuild server + web, manual smoke test

---

## 交付说明（2026-06-06）

### 构建要点

- `VITE_API_TOKEN` 须在 **构建时** 传入，并与运行时 `LLM_WIKI_API_TOKEN` 一致。
- `upstream/vite.config.ts` 中 overlay 别名（`@/commands/fs` 等）必须排在通用 `@` **之前**，否则 HTTP 构建误用 Tauri 版 `fs.ts`，`bootstrapHttpProject()` 返回 `null`，页面停在欢迎屏。
- `overlay/web/path-helpers.ts`、`backend-client.ts` 使用 `./env`、`./backend-client` 等同目录相对路径（非 `../`）。

### 用户可见界面

- Chat 为中间主区域（Wiki 视图），无单独侧边栏图标；空列表时点「新建对话」。
- 用户文档：[overlay/web/README.md](../../../overlay/web/README.md)、[开发与测试.md §Q5–Q7](../../开发与测试.md)、[新项目指引.md §5.3 / §6.1 / Q6](../../新项目指引.md)。
