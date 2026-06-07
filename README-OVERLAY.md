# Overlay 开发指南

> 全局文档入口：[文档指引.md](文档指引.md) · 代码结构：[docs/代码结构总览.md](docs/代码结构总览.md)

## 目录说明

```
overlay/
├── server/       # Headless HTTP 服务（Rust）— Phase 1
├── cli/          # 命令行工具 — Phase 3
│   ├── rust/     # search, preprocess, reindex
│   └── node/     # ingest（包装 upstream ingest.ts）
├── web/          # HttpBackend、Vite 环境 — Phase 2
├── embedding/    # Embedding 探索与实验（不含密钥）
└── config/       # 配置样例
```

## Git 工作流

### 日常开发（只改 overlay）

```bash
git checkout -b overlay/server/my-feature
# 编辑 overlay/...
git add overlay/
git commit -m "feat(server): ..."
```

### 升级 upstream

完整原理与操作见 **[docs/上游同步.md](docs/上游同步.md)**。简要步骤：

```bash
cd upstream && git reset --hard && cd ..
./scripts/sync-upstream.sh v0.4.20   # 或 origin/main
./scripts/apply-patches.sh
./scripts/build-all.sh
git add upstream overlay/patches
git commit -m "chore: bump upstream to v0.4.20"
```

**禁止**在 `upstream/` 目录内提交定制代码。若必须修改上游文件：

1. 在 `overlay/patches/` 新增 patch
2. 更新 `scripts/apply-patches.sh`
3. 在 PR/文档中记录原因，便于 upstream 合并后删除 patch

### 克隆带子模块

```bash
git clone --recurse-submodules git@github.com:dfsffsa/llm_wiki-server.git
# 已克隆但未初始化子模块：
git submodule update --init --recursive
```

## 构建与运行（Phase 1）

```bash
# 1. 构建 upstream UI（可选，用于静态托管）
npm install --prefix upstream
npm run build --prefix upstream

# 2. 构建 headless server
cargo build --release --manifest-path overlay/server/Cargo.toml

# 3. 启动（示例）
export LLM_WIKI_PROJECT=/path/to/your-wiki-project
export LLM_WIKI_API_TOKEN=your-secret
export LLM_WIKI_BIND=127.0.0.1:8080
export LLM_WIKI_CONFIG=overlay/config/server.example.json
export LLM_WIKI_STATIC=upstream/dist   # 可选

./overlay/server/target/release/llm-wiki-server
```

健康检查：`curl http://127.0.0.1:8080/api/v1/health?token=your-secret`

Docker（**UI + Server 一体镜像**，非仅前端）：

```bash
export LLM_WIKI_PROJECT=/path/to/your-wiki-project
export LLM_WIKI_API_TOKEN=your-secret
docker compose -f docker/docker-compose.yml up --build
```

说明见 [docker/README.md](docker/README.md)（含 Chat/ingest 限制、`upstream/dist` 与本地起服对比）。

## CLI（Phase 3）

```bash
./scripts/build-cli.sh

export LLM_WIKI_PROJECT=/path/to/your-wiki-project
./scripts/llm-wiki search "query" --project "$LLM_WIKI_PROJECT"
./scripts/llm-wiki rescan --project "$LLM_WIKI_PROJECT" --json
./scripts/llm-wiki preprocess note.md -o /tmp/out.txt

# ingest / vector reindex need LLM config:
cp overlay/config/llm.example.json overlay/config/llm.json
# edit overlay/config/llm.json with API keys
export LLM_WIKI_CONFIG=overlay/config/llm.json
./scripts/llm-wiki reindex --vectors --project "$LLM_WIKI_PROJECT"
./scripts/llm-wiki ingest source.pdf --project "$LLM_WIKI_PROJECT"
```

See `overlay/cli/README.md` for command reference.

## 测试与 FAQ

完整说明见 **[docs/开发与测试.md](docs/开发与测试.md)**，包括：

- Phase 0–4 进度与 Git 提交记录
- `./scripts/e2e-local.sh` / `e2e-docker.sh` / `e2e-full.sh` 用法
- 常见问题：提交规范、Docker/WSL、CLI search、**HTTP 页 vs 桌面版 LLM Chat**

快速回归：

```bash
./scripts/e2e-full.sh
# 或仅本地 HTTP（无需 Docker）
./scripts/e2e-local.sh
```

## Embedding 实验

见 `overlay/embedding/README.md`。勿提交含 API Key 的脚本；使用 `test_embedding.example.py` 与环境变量。
