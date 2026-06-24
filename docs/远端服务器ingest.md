# 远端服务器 ingest runbook

> 适用：希望在**阿里云 ECS (47.103.39.152)** 上做 wiki ingest，而不是在本地。
> 上下文：这个仓库的默认工作流是"本地 ingest + rsync wiki 数据到远端"（见 [CLAUDE.md](../CLAUDE.md) + [日常运维.md §7](./日常运维.md)）。本 runbook 覆盖另一个场景——**直接在远端跑 ingest**。
> 关键参考：[CLAUDE.md](../CLAUDE.md) · [部署-低配ECS一键脚本.md](./部署-低配ECS一键脚本.md) · [日常运维.md](./日常运维.md)

---

## 1. 决策：什么时候在远端做 ingest

| 场景 | 推荐 |
|------|------|
| 源材料在本地，新增/补 ingest | **本地 ingest** → rsync `wiki/` 到远端（默认） |
| 源材料已经在远端（比如爬虫写到了远端磁盘） | **远端 ingest** |
| 想用远端 LLM 端点（API 在云上） | **远端 ingest** |
| 远端机器 1.6 GB RAM 想 ingest | 远端 ingest **可以**（用预编译的 musl 静态 CLI，不要在远端 cargo build） |

远端 ingest 的本质是：**让 Rust CLI 进程在远端跑，调用 tsx 跑 TypeScript，调远端 LLM API，写 `wiki/` 到远端磁盘**。HTTP server 读这个 `wiki/` 目录对外提供只读浏览 + 搜索 + chat。

---

## 2. 远端已就位（不需要重做）

首次部署后这些都已在远端（[部署-低配ECS一键脚本.md §5.2](./部署-低配ECS一键脚本.md) 一次跑完）：

| 路径 | 是什么 |
|------|--------|
| `/root/llm_wiki-server/overlay/cli/rust/target/release/llm-wiki` | CLI 二进制（50 MB, musl static-pie） |
| `/root/llm_wiki-server/scripts/llm-wiki` | bash wrapper（364 B，自动设 `LLM_WIKI_REPO`） |
| `/root/llm_wiki-server/overlay/config/server.local.json` | 含真实 `LLM_API_KEY`，chmod 600（`PLACEHOLDER_FILL_ON_SERVER` 已被 sed 替换） |
| `/root/llm_wiki-server/overlay/cli/node/node_modules/tsx` | tsx 运行时（devDep，但 `npm ci` 没省） |
| `/root/llm_wiki-server/upstream/node_modules/` | zustand / milkdown 等（**ingest** 子进程要；chat 已是纯 Rust，不需要） |
| `/root/llm_wiki_projects/<项目>/` | wiki 数据，每个项目有 `wiki/` `raw/sources/` `purpose.md` 等 |

> **不要重传这些**。如果怀疑缺失，跑 `sync-artifacts.sh`（首次可能漏传）或 `deploy-ecs.sh`（重置）。

---

## 3. 三步上手

### 3.1 上传新源材料（如果有）

源材料放 `raw/sources/`，ingest 时按 `wiki/sources/<同名>.md` 是否存在决定 SKIP 或处理。

```bash
SSHHOME=/home/ab/cross-device-syncer/ssh-tunnels
SSH="$SSHHOME/ecs99-connect-22022.sh"

# 单文件 scp
scp -P 22022 ~/path/to/new-source.md root@47.103.39.152:/root/llm_wiki_projects/ParentingBooks/raw/sources/

# 整个目录 rsync
rsync -avz --progress -e "ssh -p 22022" \
  ~/overseas-github/llm_wiki_projects/ParentingBooks/raw/sources/ \
  root@47.103.39.152:/root/llm_wiki_projects/ParentingBooks/raw/sources/
```

文件名建议**保留中文 / 人类可读**（`01-辅食添加.md`），不强制。

### 3.2 跑 ingest

```bash
SSH="ssh -p 22022 root@47.103.39.152"

# 单文件
"$SSH" 'cd /root/llm_wiki-server && \
  LLM_WIKI_REPO=$PWD \
  ./scripts/llm-wiki ingest \
    /root/llm_wiki_projects/ParentingBooks/raw/sources/<source>.md \
    --project /root/llm_wiki_projects/ParentingBooks \
    --config /root/llm_wiki-server/overlay/config/server.local.json'

# 批量（扫整个 raw/sources/，已入库 SKIP）
"$SSH" 'cd /root/llm_wiki-server && \
  LLM_WIKI_REPO=$PWD \
  ./scripts/ingest-batch.sh /root/llm_wiki_projects/ParentingBooks'
```

> **必须设 `LLM_WIKI_REPO=/root/llm_wiki-server`**。CLI 用它定位 `overlay/cli/node/src/cmd-ingest.ts`；不设会回退到 `CARGO_MANIFEST_DIR`，那是构建机路径（`/home/ab/...`），远端报 `Node ingest script not found`。

**成功的标志**（在 ssh 命令的 stdout 末尾）：

```
[ingest] project=/root/llm_wiki_projects/ParentingBooks
[ingest] source=/root/llm_wiki_projects/ParentingBooks/raw/sources/<source>.md
[ingest] model=MiniMax-M2.7
[ingest] done — N wiki file(s) written
  /root/llm_wiki_projects/ParentingBooks/wiki/sources/<source>.md
  /root/llm_wiki_projects/ParentingBooks/wiki/concepts/...
  ...
```

N 一般 3–8（每篇会拆 sources + 几个 concepts/entities/lessons）。

### 3.3 验证

```bash
SSH="ssh -p 22022 root@47.103.39.152"

# 1) 文件确实写入了
"$SSH" 'ls -lh /root/llm_wiki_projects/ParentingBooks/wiki/sources/ | tail -5'

# 2) HTTP 搜索能命中（先重启 server，让它扫新文件——一般不需要，只读读盘）
"$SSH" "curl -sS -H 'Authorization: Bearer minmax2.7' \
  -H 'Content-Type: application/json' \
  -X POST -d '{\"query\":\"<关键词>\",\"top_k\":3}' \
  http://127.0.0.1:8081/api/v1/projects/<project-id>/search | python3 -m json.tool | head -30"

# project-id 从这里拿
"$SSH" "curl -sS -H 'Authorization: Bearer minmax2.7' \
  http://127.0.0.1:8081/api/v1/projects | python3 -c 'import json,sys; d=json.load(sys.stdin); [print(p[\"id\"],p[\"name\"]) for p in d[\"projects\"]]'"
```

> HTTP server 是只读 + 按需重扫 wiki 目录，**正常情况下 ingest 后不需要 restart**。如果搜索不到刚写入的，重启一次：`systemctl restart llm-wiki-server`。

---

## 4. 配置文件详解

`/root/llm_wiki-server/overlay/config/server.local.json`：

```json
{
  "projects": [
    { "path": "/root/llm_wiki_projects" },
    { "path": "/root/llm_wiki_projects/ParentingBooks" },
    { "path": "/root/llm_wiki_projects/CivilCareer" }
  ],
  "apiConfig": { "enabled": true, "token": "minmax2.7", ... },
  "llmConfig": {
    "provider": "custom",
    "model": "MiniMax-M2.7",
    "customEndpoint": "https://api.minimaxi.com/anthropic",
    "apiKey": "sk-cp-...真实密钥...已注入...",
    ...
  },
  "embeddingConfig": { ... }
}
```

| 字段 | 作用 | ingest 用到吗？ |
|------|------|----------------|
| `projects[].path` | HTTP server 列出项目；ingest 不直接读（用 `--project`） | ❌ |
| `llmConfig.model` | LLM 模型名 | ✅ |
| `llmConfig.apiKey` | LLM 密钥 | ✅（已注入） |
| `llmConfig.customEndpoint` | LLM API endpoint（Anthropic / OpenAI 兼容） | ✅ |
| `embeddingConfig` | 向量嵌入（reindex --vectors 用） | ❌（ingest 不依赖） |

**要换 LLM**：改 `llmConfig` 三件套（model / apiKey / customEndpoint），**不需要重启**——CLI 直接读 JSON。**要换 token**：改 `apiConfig.token` + 同时改 `VITE_API_TOKEN` 重新构建 UI（HTTP server token 改了，UI bundle 不变会出现 401）。

---

## 5. 常见问题速查

| 现象 | 原因 | 处理 |
|------|------|------|
| `Node ingest script not found` | `LLM_WIKI_REPO` 未设 | ssh 时前缀 `cd /root/llm_wiki-server && LLM_WIKI_REPO=$PWD` |
| `Config required for ingest` | 缺 `--config` 或 `LLM_WIKI_CONFIG` | 加 `--config /root/llm_wiki-server/overlay/config/server.local.json` |
| `Source file not found` | 路径错或没传 | `ls` 确认文件存在；绝对路径 |
| `Failed to run Node ingest (is Node/npx installed?)` | Node 没装或 PATH 错 | `which node; node -v`，应 ≥ 20 |
| `Cannot find module 'tsx'` | `overlay/cli/node/node_modules` 缺失 | `cd /root/llm_wiki-server/overlay/cli/node && npm ci` |
| `Cannot find package 'zustand'` | `upstream/node_modules` 缺失 | `cd /root/llm_wiki-server/upstream && npm ci --omit=dev` |
| `401 Unauthorized` on /api/v1/search | UI bundle 的 token 与 server 不一致 | 重新 `VITE_API_TOKEN=... ./scripts/build-web.sh` + `sync-artifacts.sh` |
| LLM 调用 4xx/5xx | endpoint 错 / apiKey 过期 | 查 `llmConfig`；手动 `curl` 试 endpoint |
| ingest 报"already exists" | `wiki/sources/<同名>.md` 已有 | 删旧的 `wiki/sources/<同名>` 再 ingest，或加 `--force`（如有） |
| **想做向量嵌入** | `reindex --vectors`（CLI 命令） | `./scripts/llm-wiki reindex --vectors --project ... --config ...`（首次较慢） |
| **想换 wiki 项目** | ParentingBooks → CivilCareer | 改 `--project` 路径；HTTP 端多项目时用 `/api/v1/projects/{id}/...` |

---

## 6. 远端 vs 本地 ingest：性能 / 行为差异

| 维度 | 本地 | 远端 |
|------|------|------|
| 源材料传输 | 不需要 | 上传 markdown 到远端（rsync） |
| LLM 调用延迟 | 取决于本机→LLM 链路 | 取决于远端→LLM 链路（通常更短） |
| `wiki/` 写入位置 | 本地 | 远端 |
| 同步到 HTTP server | **必须** rsync `wiki/` | 自动（同一台机器） |
| CLI 启动开销 | 本地 50ms | 远端 ~1s（tsx 冷启 + Node 22） |
| 调试 | 直接看本地 stdout | ssh + 看 stdout |

> 远端 LLM 调用延迟通常更短（如果 API 也在云上），是主要优势。调试麻烦（要 ssh 进去）是主要劣势。

---

## 7. 一键 ingest 脚本（可选）

如果经常远端 ingest，可把这段存成本地脚本（`scripts/remote-ingest.sh`，不一定要入仓）：

```bash
#!/usr/bin/env bash
# ./scripts/remote-ingest.sh <project-relative-path> <source-filename>
# 例: ./scripts/remote-ingest.sh ParentingBooks 01-辅食添加.md
set -euo pipefail
SSHHOME=/home/ab/cross-device-syncer/ssh-tunnels
SSH="$SSHHOME/ecs99-connect-22022.sh"
PROJECT="${1:?project name, e.g. ParentingBooks}"
SOURCE="${2:?source filename, e.g. 01-辅食添加.md}"

REMOTE_PROJECT="/root/llm_wiki_projects/${PROJECT}"
REMOTE_SOURCE="${REMOTE_PROJECT}/raw/sources/${SOURCE}"

# 1) 传源材料
LOCAL_SOURCE="$HOME/overseas-github/llm_wiki_projects/${PROJECT}/raw/sources/${SOURCE}"
if [[ -f "$LOCAL_SOURCE" ]]; then
  scp -P 22022 "$LOCAL_SOURCE" "root@47.103.39.152:${REMOTE_SOURCE}"
fi

# 2) 跑 ingest
"$SSH" "cd /root/llm_wiki-server && \
  LLM_WIKI_REPO=\$PWD \
  ./scripts/llm-wiki ingest '${REMOTE_SOURCE}' \
    --project '${REMOTE_PROJECT}' \
    --config /root/llm_wiki-server/overlay/config/server.local.json"
```

调用：

```bash
./scripts/remote-ingest.sh ParentingBooks 01-辅食添加.md
```

---

## 8. 相关文档

| 文档 | 何时读 |
|------|--------|
| [CLAUDE.md](../CLAUDE.md) | 第一次接触这个项目；理解架构、产物、部署模型 |
| [部署-低配ECS一键脚本.md](./部署-低配ECS一键脚本.md) | 第一次部署；远端路径/端口/坑位 |
| [日常运维.md §3](./日常运维.md) | ingest 行为详解（SKIP 规则、批处理、reindex） |
| [代码结构总览.md §7.4](./代码结构总览.md) | `autoIngest` 内部阶段（preprocess / extract / write） |
| [overlay/cli/README.md](../overlay/cli/README.md) | CLI 子命令参数 |
| [overlay/config/README.md](../overlay/config/README.md) | server.local.json 字段 |
