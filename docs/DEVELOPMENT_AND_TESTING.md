# 开发进度、测试与常见问题

> 最后更新：2026-06-05  
> 主仓库：`llm_wiki-server`（集成 overlay + upstream submodule）  
> 测试用真实 wiki 项目示例：`~/overseas-github/llm_wiki_projects/CivilCareer`

本文档汇总集成开发进度、构建/调试/测试方法，以及近期对话中的常见问题与答复。

---

## 1. 开发进度（Phase 0–4）

| 阶段 | 状态 | 说明 |
|------|------|------|
| **Phase 0** Git 与仓库结构 | ✅ | submodule、overlay 骨架、`sync-upstream.sh` |
| **Phase 1** Headless Server | ✅ | Rust HTTP 服务、静态 UI、Dockerfile、关键词搜索 API |
| **Phase 2** Web 只读 | ✅ | `overlay/web` HTTP 适配、`0002-http-ui-bootstrap.patch` |
| **Phase 3** CLI | ✅ | Rust CLI + Node ingest/reindex、`llm-wiki-common` 共享 crate |
| **Phase 4** 上游同步 | ✅ | upstream **v0.4.20**，patch 已针对 v0.4.20 重生 |

### 主要 Git 提交（main 线）

| Commit | 说明 |
|--------|------|
| `9fc8a33` | 完成 overlay 集成（server / web / CLI / common）并 bump upstream v0.4.20 |
| `b257fce` | 修复中文搜索 snippet UTF-8 panic；新增 `e2e-docker.sh` / `e2e-local.sh` |
| `9499542` | 新增全链路脚本 `e2e-full.sh` |

### 尚未实现 / 已知限制

| 项目 | 说明 |
|------|------|
| HTTP UI **Chat + RAG** | 桌面版前端直连 LLM；headless `POST /chat` 仍为 **501** |
| HTTP UI **写入** | fs 适配只读；聊天记录、入库、Save to Wiki 不可用 |
| Server **混合向量搜索** | 仅关键词；LanceDB RRF 待后续抽取 |
| CLI **PDF/Office 预处理** | 需桌面 pdfium 或 `--copy-fallback` |
| **Docker 镜像构建** | 本机若 Docker Hub 拉取失败，需 `docker login` 或配置镜像源 |
| **WSL + docker.exe** | 需 `--env-file` + `//wsl.localhost/...` 路径（见 `e2e-docker.sh`） |

---

## 2. 环境与一次性准备

```bash
cd ~/overseas-github/llm_wiki-server
git submodule update --init --recursive

# Node（建议 nvm）
source ~/.nvm/nvm.sh

# Rust（cargo 已安装则跳过）
# protoc：build-cli.sh 会自动下载到 .tools/protoc（LanceDB 依赖）
```

### 环境变量速查

| 变量 | 用途 |
|------|------|
| `LLM_WIKI_PROJECT` | Wiki 项目根目录（含 `wiki/`、`raw/`） |
| `LLM_WIKI_API_TOKEN` | HTTP API 鉴权 token |
| `LLM_WIKI_BIND` | 监听地址，如 `127.0.0.1:8080` |
| `LLM_WIKI_STATIC` | 静态 UI 目录，通常 `upstream/dist` |
| `LLM_WIKI_CONFIG` | 服务端 JSON（`overlay/config/server.example.json`） |
| `LLM_WIKI_REPO` | 由 `./scripts/llm-wiki` 自动设置 |
| `VITE_BACKEND=http` | 构建 HTTP 只读 UI 时由 `build-web.sh` 设置 |
| `VITE_API_TOKEN` | 构建 UI 时嵌入 bundle，需与 `LLM_WIKI_API_TOKEN` 一致 |

---

## 3. 构建

```bash
# 应用 patch（构建 HTTP UI 前必须）
./scripts/apply-patches.sh

# 一键：HTTP UI + server + CLI
./scripts/build-all.sh

# 分项
VITE_BACKEND=http VITE_API_TOKEN=your-token ./scripts/build-web.sh
cargo build --release --manifest-path overlay/server/Cargo.toml
./scripts/build-cli.sh
```

**注意：** 修改 `llm-wiki-common` 后需**同时**重建 server 与 CLI，否则 CLI 可能仍含旧逻辑（例如 search panic）。

---

## 4. 运行

### 4.1 Headless Server + 浏览器（HTTP 只读）

```bash
export LLM_WIKI_PROJECT=~/overseas-github/llm_wiki_projects/CivilCareer
export LLM_WIKI_API_TOKEN=e2e-test-token
export LLM_WIKI_BIND=127.0.0.1:8080
export LLM_WIKI_STATIC=~/overseas-github/llm_wiki-server/upstream/dist
export LLM_WIKI_CONFIG=~/overseas-github/llm_wiki-server/overlay/config/server.example.json

./overlay/server/target/release/llm-wiki-server
# 浏览器打开 http://127.0.0.1:8080/
```

健康检查：

```bash
curl "http://127.0.0.1:8080/api/v1/health?token=e2e-test-token"
```

### 4.2 CLI

```bash
export LLM_WIKI_PROJECT=~/overseas-github/llm_wiki_projects/CivilCareer

./scripts/llm-wiki search "职场" --project "$LLM_WIKI_PROJECT" --top-k 10
./scripts/llm-wiki rescan --project "$LLM_WIKI_PROJECT" --json
./scripts/llm-wiki preprocess note.md -o /tmp/out.txt

# ingest / 向量重建需 LLM 配置
cp overlay/config/llm.example.json overlay/config/llm.json
# 编辑 apiKey、model、endpoint
export LLM_WIKI_CONFIG=overlay/config/llm.json
./scripts/llm-wiki reindex --vectors --project "$LLM_WIKI_PROJECT"
./scripts/llm-wiki ingest source.pdf --project "$LLM_WIKI_PROJECT"
```

CLI 参数使用 **kebab-case**，例如 `--top-k`（不是 `--top_k`）。

### 4.3 桌面版（完整 LLM Chat / 入库 / 写入）

与官方 `llm_wiki` 体验一致：

```bash
cd upstream
npm install
npm run tauri dev
```

在 Settings 配置 LLM → Chat 面板对话 → RAG 检索 wiki。

### 4.4 Docker

```bash
export LLM_WIKI_PROJECT=~/overseas-github/llm_wiki_projects/CivilCareer
export LLM_WIKI_API_TOKEN=e2e-test-token
docker compose -f docker/docker-compose.yml up --build
```

WSL 下若 `docker` 命令不可用，使用 Docker Desktop 的 `docker.exe`；全链路脚本 `e2e-docker.sh` 已处理 env-file 与卷路径。

---

## 5. 测试脚本

| 脚本 | 作用 |
|------|------|
| `./scripts/e2e-local.sh` | 本地 headless：health / projects / search / 静态 UI（**无需 Docker**） |
| `./scripts/e2e-docker.sh` | Docker Compose 部署 + 同上 API 检查 |
| `./scripts/e2e-full.sh` | **全链路**：patch → mock 测试 → 构建 → CLI → HTTP API → 可选 Docker |

```bash
# 推荐日常回归
./scripts/e2e-full.sh

# 仅本地 HTTP（最快）
./scripts/e2e-local.sh

# upstream 单元测试
npm run test:mocks --prefix upstream    # 1236 tests，无真实 LLM
npm run test:llm --prefix upstream      # 需 API key，见 load-test-env
cargo test --manifest-path overlay/crates/llm-wiki-common/Cargo.toml
```

### 全链路测试结果（2026-06-05，CivilCareer）

- **passed: 23**（构建、CLI、HTTP API、静态 UI）
- **skipped: 1**（Docker Hub 拉取失败时跳过 Docker 段）
- 修复项：中文 search snippet UTF-8 边界；CLI 需 rebuild 才能带上该修复

---

## 6. 常见问题与答复（FAQ）

### Q1：如何继续 Phase 2 / Phase 3 / Phase 4？

**答：** 均已在本仓库完成（见 §1）。Phase 4 将 upstream 升至 **v0.4.20** 并重生 `overlay/patches/0002-http-ui-bootstrap.patch`。后续升级：

```bash
./scripts/sync-upstream.sh vX.Y.Z
./scripts/apply-patches.sh
# patch 冲突时手动合并 upstream 文件后：
# cd upstream && git diff > ../overlay/patches/0002-http-ui-bootstrap.patch
./scripts/build-all.sh
```

---

### Q2：如何提交并推送改动？

**答：** 按 Git 安全流程：只提交 `overlay/`、`scripts/`、`docs/`，**不要**提交 `upstream/` 内 patch 脏工作区；submodule 仅更新指针到干净 tag。

```bash
cd upstream && git reset --hard v0.4.20 && cd ..
git add overlay/ scripts/ docs/ upstream
git commit -m "your message"
git push origin main
```

`upstream/` 的 HTTP patch 在**构建时**由 `./scripts/apply-patches.sh` 应用，不写入 submodule commit。

---

### Q3：如何做端到端验证（Docker + HTTP UI + 真实 wiki）？

**答：**

1. **本地（推荐）：** `./scripts/e2e-local.sh` 或 `./scripts/e2e-full.sh`
2. **Docker：** `./scripts/e2e-docker.sh`（需 Docker Desktop 运行且能拉取 `node`/`rust`/`debian` 镜像）
3. **手动：** 启动 server（§4.1）+ 浏览器访问 CivilCareer 项目

HTTP 模式下可验证：**浏览 wiki、关键词搜索、知识图谱**。Chat / 入库不在此模式。

---

### Q4：`llm-wiki search "职场"` 返回什么？

**答：** 对 CivilCareer 项目（`--top-k 10`）示例：

```
mode: keyword (token hits: 10)
 1. [256.0] wiki/insights/ren-xing-ben-e-lun.md — 人性本恶论：职场人际关系的黑暗森林
 2. [256.0] wiki/sources/laoa-职场回忆录-015.md — 职场回忆录第十五章
 3. [256.0] wiki/sources/laoa-职场回忆录-041.md — 老A职场回忆录·第四十一章：...
 ...
10. [201.0] wiki/index.md — Wiki Index
```

JSON 输出：`./scripts/llm-wiki search "职场" --project "$LLM_WIKI_PROJECT" --json`

标题/文件名含查询词的条目分数更高（如 256）；正文匹配次之（如 216）。

---

### Q5：像桌面 llm_wiki 那样，在**页面上**测试大模型 Chat 怎么做？

**答：** **HTTP 部署（http://127.0.0.1:8080）目前不能像桌面版那样完整使用 Chat + RAG。**

| 能力 | HTTP UI | 桌面 `tauri dev` |
|------|---------|------------------|
| 浏览 / 搜索 / 图谱 | ✅ | ✅ |
| Chat 流式对话 | ❌（501 + 只读 fs + CORS） | ✅ |
| Settings 测试 LLM 连接 | ⚠️ 浏览器 CORS 可能失败 | ✅ |
| 入库 / 写 wiki | ❌ | ✅ |

**要在页面上体验大模型，请用桌面版：**

```bash
cd upstream && npm run tauri dev
```

**测 LLM 入库流水线（非页面 Chat）：**

```bash
export LLM_WIKI_CONFIG=overlay/config/llm.json   # 含真实 apiKey
./scripts/llm-wiki ingest your-source.pdf --project "$LLM_WIKI_PROJECT"
```

**自动化 real-LLM 测试（开发者）：**

```bash
cd upstream && npm run test:llm
```

**若未来要在 HTTP 页支持 Chat，需：** server 端 Chat 代理（绕 CORS）、允许写 `.llm-wiki/chats/` 或改用 localStorage。

---

### Q6：调试技巧

| 现象 | 处理 |
|------|------|
| CLI search panic（中文） | 重建 CLI：`cargo build --release --manifest-path overlay/cli/rust/Cargo.toml` |
| `GET /files` 返回 413 | 大项目 listing 超过 `maxFiles`；用 `recursive=false` 或增大 `maxFiles` |
| Docker compose 报 `LLM_WIKI_PROJECT` 缺失 | WSL 下用 `e2e-docker.sh` 的 env-file，勿裸跑 `docker.exe` |
| patch 不 apply | `cd upstream && git reset --hard v0.4.20 && ../scripts/apply-patches.sh` |
| UI 调 API 401 | 构建 UI 时设置 `VITE_API_TOKEN` 与 server 的 `LLM_WIKI_API_TOKEN` 一致 |

日志：

```bash
# e2e-local 后台 server 日志
/tmp/llm-wiki-e2e.log

# 手动启动时可前台看 stderr
RUST_BACKTRACE=1 ./overlay/server/target/release/llm-wiki-server
```

---

## 7. 相关文档

| 文档 | 内容 |
|------|------|
| [ARCHITECTURE.md](./ARCHITECTURE.md) | 架构与分阶段方案 |
| [README-OVERLAY.md](../README-OVERLAY.md) | Overlay 开发与构建速查 |
| [overlay/cli/README.md](../overlay/cli/README.md) | CLI 命令参考 |
| [overlay/web/README.md](../overlay/web/README.md) | HTTP 只读 UI 适配 |
| [overlay/patches/README.md](../overlay/patches/README.md) | Patch 说明 |
| [COMPARISON_LLM_KNOWLEDGE_BASE.md](./COMPARISON_LLM_KNOWLEDGE_BASE.md) | 与 llm-knowledge-base 对照与借鉴建议 |
