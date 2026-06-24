# 向量搜索接入 + Chat 去 Node 化

> 设计文档（2026-06-23）。对应实施：HTTP `/search` 接入 LanceDB 混合检索；`/chat` 用 Rust reqwest 直连 LLM，移除每次请求 spawn Node tsx 子进程。

## 1. 背景

两件事来自商用评估（见 `docs/可行性评估.md`、根目录评估结论）：

1. **向量搜索未接入 HTTP**：`overlay/server/src/api/search.rs` 对 `queryEmbedding` 直接返回 501，对外 API 只有纯关键词搜索。RAG 知识库产品的核心卖点缺失。
2. **Chat 每请求 spawn 一个 Node tsx 进程**：`overlay/server/src/api/chat.rs` 每次 chat 都 `Command::new(node).arg(tsx_cli)`，冷启动慢、并发天花板低（`MAX_CONCURRENT_CHAT=8`）、远端要背 482MB `node_modules`、且代码大量篇幅在对抗 pipe/stderr 死锁。Chat 本质只是转发一个 LLM HTTP 端点，没必要走 Node。

## 2. 目标与非目标

### 目标

- HTTP `/search` 支持**混合检索**（关键词 + 向量），向量为可选增强项。
- **向量侧任何失败都不让系统崩**：未配置 embedding、无 LanceDB 表、embed 接口不可达、维度不匹配……一律降级为纯关键词，HTTP 正常返回结果。
- `/chat` 用 Rust reqwest 直连 LLM，SSE 线格式与前端契约**完全不变**（`overlay/web/lib/llm-client.ts` 无需改动）。
- `<think>` 思考过程分割、`reasoning_content` 提取、配额/并发槽、客户端断连杀上游等行为保持一致。

### 非目标

- 不重写 ingest（仍走 Node TS，因上游 `ingest.ts` 深度耦合 Zustand）。
- Chat **只支持 OpenAI 兼容（`chat_completions`）线路**。`apiMode: "anthropic_messages"`、`minimax`、`azure` 等其他线路暂不支持，服务端返回明确错误提示用户切回 OpenAI 兼容或仍用桌面端。
- 不动 CLI 的 ingest/reindex Node 包装。
- 不引入 axum/tower（那是更大的商用化改造，见根目录评估 §三.3，本次不做）。

## 3. 降级边界（关键决策）

用户明确：**"如果没有配置向量检索，回退到关键字检索，同样可以返回结果"**。

实现成"向量侧全部容错"：

| 向量侧失败情形 | 行为 |
|---|---|
| 配置无 `embeddingConfig` 或 `enabled=false` | 纯关键词，`mode:"keyword"` |
| 有配置但无 LanceDB 表 / `.llm-wiki/lancedb` 不存在 | 纯关键词，`vector_hits:0`，`eprintln!` 告警 |
| embed 接口不可达 / 401 / 超时 | 纯关键词 |
| 向量维度与表中不符 / 查询报错 | 纯关键词 |
| 向量有结果 | RRF 融合，`mode:"hybrid"` |

**关键词侧失败**（磁盘读错误、`wiki/` 不存在）保持现状向上 `Err` → HTTP 500。理由：这类是运维级故障，掩盖反而危险；向量侧才是"可选增强"，才该吞。

## 4. 架构

### 4.1 crate 依赖变更

```
llm-wiki-common  ← 新增: lancedb, arrow-array, arrow-schema, futures
   ↑                ← 新增模块: vector.rs (从 cli 迁入), search/vector.rs
   │
llm-wiki-server  ← 新增: reqwest (rustls + json + stream), futures
   │                ← 新增: llm.rs (embed_query + stream_chat)
   │                ← ServerState 新增: Arc<tokio::runtime::Runtime>
   │
llm-wiki-cli     ← 移除: lancedb, arrow-*, futures (改用 common::vector)
```

`vector.rs` 从 `overlay/cli/rust/src/` 迁到 `overlay/crates/llm-wiki-common/src/`，CLI 改为 `use llm_wiki_common::vector`。这样 server 和 cli 共享同一份 LanceDB 读写代码。

### 4.2 搜索数据流

```
POST /api/v1/projects/{id}/search  { query, topK?, includeContent? }
        │
        ├─ resolve_project → project.path
        ├─ 关键词检索 (common::search_keyword)            ← 永远跑，Result
        ├─ 读 embeddingConfig (来自 ServerState.load_app_state)
        ├─ 若有配置 → server::llm::embed_query(query)     ← 失败则记日志、跳过向量
        ├─ 若拿到 embedding → common::search_vector(...)  ← 失败则记日志、跳过
        ├─ 两边都有 → common::hybrid::merge_rrf(...)      ← 纯函数
        └─ 响应 { mode:"hybrid"|"keyword", results, tokenHits, vectorHits }
```

`page_id` → wiki 相对路径映射：ingest 时 `pageId = 文件名去 .md`（`upstream/src/lib/ingest.ts:975`）。向量命中返回 `page_id`，common 侧 `walk wiki/` 建 `page_id → relpath` 索引，拼出 `path/title/snippet`（snippet 从命中的 `chunk_text` 截取）。

### 4.3 Chat 数据流（去 Node）

```
POST /api/v1/projects/{id}/chat  { messages:[...] }
   ├─ authorize (cookie/bearer) + 配额 + 并发槽         ← 与现状一致
   ├─ 读 llmConfig；apiMode != chat_completions → 400 明确错误
   ├─ 构造 OpenAI 兼容 body { model, messages, stream:true }
   ├─ reqwest POST customEndpoint (+ /chat/completions)  stream
   └─ server::llm::stream_chat 逐行解析 SSE:
        data: {"choices":[{"delta":{"content":"...","reasoning_content":"..."}}]}
        → 路由进 <think> 分割器 → 写 SSE 帧 data:{"event":"token|reasoning","data":{"token"}}
        → [DONE] → flush → done 帧
```

SSE 输出帧格式（**与 Node 版完全一致**，`overlay/web/lib/llm-client.ts:90-103` 解析）：

```
data: {"event":"token","data":{"token":"..."}}

data: {"event":"reasoning","data":{"token":"..."}}

data: {"event":"done","data":{}}

data: {"event":"error","data":{"message":"..."}}
```

## 5. 关键设计点

### 5.1 RRF 融合（纯函数，TDD）

`common::hybrid::merge_rrf(keyword_results, vector_results, top_k)`：

- 每条结果按 `path`（关键词）或 `page_id`（向量）归一化去重。
- RRF score = `1/(60 + rank_keyword) + 1/(60 + rank_vector)`（k=60，标准 RRF）。
- 向量结果无关键词命中时，仍以纯向量分纳入（`score` 来自 RRF，`vector_score` 保留原 `_distance` 转换）。
- 输出按 RRF score 降序，截 `top_k`。

### 5.2 `<think>` 分割器（纯状态机，TDD）

移植 `overlay/cli/node/src/cmd-llm-stream.ts:63-116` 的 hold-back 缓冲逻辑到 Rust：

- 状态 `think_mode: bool` + `holdback: String`。
- `route_content(token)`: 在 holdback 里找 `<think>`/`</think>`，安全长度前缀立即 emit，跨 token 的 tag 片段保留。
- `flush()`: 流结束时把残余 buffer 按当前 mode emit。

### 5.3 tokio runtime 共享

Server 主线程是 tiny_http 同步循环。HTTP handler 里要 `block_on` 异步的向量查询/embed/chat-stream。在 `main.rs` 启动时建一个 `Arc<tokio::runtime::Runtime>`（multi-thread），存进 `ServerState`，所有 handler 复用。**不为每个请求建 runtime**（建销毁开销大，且 chat 长流会占满阻塞线程池——chat 用独立线程 + `runtime.spawn` + channel 回写，见 5.4）。

### 5.4 Chat 流式回写

chat 是长连接 SSE，不能 `block_on`（会占住 tiny_http worker 线程数十秒）。方案：

```
worker 线程:
  ├─ 发起 reqwest 请求 (runtime.block_on 拿到 streaming Response)
  ├─ spawn 一个 task 在 runtime 上读 SSE 流 → 解析 → 推入 mpsc channel
  └─ worker 线程从 channel 读 → 写 HTTP chunked 帧 → flush（每帧）
  断连: HTTP write 失败 → drop channel sender → task 取消 → reqwest drop → 上游连接关
```

保留原 `MAX_CONCURRENT_CHAT=8` 槽 + 配额逻辑不变。

### 5.5 embed_query 失败处理

`server::llm::embed_query(cfg, text) -> Result<Vec<f32>, String>`。调用方（search handler）对 `Err` 的处理是**记日志 + 跳过向量**，不传播成 HTTP 错误。

## 6. 测试策略

TDD 覆盖纯逻辑（无 IO）：

| 模块 | 测试 |
|---|---|
| `common::hybrid::merge_rrf` | 纯关键词/纯向量/混合去重/排名顺序/top_k 截断 |
| `common::hybrid::ThinkSplitter` | 普通 token / 跨 token 的 `<think>` / 未闭合 / 嵌套场景 |
| `server::llm::parse_sse_line` | OpenAI delta.content / reasoning_content / [DONE] / 空行 |
| `server::llm::build_openai_body` | messages 映射、stream:true、model 字段 |

IO 层（lancedb 查询、reqwest 调用、HTTP 端到端）不做单测，靠 `scripts/e2e-local.sh` + 手动验证。

## 7. 受影响文件

新增：
- `overlay/crates/llm-wiki-common/src/vector.rs`（迁入）
- `overlay/crates/llm-wiki-common/src/hybrid.rs`（RRF + ThinkSplitter 不放这；ThinkSplitter 是 chat 用的，放 server）
- `overlay/server/src/llm.rs`（embed_query + stream_chat + parse_sse_line + ThinkSplitter）

修改：
- `overlay/crates/llm-wiki-common/Cargo.toml`（加 lancedb/arrow/futures）
- `overlay/crates/llm-wiki-common/src/lib.rs`（pub mod vector, hybrid）
- `overlay/crates/llm-wiki-common/src/search/mod.rs`（pub search_vector, hybrid_search）
- `overlay/server/Cargo.toml`（加 reqwest, futures）
- `overlay/server/src/main.rs`（建 runtime 注入 ServerState）
- `overlay/server/src/state.rs`（存 runtime）
- `overlay/server/src/api/search.rs`（接 hybrid，删 501）
- `overlay/server/src/api/chat.rs`（reqwest 重写）
- `overlay/cli/rust/Cargo.toml`（删 lancedb/arrow/futures）
- `overlay/cli/rust/src/main.rs` + `cmd_vector.rs`（改用 common::vector）

删除：
- `overlay/cli/rust/src/vector.rs`（迁入 common）

## 8. 兼容性

- Web 前端 `overlay/web/lib/search.ts` 不发 `queryEmbedding`（只发 `{topK, includeContent}`）——服务端自管 embed，无破坏。
- Web 前端 `llm-client.ts` 解析的 SSE 帧格式不变。
- CLI `llm-wiki search` 仍只走关键词（CLI 侧暂不接向量，保持简单；可后续加）。
- 配置 `server.example.json` 的 `embeddingConfig` / `llmConfig` 字段不变。

## 9. 风险

| 风险 | 缓解 |
|---|---|
| lancedb 0.27 API 与 vector.rs 现有写法不一致 | 写法已验证（upsert 在用）；查询用 `nearest_to().limit().execute()` + `try_collect`（docs.rs 确认） |
| reqwest rustls 在 musl 静态链接体积/兼容 | musl 已用；rustls 比 openssl 更适合静态。先本地 glibc 验证，musl 后测 |
| Chat 跨平台 SSE 解析边界（`data:` 行缓冲） | 移植 Node 版 line-buffer 逻辑 + 单测 |
| 维度不匹配（embed 模型换过、旧向量库） | search_vector 捕获错误 → 降级关键词 |
