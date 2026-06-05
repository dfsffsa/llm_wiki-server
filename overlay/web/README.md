# Web platform adapter (Phase 2)

将 `upstream` 前端从 Tauri `invoke()` 切换为 HTTP API（只读浏览）。

## 架构

```
upstream/vite.config.ts   ← patch：VITE_BACKEND=http 时 alias 到 overlay/web/*
overlay/web/
├── backend-client.ts     ← HTTP 客户端
├── commands/fs.ts        ← 只读文件 API
├── commands/file-sync.ts ← 空实现
├── lib/search.ts         ← POST /search
└── lib/project-store.ts  ← localStorage
```

## 构建

```bash
./scripts/apply-patches.sh   # App.tsx + vite.config.ts + bootstrap stub

export VITE_API_TOKEN=your-secret   # 与 LLM_WIKI_API_TOKEN 一致
./scripts/build-web.sh
# 输出: upstream/dist
```

完整栈（UI + server）：

```bash
./scripts/build-all.sh
LLM_WIKI_PROJECT=/path/to/wiki LLM_WIKI_API_TOKEN=your-secret \
  LLM_WIKI_STATIC=upstream/dist \
  ./overlay/server/target/release/llm-wiki-server
```

## 环境变量（构建时）

| 变量 | 说明 |
|------|------|
| `VITE_BACKEND=http` | 由 `build-web.sh` 自动设置 |
| `VITE_API_TOKEN` | 嵌入 bundle 的 API token |
| `VITE_API_BASE` | 可选 API 根 URL（默认同源） |

## 只读限制

写入、入库、Chat、文件同步等在 HTTP 模式下不可用。浏览 wiki、搜索、图谱可用。

**大模型 Chat：** HTTP 模式不能像桌面版那样在页面上完整使用 Chat/RAG，见 [docs/DEVELOPMENT_AND_TESTING.md §6](../docs/DEVELOPMENT_AND_TESTING.md#6-常见问题与答复faq)。

## 开发

```bash
# 终端 1：headless server
LLM_WIKI_PROJECT=... LLM_WIKI_API_TOKEN=... LLM_WIKI_BIND=127.0.0.1:8080 \
  cargo run --manifest-path overlay/server/Cargo.toml

# 终端 2：Vite dev（proxy /api → 8080）
cd upstream
VITE_BACKEND=http VITE_API_TOKEN=... npm run dev
```
