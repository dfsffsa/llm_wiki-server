# Docker 部署说明

> 最后更新：2026-06-06

本目录提供 **HTTP 版 llm-wiki-server** 的容器化打包与 Compose 编排。  
**不是**桌面 Tauri 应用，**不是**前端开发服务器（`npm run dev`），**不包含** CLI 入库工具链。

---

## 容器里跑的是什么？

**一个容器 = Rust 后端 + 已构建的 Web 静态文件**，不是「只跑前端」：

```text
浏览器  →  http://host:8080
┌────────────────────────────────────────┐
│  llm-wiki-server 容器                   │
│  ├─ llm-wiki-server（Rust）             │  API：搜索、读文件、图谱…
│  └─ /app/dist（来自 upstream/dist）     │  完整 React UI + /lite/
└────────────────────────────────────────┘
         ↑ 卷挂载
宿主机 Wiki 项目目录  →  /data/wiki
```

| 组件 | 在容器内 | 说明 |
|------|----------|------|
| Web UI | ✅ `/app/dist` | 构建阶段 `npm run build` 的产物，由 server **静态托管** |
| API Server | ✅ `llm-wiki-server` | 与本地 `LLM_WIKI_STATIC=upstream/dist` 起服相同 |
| Wiki 数据 | 卷挂载 | 宿主机项目目录，**不进镜像** |
| CLI ingest | ❌ | 在宿主机跑 `./scripts/llm-wiki ingest` |
| Node.js 运行时 | ❌（当前镜像） | Chat SSE 依赖 `npx tsx`，见 [已知限制](#已知限制) |

---

## 文件说明

| 文件 | 作用 |
|------|------|
| [`Dockerfile`](Dockerfile) | 三阶段构建：UI → Rust server → debian-slim 运行时 |
| [`docker-compose.yml`](docker-compose.yml) | 端口、环境变量、wiki 卷、配置挂载 |

### Dockerfile 三阶段

1. **ui-builder**（`node:22`）— `apply-patches.sh` + `build-web.sh` → `upstream/dist`
2. **server-builder**（`rust:1`）— `cargo build --release` → `llm-wiki-server`
3. **runtime**（`debian:bookworm-slim`）— 仅拷贝 `dist` + 二进制，`EXPOSE 8080`

---

## 快速启动

```bash
export LLM_WIKI_PROJECT=~/overseas-github/llm_wiki_projects/CivilCareer
export LLM_WIKI_API_TOKEN=e2e-test-token

docker compose -f docker/docker-compose.yml up --build
# 浏览器 http://127.0.0.1:8080/  或  /lite/
```

Compose 默认：

- 端口：`8080`（可用 `LLM_WIKI_PORT` 改）
- Wiki：`${LLM_WIKI_PROJECT}` → 容器内 `/data/wiki`
- 配置：`overlay/config/server.example.json` → `/etc/llm-wiki/config.json`（只读）

含真实 LLM 密钥时，复制并编辑配置后改 compose 挂载路径，或使用 `*.local.json`（勿提交 Git，见 [overlay/config/README.md](../overlay/config/README.md)）。

---

## 与本地直接运行的关系

| | 本地 | Docker |
|---|------|--------|
| 命令 | `./overlay/server/target/release/llm-wiki-server` | `docker compose up` |
| 静态 UI | `LLM_WIKI_STATIC=upstream/dist` | 镜像内 `/app/dist`（构建时打入） |
| Wiki | `LLM_WIKI_PROJECT=...` | 卷挂载到 `/data/wiki` |
| 改 UI 后 | 重新 `build-web.sh` + 重启 server | 重新 `docker compose up --build` |

逻辑等价：**同一套 server + dist**，Docker 只是把依赖打进镜像，便于交付与 E2E。

---

## 算不算「轻量级部署」？

**相对桌面 Tauri：是。** 单进程 headless 服务，适合 NAS / 内网 Linux，无需 GUI。

**相对「只挂 Nginx 静态页」：构建不轻、运行较轻。**

- 首次 `docker build` 需拉 `node`、`rust` 基础镜像，耗时较长
- 运行期通常只有一个 Rust 进程 + 静态文件，内存占用不大
- 适合：**已有 wiki 项目、只想对外提供 HTTP 浏览/搜索** 的场景

---

## 已知限制

1. **Chat 可能不可用** — 运行时镜像**未安装 Node.js**，而 `POST /chat` 会 spawn `npx tsx cmd-llm-stream.ts`。浏览 wiki、搜索、图谱通常正常；需要 Chat 时请 **本机直接跑 server**（本机需安装 Node/npx），或自行在 Dockerfile runtime 阶段加 Node。
2. **不含 ingest** — 批量入库在宿主机执行 `scripts/llm-wiki` / `ingest-batch.sh`，再把 wiki 目录挂进容器。
3. **WSL + Docker Desktop** — 卷路径需 `//wsl.localhost/...`；请用 [`scripts/e2e-docker.sh`](../scripts/e2e-docker.sh) 而非裸跑 `docker.exe`。

---

## E2E 测试

```bash
./scripts/e2e-docker.sh
# 或全链路（含可选 Docker 段）
./scripts/e2e-full.sh
```

---

## 相关文档

- [`upstream/dist` 是什么](../docs/代码结构总览.md#121-upstreamdist-构建产物) — 构建产物目录说明
- [开发与测试.md §4.4](../docs/开发与测试.md#44-docker)
- [架构与改造方案.md](../docs/架构与改造方案.md) — Phase 1 Docker Compose
