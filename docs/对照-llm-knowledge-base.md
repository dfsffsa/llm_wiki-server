# llm-knowledge-base 与 llm_wiki-server 对照

> 最后更新：2026-06-05  
> 对照仓库：`~/overseas-github/llm-knowledge-base`（Schema 1.1.0） vs 本集成仓库 + upstream **v0.4.20**

本文档记录对 sibling 项目 **llm-knowledge-base** 的审阅结论：哪些理念值得借鉴，哪些不必照搬。重点覆盖文本处理、向量索引、检索与大模型回复。

---

## 1. 两个项目分别是什么

| | llm-knowledge-base | llm_wiki / llm_wiki-server |
|---|-------------------|---------------------------|
| **性质** | Schema + `AGENTS.md` + 文档模板 | 桌面 App + headless server + CLI |
| **运行时** | 无；靠 Cursor/Claude 等外部 Agent 读写 vault | Tauri / HTTP / `llm-wiki` CLI |
| **检索默认** | Index-first（`_index`、`_concepts`、`_graph`） | 关键词 + 图谱 relevance + LanceDB RRF |
| **向量** | 可选（>500 篇时用 Chroma 预过滤文件名） | 内置 LanceDB + embedding 流水线 |
| **关系** | 方法论与 Karpathy LLM Wiki 工作流的 formalize | 其文档中称为「桌面 App 实现」 |

`llm-knowledge-base` **没有**可借鉴的 Rust/TS 检索引擎代码；价值在**工作流、质量规则与检索哲学**。

---

## 2. 文本处理（编译 / 入库）

### llm-knowledge-base

- `raw/` 只读 → LLM 编译为 `wiki/concepts/`、`summaries/`、`topics/`
- Frontmatter：`confidence`、`sources`、`related`、`tags`
- 三个索引文件：`_index.md`、`_concepts.md`、`_graph.md`（每次 write 后更新）
- **Sandbox 晋升**：默认写 `output/`、`learning/`；进入 `wiki/` 需质量达标或显式指令
- **`insights/`**：仅人类书写，Agent 永不写入（防污染）

### llm_wiki-server 现状

- 两步入库：analysis → generation（`upstream/src/lib/ingest.ts`）
- YAML frontmatter、wikilink、source traceability
- `schema.md` / `purpose.md` 规则层

### 建议借鉴

| 理念 | 落地建议 |
|------|----------|
| `confidence: high \| medium \| low \| speculative` | 扩展 wiki frontmatter；`speculative` 不进入图谱边集 |
| `insights/` 人类层 | 大项目中区分 Agent 编译页与人工洞见（或 Obsidian 双 vault） |
| 索引与实体页同步 | lint 检查 `wiki/index.md` 是否与 `entities/`、`concepts/` 一致 |
| 增量 compile（1–3 个 raw/次） | CLI/Agent 文档与 ingest 队列策略 |

---

## 3. 向量索引

### llm-knowledge-base

- **默认不用向量**（适合 50–2000 篇 curated 文档）
- 规模化 hybrid（文档建议，非内置）：

```text
向量索引 → 候选文章文件名 → 加载完整 wiki 文章 → LLM 推理
（不对 raw 做固定长度 chunk RAG）
```

### llm_wiki-server 现状

- upstream：LanceDB v2、`text-chunker.ts`、embedding 配置、RRF 融合
- headless server：**仅关键词**（混合向量待 port）
- CLI：`vector upsert-chunks` 等 LanceDB 子命令

### 建议借鉴 / 不借鉴

| 做法 | 结论 |
|------|------|
| 「整页加载再推理」而非 chunk 黑盒 | **采纳** — 与 upstream RAG 方向一致，server Chat 实现时优先 page/chunk 可解释性 |
| 完全放弃向量 | **不采纳** — CivilCareer 等 600+ 页项目需要 hybrid |
| Chroma 预过滤 | **参考思路** — 可用 LanceDB 命中 → 路径 → 读整页，不必另起栈 |

---

## 4. 文本检索

### llm-knowledge-base

```text
Query → 读 _index + _concepts → 选定文章路径 → 加载全文 → 回答并 cite 文件
```

- 透明、可审计；矛盾靠 lint 显式处理
- 小库可用 ripgrep

### llm_wiki-server 现状

| 信号 | 实现 |
|------|------|
| 关键词 | `llm-wiki-common` / server API / CLI |
| 图谱 | wikilink + Adamic-Adar + source overlap + type affinity |
| 向量 | LanceDB（桌面 + CLI；server 待接） |
| 融合 | RRF（upstream Rust） |

### 建议借鉴

| 理念 | 落地建议 |
|------|----------|
| Index-first 导航 | Chat RAG 前先读 `wiki/index.md`（及类型目录），再 load 页面，降 token、提高可解释性 |
| `learning/gaps.md` 式缺口列表 | 与现有 `lint.json` / review 合并：缺 concept、薄覆盖、矛盾 |
| 文件级 citation | HTTP search 已返回 path/title/snippet；server Chat 须保持每段可追溯到 wiki 路径 |

---

## 5. 大模型回复（Query / Chat）

### llm-knowledge-base

- 无固定 Chat UI；`AGENTS.md §7` 查询流程
- 答案写 `output/reports/`，可选 `filed_back: true` 回写 wiki
- Learning：flashcard + FSRS `_review.md` + `gaps.md`；Socratic 问答对照 wiki 打分
- 质量：`web-imputed` / `agent-inferred` 标记；矛盾 → `status: quarantined`

### llm_wiki-server 现状（2026-06-06）

- 桌面：`chat-panel.tsx` + `streamChat` + RAG（graph + vector）
- HTTP UI：**Chat 可用** — `overlay/server` SSE 代理 + React UI / Lite `/lite/`；RAG 在浏览器侧 keyword search + 服务端 LLM
- Rescan：HTTP 仍 501，请用 CLI `llm-wiki rescan`
- Lint / sweep：`upstream/src/lib/lint.ts` 等

### 建议借鉴

| 理念 | 说明 |
|------|------|
| 回答先进 `output/` 沙箱 | server Chat 可先写 report，用户确认再入库 |
| `filed_back` | 区分一次性问答 vs 沉淀进 wiki |
| Learning layer | CivilCareer 等场景可选 Phase 5：flashcard + 复习队列 |
| 污染隔离 | confidence 降级、quarantine 与 lint 集成 |

### 推荐实现路径（server Chat）— 已实现 MVP

1. ~~**Server Chat 代理**~~ ✅ `overlay/server/src/api/chat.rs` + `llm.rs`（reqwest 直连，不再走 Node `cmd-llm-stream.ts`）
2. ~~**RAG**~~ ✅ 关键词 + LanceDB 向量混合检索已接入 `/search`（RRF 融合，向量侧失败降级 keyword）
3. **Citation** — 强制引用 `wiki/...md`；可选 `output/reports/`
4. **持久化** — HTTP 模式 localStorage；桌面 `.llm-wiki/chats/`

详见 [开发与测试.md §Q5](./开发与测试.md#q5像桌面-llm_wiki-那样在页面上测试大模型-chat-怎么做)。

---

## 6. 总览：学什么、不学什么

```text
llm-knowledge-base              llm_wiki-server
────────────────────            ─────────────────────────
Schema / 质量规则 / lint 哲学  →  吸收到 schema、lint、文档
Index-first 查询               →  加强 Chat RAG 第一层
confidence / quarantine        →  frontmatter + lint 扩展
learning / gaps                →  可选新模块
insights 人类层               →  防污染、双目录或双 vault

默认零向量                     ✗  保留 LanceDB hybrid
无应用运行时                   ✗  已有 server/CLI/桌面
```

**一句话：** kb-base 强在**知识治理与查询哲学**；llm_wiki-server 强在**工程与 hybrid 检索**。优先嫁接 confidence、gap、insights 隔离与 index-first RAG，而不是替换现有向量栈。

---

## 7. 参考链接（kb-base 仓库内）

| 路径 | 主题 |
|------|------|
| `AGENTS.md` | 完整 Schema |
| `docs/why-not-rag.md` | 编译式 wiki vs RAG |
| `docs/learning-layer.md` | 闪卡、FSRS、gap |
| `docs/contamination-mitigation.md` | 双 vault、confidence、quarantine |
| `docs/项目架构说明.md` | 与 llm_wiki 的分工说明 |
| `examples/ai-alignment/` | 示例 vault 结构 |
