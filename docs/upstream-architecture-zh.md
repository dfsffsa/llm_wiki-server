# LLM Wiki 上游架构说明（桌面版）

> 从本地 `llm_wiki` 工作区迁移。描述 **upstream 桌面应用** 的逻辑架构，便于理解 overlay 改造边界。  
> 原文基于仓库 **0.4.14** 调研编写；本集成仓库 upstream submodule 当前为 **v0.4.16**。  
> 姊妹项目：**llm-knowledge-base**（Schema 规范，无 GUI）。

---

## 1. 项目定位

**llm_wiki** 是 [Karpathy LLM Wiki 模式](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 的**跨平台桌面应用实现**（Tauri v2 + React）。

核心差异于传统 RAG：

- 不是「每次提问从 raw 文档临时 chunk 检索」。
- 而是 **Ingest 阶段由 LLM 增量编译持久化 wiki**，Query 阶段在 wiki 上检索 + 组装 context + Chat。

### 1.1 三大核心操作（与 Karpathy 原文一致）

| 操作 | 含义 |
|------|------|
| **Ingest** | 原始文件 → LLM 分析/生成 → 写入 `wiki/` |
| **Query** | 用户提问 → 检索 wiki 页 → LLM 生成带引用的回答 |
| **Lint** | 结构/语义健康检查（orphan、broken link、矛盾等） |

### 1.2 相对 Schema 规范的增强

见 upstream `README.md`「What We Changed & Added」，主要包括：

- 桌面 GUI、ingest 队列、两步 CoT ingest、知识图谱、Louvain 社区、Graph Insights
- 可选 **混合检索**（关键词 + LanceDB 向量 + RRF）
- Deep Research、Review 队列、Chrome Web Clipper、本地 HTTP API（`:19828`）

---

## 2. 总体技术架构

```text
┌─────────────────────────────────────────────────────────────────┐
│                     React 19 前端 (Vite)                         │
│  Wiki / Sources / Search / Graph / Chat / Lint / Review / Settings│
└────────────────────────────┬────────────────────────────────────┘
                             │ Tauri invoke()
┌────────────────────────────▼────────────────────────────────────┐
│                   Rust 后端 (src-tauri/)                         │
│  fs · search · vectorstore · project · file_sync · api_server   │
│  clip_server(19827) · pdf/office extract · panic_guard          │
└────────────────────────────┬────────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        ▼                    ▼                    ▼
  项目目录 (wiki/)      LanceDB 向量索引      外部 HTTP
  raw/ + .llm-wiki/     (.llm-wiki/ 下)      LLM / Embedding / Web Search
```

| 层级 | 技术 |
|------|------|
| 桌面壳 | Tauri v2 |
| 前端 | React 19 + TypeScript + Vite + shadcn/ui + Tailwind v4 |
| 状态 | Zustand（`src/stores/`） |
| 编辑器 | Milkdown |
| 图谱 UI | sigma.js + graphology + ForceAtlas2 |
| 向量库 | LanceDB（Rust，embedded，可选） |
| LLM 调用 | 前端 `streamChat`（`src/lib/llm-client.ts`），经 tauri-plugin-http 规避 CORS |
| 本地 API | tiny_http，`127.0.0.1:19828` |
| Clip API | `127.0.0.1:19827`（Chrome 扩展） |

---

## 3. 单个 Wiki Project 的目录结构

创建项目时（`commands/project.rs` → `create_project`）会生成：

```text
<project-root>/
├── purpose.md
├── schema.md
├── raw/sources/               # 原始材料（immutable）
├── raw/assets/
├── wiki/                      # LLM 生成的知识页
│   ├── index.md, log.md, overview.md
│   ├── entities/, concepts/, sources/, queries/, ...
├── .obsidian/
└── .llm-wiki/                 # App 私有状态
    ├── project.json
    ├── ingest-queue.json
    ├── chats/*.json
    └── lancedb/                 # 启用 embedding 时
```

---

## 4. 核心调用链（摘要）

### Ingest

`ingest-queue.ts` → `autoIngest()`（`ingest.ts`）：缓存检查 → preprocess → 可选 caption → Analysis LLM → Generation LLM → 写 wiki → 可选 vector upsert。

### Query / Chat

`searchWiki` → `graph-relevance` 扩展 → context budget → `streamChat`。

### 本地 HTTP API（19828）

| 端点 | 作用 |
|------|------|
| `GET /api/v1/health` | 健康检查 |
| `GET /api/v1/projects` | 项目列表 |
| `GET .../files`, `.../files/content` | 读 wiki |
| `POST .../search` | hybrid 检索 |
| `GET .../graph` | wikilink 图 |
| `POST .../sources/rescan` | 重扫源目录 |

---

## 5. 与 overlay 改造的关系

| 能力 | 桌面 upstream | llm_wiki-server overlay 目标 |
|------|---------------|------------------------------|
| 浏览 wiki | GUI | Web + HTTP |
| Ingest | GUI + 队列 | **CLI** |
| 搜索 | GUI + Rust | HTTP API + **CLI** |
| Chat | WebView TS | 暂不迁移（API 501） |

完整改造计划见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

---

## 6. 与 hint_analysis_compass 的预期差距

| 维度 | llm_wiki | hint_analysis_compass |
|------|----------|------------------------|
| 知识单元 | Markdown wiki 页 | JSON scenario/script/principle |
| 检索 | 页级 hybrid IR | 分类 + 卡片路由 |
| 回答 | 自由 Chat + citation | 固定 skeleton + used_cards |
| 高风险 | 无硬分流 | risk_rule 强制 |

推荐：**llm_wiki 编译 + 导出 JSON** → 下游问答引擎。

---

## 7. 相关链接

| 资源 | URL |
|------|-----|
| 官方仓库 | https://github.com/nashsu/llm_wiki |
| Karpathy 原文 | https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f |
| Agent Skill | https://github.com/nashsu/llm_wiki_skill |

*本文档为团队调研时编写，已迁入 llm_wiki-server 仅作参考。*
