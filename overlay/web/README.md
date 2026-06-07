# Web platform adapter (Phase 2)

将 `upstream` 前端从 Tauri `invoke()` 切换为 HTTP API（只读浏览 + Chat RAG）。

## 架构

```
upstream/vite.config.ts   ← VITE_BACKEND=http 时 alias 到 overlay/web/*（overlay 别名须在 @ 之前）
overlay/web/
├── env.ts                ← isHttpBackend、apiToken（构建时嵌入）
├── backend-client.ts     ← HTTP 客户端（projects / files / search / chat SSE）
├── path-helpers.ts       ← 项目路径、只读断言
├── commands/fs.ts        ← 只读文件 API + bootstrapHttpProject
├── commands/file-sync.ts ← 空实现
├── lib/search.ts         ← POST /search
├── lib/llm-client.ts     ← streamChat → POST /chat（服务端 LLM 代理）
├── lib/persist.ts        ← Chat 历史 localStorage
└── lib/project-store.ts  ← 其余设置 localStorage
```

服务端 Chat 代理：`overlay/server/src/api/chat.rs` → `overlay/cli/node/src/cmd-llm-stream.ts`。

## 构建

```bash
# 若 upstream 尚未打过 HTTP patch（App.tsx 自动打开 server 项目）
./scripts/apply-patches.sh
# 若提示 patch 冲突但 upstream 已含相同改动，可跳过，直接构建

export VITE_API_TOKEN=your-secret   # 必须与 LLM_WIKI_API_TOKEN 一致
VITE_BACKEND=http VITE_API_TOKEN=your-secret ./scripts/build-web.sh
# 输出: upstream/dist（Vite 静态构建产物，见 docs/代码结构总览.md §12.1）
```

**构建后自检（避免页面停在欢迎屏、看不到 Chat）：**

```bash
# token 应出现在 bundle 中
grep -r "your-secret" upstream/dist/assets/ | head -1

# 应命中 overlay 的 HTTP 客户端，而非 Tauri invoke
grep -l "runtime-config\|chatStream" upstream/dist/assets/*.js
```

完整栈（UI + server）：

```bash
./scripts/build-all.sh
LLM_WIKI_PROJECT=/path/to/wiki LLM_WIKI_API_TOKEN=your-secret \
  LLM_WIKI_STATIC=upstream/dist \
  LLM_WIKI_CONFIG=overlay/config/server.example.json \
  ./overlay/server/target/release/llm-wiki-server
```

## 环境变量（构建时）

| 变量 | 说明 |
|------|------|
| `VITE_BACKEND=http` | 由 `build-web.sh` 自动设置 |
| `VITE_API_TOKEN` | **构建时**嵌入 bundle；与运行时 `LLM_WIKI_API_TOKEN` 必须一致 |
| `VITE_API_BASE` | 可选 API 根 URL（默认同源） |

修改 token 或 overlay 代码后，必须**重新** `build-web.sh`，并在浏览器 **Ctrl+Shift+R** 强刷。

## 页面布局与 Chat

HTTP 模式**没有**单独的「Chat」侧边栏图标。打开项目后：

| 区域 | 内容 |
|------|------|
| 最左竖条 | 视图切换（Wiki / Sources / Search / Graph …） |
| 左栏 | 文件树 + Activity |
| **中间** | **Chat 面板**（`activeView === "wiki"` 时默认显示） |
| 右栏 | 选中文件的预览（可选） |

首次使用：中间 Chat 左侧对话列表为空时，点击 **「新建对话」** 开始提问。

若看到的是 **「打开项目 / 创建项目」欢迎页**，说明 HTTP 项目未自动加载（见下方排错）。

## 只读限制

写入、入库、文件同步等在 HTTP 模式下不可用。浏览 wiki、搜索、图谱、**Chat（RAG）** 可用。

**Chat：** LLM 经 `POST /api/v1/projects/{id}/chat` 服务端代理，需 `LLM_WIKI_CONFIG` 含 `llmConfig`（及 `LLM_API_KEY` 等环境变量）。前端 RAG 与桌面版相同；`streamChat` 走 `overlay/web/lib/llm-client.ts`。聊天历史存 **localStorage**（非 `.llm-wiki/chats/`）。

```bash
curl "http://127.0.0.1:8080/api/v1/runtime-config?token=$LLM_WIKI_API_TOKEN"
# chatEnabled: true → 可对话
```

见 [docs/开发与测试.md §Q5–Q7](../docs/开发与测试.md#q5像桌面-llm_wiki-那样在页面上测试大模型-chat-怎么做)。

## 排错：看不到 Chat / 只有欢迎页

| 现象 | 原因 | 处理 |
|------|------|------|
| 欢迎屏（Open Project） | `bootstrapHttpProject()` 未拿到 server 项目 | 检查 server 是否在跑、`LLM_WIKI_PROJECT` 是否有效 |
| API 401 | 构建时未设 `VITE_API_TOKEN` 或与 server 不一致 | 带 token 重建 UI，强刷浏览器 |
| 中间空白 / 无对话 | 未新建会话 | 点击 Chat 左栏 **「新建对话」** |
| Chat 无回复 | `chatEnabled: false` 或未配 LLM | 配置 `LLM_WIKI_CONFIG` + `LLM_API_KEY`，查 `runtime-config` |
| 改了 overlay 仍旧行为 | 用了未带 HTTP alias 的旧 dist | 重新 `build-web.sh`，强刷缓存 |

**vite 别名：** `upstream/vite.config.ts` 中 overlay 路径（如 `@/commands/fs`）必须列在通用 `@` **之前**，否则构建会误用上游 Tauri 版 `fs.ts`（其中 `bootstrapHttpProject` 恒返回 `null`）。

## 开发

```bash
# 终端 1：headless server
LLM_WIKI_PROJECT=... LLM_WIKI_API_TOKEN=... LLM_WIKI_BIND=127.0.0.1:8080 \
  LLM_WIKI_CONFIG=overlay/config/server.example.json \
  cargo run --manifest-path overlay/server/Cargo.toml

# 终端 2：Vite dev（proxy /api → 8080）
cd upstream
VITE_BACKEND=http VITE_API_TOKEN=... npm run dev
```
