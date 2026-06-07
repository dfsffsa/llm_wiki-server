# Lite Chat UI — Design Spec

> **Status:** Implemented (2026-06-06) — UX polish + Markdown ongoing  
> **Date:** 2026-06-06  
> **URL:** `http://127.0.0.1:8080/lite/`（与完整版 `/` 共存）

## 1. 目标

为**非专业人士**提供极简问答页：选主题 → 对话 → 看历史。不暴露文件树、设置、图谱、入库等能力。

**对话主题** = `~/overseas-github/llm_wiki_projects/` 下**已完成 Ingest** 的 wiki 项目。当前：

| 目录 | 展示名 | 视觉主题（方案 C） |
|------|--------|-------------------|
| `ParentingBooks` | 育儿百科 | 暖色：奶油底、珊瑚/蜜桃强调、圆角柔和 |
| `CivilCareer` | 职场进阶 | 冷色：浅灰底、蓝灰/藏青强调、偏干练 |

借鉴产品交互（非照搬视觉）：

- **ChatGPT** — 首页大卡片选「助手/主题」
- **豆包 / 通义** — 对话区气泡 + 底部建议问题芯片
- **微信** — 历史会话列表、返回上一级

## 2. 非目标（YAGNI）

- 不替代完整版 React UI（`/`` 保留）
- 不做用户账号、云端同步历史
- 不做 Settings、LLM 配置 UI（沿用 server `LLM_WIKI_CONFIG`）
- 不做向量搜索配置 UI（lite 使用关键词 search + chat 代理）
- 不做 Ingest / 写 wiki

## 3. 架构

```
overlay/static/lite/          ← 源码（纯静态）
  index.html
  app.css
  app.js                      ← ES module 入口
  markdown.js                 ← marked + DOMPurify
  vendor/
    marked.esm.js
    purify.es.js              ← 勿用 .mjs（MIME 问题，见 §14）
  projects.meta.json          ← 展示元数据（中文名、配色、建议问题）
  config.js                   ← API token / base（构建或部署时注入）

upstream/dist/lite/           ← 构建输出（copy 进 dist）

llm-wiki-server
  GET  /lite/                 ← 静态页
  GET  /api/v1/projects       ← 多项目列表（需扩展）
  POST /api/v1/projects/{id}/search
  POST /api/v1/projects/{id}/chat   ← SSE
  GET  /api/v1/runtime-config
```

**资源占用：** 无 React/Vite 运行时；总计约 30–80 KB（gzip 后更小）。仅对话时占用 server 既有 Chat 子进程。

## 4. Server 改动（多项目注册）

### 4.1 问题

当前 `LLM_WIKI_PROJECT` 只挂载一个目录；`load_projects()` 在 headless 模式下通常只返回该项。Lite 页需要同时列出 **ParentingBooks** 与 **CivilCareer**，且 Chat/Search 能按 `projectId` 路由到正确路径。

### 4.2 方案

在 server 配置 JSON 增加可选字段 `projects`（路径列表）。解析逻辑：

1. 若配置了 `projects[]`：逐项 canonicalize，读取 `.llm-wiki/project.json` 得 `id`（无则生成/用路径 hash）
2. 始终保证 `LLM_WIKI_PROJECT` 在列表中且标记 `current: true`（兼容旧行为）
3. `resolve_project(id)` 在合并后的列表中查找

环境变量备选（实现时二选一或并存）：

```bash
LLM_WIKI_PROJECTS=/path/ParentingBooks,/path/CivilCareer
```

示例 `overlay/config/server.example.json` 扩展（本地含密钥副本可用 `server.minimax.local.json`，见 `.gitignore` 的 `*.local.json`）：

```json
{
  "projects": [
    { "path": "/home/li/overseas-github/llm_wiki_projects/ParentingBooks" },
    { "path": "/home/li/overseas-github/llm_wiki_projects/CivilCareer" }
  ],
  "llmConfig": { "...": "..." }
}
```

### 4.3 校验

- 路径必须含 `wiki/` 且存在 `purpose.md` 或 `wiki/index.md`（表示已初始化）
- Ingest 未完成的项目仍可列出，卡片角标显示「内容更新中」（根据 `wiki/sources/` 数量或可选 `ingestStatus` 字段）

## 5. 前端结构（单页两视图）

### 5.1 视图 A — 选主题（`#view-home`）

- 顶部：产品名 + 一句 slogan（「选一个话题，开始提问」）
- 网格：每个项目一张**大卡片**
  - 图标 emoji（👶 / 💼，来自 `projects.meta.json`）
  - 中文标题、副标题（1 行）
  - 卡片背景/边框使用该项目主题色（方案 C）
- 点击卡片 → 写入 `sessionStorage.activeProjectId` → 切到视图 B

### 5.2 视图 B — 对话（`#view-chat`）

- 顶栏：返回按钮、项目中文名、主题色顶栏
- 主体：消息气泡列表（用户右对齐、助手左对齐）
- 底部：多行输入框 + 发送；上方 **建议问题芯片**（来自 meta，点击即发送）
- 侧栏/抽屉：**历史对话**（本项目下 localStorage 会话列表）
  - 新建对话、切换会话、删除会话

### 5.3 主题色（方案 C）

| Token | ParentingBooks | CivilCareer |
|-------|----------------|-------------|
| `--bg` | `#FFF8F3` | `#F4F6F9` |
| `--accent` | `#E8836B` | `#3D5A80` |
| `--accent-soft` | `#FFE4D6` | `#D6E4F0` |
| `--text` | `#3D2C29` | `#1E293B` |
| `--bubble-user` | `#E8836B` | `#3D5A80` |
| `--bubble-ai` | `#FFFFFF` | `#FFFFFF` |

通过 `document.documentElement.dataset.theme = projectKey` 切换 CSS 变量。

## 6. `projects.meta.json`

静态文件，key 为项目目录名（`ParentingBooks`），**不**含密钥：

```json
{
  "ParentingBooks": {
    "title": "育儿百科",
    "subtitle": "喂养、护理、发育与常见病",
    "emoji": "👶",
    "theme": "parenting",
    "starters": ["宝宝几个月添加辅食？", "吐奶怎么办？", "纯母乳需要补维生素D吗？"]
  },
  "CivilCareer": {
    "title": "职场进阶",
    "subtitle": "公务员职场经验与避坑指南",
    "emoji": "💼",
    "theme": "career",
    "starters": ["新人怎么融入单位？", "和领导相处要注意什么？", "哪些事是职场红线？"]
  }
}
```

首页渲染：`GET /projects` 与 meta 按 `path` 末尾目录名 merge；API 有而 meta 无则用 `name` 兜底。

## 7. 对话与 RAG 数据流

与完整版 `ChatPanel` 对齐的**轻量客户端 RAG**（纯 JS，无 bundler）：

1. 用户发送 `text`
2. `POST /api/v1/projects/{id}/search` `{ query: text, topK: 8, includeContent: true }`
3. 取 top 结果拼成 context 块（path + snippet/content）
4. 组装 messages：
   - system：你是{title}助手，仅基于以下资料回答…
   - 可选：注入 context
   - history：当前会话 prior turns
   - user：text
5. `POST /api/v1/projects/{id}/chat` + `Accept: text/event-stream`
6. 解析 SSE：`token` / `reasoning` / `done` / `error`（与 `overlay/web/lib/llm-client.ts` 一致）
7. 流式追加到助手气泡

**认证：** `config.js` 或构建时注入 `API_TOKEN`，请求头 `Authorization: Bearer …` + query `?token=`（与现有 `backend-client` 一致）。

## 8. 历史对话（localStorage）

Key：`llm-wiki-lite:{projectId}`

```json
{
  "conversations": [
    { "id": "uuid", "title": "宝宝吐奶", "updatedAt": 1717680000 }
  ],
  "messages": {
    "uuid": [
      { "role": "user", "content": "...", "ts": 1717680000 },
      { "role": "assistant", "content": "...", "ts": 1717680001 }
    ]
  }
}
```

- 首条用户消息前 20 字作为会话 title
- 最多保留 50 会话 / 项目（超出删最旧）
- 无跨设备同步

## 9. 错误与空状态

| 情况 | UI |
|------|-----|
| `runtime-config.chatEnabled === false` | 首页顶部黄条：「问答暂不可用，请联系管理员」 |
| search 无结果 | 仍发 chat，system 注明「未检索到相关页面」 |
| chat 401 | 「链接已失效，请刷新页面」 |
| chat 5xx / 断流 | 气泡内显示错误 + 重试按钮 |
| 项目 wiki 为空 | 卡片灰显 +「内容准备中」 |

## 10. 构建与部署

```bash
# 复制 lite 静态资源到 dist（新脚本或扩展现有 build-web.sh）
cp -r overlay/static/lite upstream/dist/lite

# server 启动（多项目 + 静态）
LLM_WIKI_PROJECT=.../ParentingBooks   # current 默认项
LLM_WIKI_STATIC=upstream/dist
LLM_WIKI_CONFIG=overlay/config/server.example.json  # 含 projects[]；密钥可用 *.local.json
```

访问：

- 完整版：`http://127.0.0.1:8080/`
- 轻量版：`http://127.0.0.1:8080/lite/`

## 11. 测试计划

1. **静态：** 无 JS 时 HTML 可读；Lighthouse 移动友好
2. **API：** `curl /projects` 返回 2 项；分别对两 id `search` + `chat` 成功
3. **E2E 手动：** 选育儿 → 发「吐奶」→ 有流式回复；切换职场 → 历史隔离；刷新后会话仍在
4. **回归：** 完整版 `/` 行为不变

## 12. 实现顺序（供 writing-plans 拆分）

1. Server：`projects[]` 配置 + `load_projects` 合并
2. `overlay/static/lite/` 骨架 + `projects.meta.json`
3. `app.js`：项目列表、主题切换、chat+RAG、SSE、localStorage
4. `scripts/build-web.sh` 或 `copy-lite-static.sh` 复制到 `dist/lite`
5. 文档：`overlay/static/lite/README.md` + `NEW_PROJECT_GUIDE` 链接

## 13. 开放问题（实现前默认）

- **默认进入：** 打开 `/lite/` 总是先显示选主题页（不记住上次项目）；可选后续加「记住上次」
- **ParentingBooks Ingest：** 批量进行中；lite 可先上线，随 ingest 增加回答质量提升
- **API Token：** 与完整版相同，构建 lite 时写入 `config.js`（勿提交含真实 token 的文件到 Git）

---

**请审阅本 spec。** 确认或修改后，将进入 `writing-plans` 产出分步实现计划。

---

## 14. 实现记录（2026-06-06）

### 14.1 已完成

| 项 | 说明 |
|----|------|
| Server 多项目 | `projects[]` / `LLM_WIKI_PROJECTS`；`GET /api/v1/projects` |
| 静态页骨架 | `overlay/static/lite/`；`/lite/` 子目录 `index.html` 服务 |
| 布局 | 左侧历史侧栏（默认可见）；用户/助手气泡分行 |
| RAG + SSE | `search` → `chat` 流式；取消流时过滤 `AbortError` |
| 流式 UX | 阶段文案「正在检索资料…」「正在生成回答…」；120s 超时；`finally` 清理状态 |
| Markdown | `marked` + `DOMPurify`；流式节流重渲染（~120ms） |
| 构建 | `build-web.sh` 复制 lite + vendor 到 `dist/lite/` |
| 文档 | `overlay/static/lite/README.md`；`DEVELOPMENT_AND_TESTING` §Q8 |

### 14.2 已知限制

- 无 KaTeX / Mermaid / wikilink 跳转（完整版 React UI 具备）
- 历史仅存浏览器 `localStorage`，无跨设备同步
- 默认每次打开 `/lite/` 先显示选主题页（不记住上次项目）

### 14.3 排错备忘

- **首页无项目卡片：** ES module 链断裂（DOMPurify 须为 `purify.es.js`；勿在 `vendor/` 保留 `.mjs` 副本）
- **强刷：** 修改静态资源后 `Ctrl+Shift+R`
- **Token：** `dist/lite/config.js` 与 `LLM_WIKI_API_TOKEN` 一致
