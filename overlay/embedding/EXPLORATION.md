# LLM Wiki Embedding 配置探索文档

> 本文档由 Claude Code 会话生成，记录了 2026/06/02 对 LLM Wiki 项目 embedding 功能的探索结果。

---

## 1. 项目结构

```
llm_wiki/
├── src/                          # 前端代码 (React + TypeScript)
│   ├── components/settings/       # 设置界面组件
│   │   └── sections/
│   │       └── embedding-section.tsx   # Embedding 配置 UI
│   ├── lib/
│   │   └── embedding.ts          # Embedding 核心逻辑
│   └── stores/
│       └── wiki-store.ts        # Zustand 状态管理，含 EmbeddingConfig 接口定义
├── src-tauri/                    # Rust 后端代码
│   └── src/commands/
│       └── vectorstore.rs        # LanceDB 向量数据库操作
└── ...
```

---

## 2. Embedding 配置相关文件

### 2.1 配置定义 (TypeScript)

**文件**: `src/stores/wiki-store.ts`

```typescript
interface EmbeddingConfig {
  enabled: boolean
  endpoint: string          // e.g. "http://127.0.0.1:1234/v1/embeddings"
  apiKey: string
  model: string              // e.g. "text-embedding-qwen3-embedding-0.6b"
  outputDimensionality?: number
  maxChunkChars?: number
  overlapChunkChars?: number
  extraHeaders?: Record<string, string>
}
```

### 2.2 Embedding 核心逻辑

**文件**: `src/lib/embedding.ts`

主要函数：
- `fetchEmbedding(text, cfg)` - 调用 embedding API，支持自动减半重试
- `embedPage(projectPath, pageId, title, content, cfg)` - 嵌入单个页面
- `embedAllPages(projectPath, cfg, onProgress)` - 批量嵌入所有页面
- `searchByEmbedding(projectPath, query, cfg, topK)` - 向量搜索
- `getEmbeddingCount(projectPath)` - 获取已索引的 chunk 数量

### 2.3 UI 配置界面

**文件**: `src/components/settings/sections/embedding-section.tsx`

用户在 Settings → Embedding 面板中看到的配置项：
- Enable/Disable 开关
- Endpoint URL
- API Key
- Model 名称
- Output Dimensionality (可选)
- Extra Headers (可选)
- Max Chunk Chars
- Overlap Chunk Chars
- Test Connection / Test Function 按钮
- Reindex All 按钮

---

## 3. 向量存储位置

向量数据存储在项目的 **`.llm-wiki/lancedb/`** 目录下。

**Rust 路径计算** (`src-tauri/src/commands/vectorstore.rs:44-46`):

```rust
fn db_path(project_path: &str) -> String {
    format!("{}/.llm-wiki/lancedb", project_path.replace('\\', "/"))
}
```

**示例**:
```
CivilCareer/.llm-wiki/lancedb/
```

---

## 4. 全局配置文件

配置存储在 **全局 `app-state.json` 文件**（不是项目目录下）：

| 操作系统 | 路径 |
|---------|------|
| **Windows** | `%APPDATA%\com.llmwiki.app\app-state.json` 或 `%APPDATA%\LLM Wiki\app-state.json` |
| **macOS** | `~/Library/Application Support/com.llmwiki.app/app-state.json` |
| **Linux** | `~/.local/share/com.llmwiki.app/app-state.json` 或 `~/.config/com.llmwiki.app/app-state.json` |

### 配置 JSON 结构

```json
{
  "embeddingConfig": {
    "enabled": true,
    "endpoint": "http://127.0.0.1:1234/v1/embeddings",
    "apiKey": "your-api-key",
    "model": "text-embedding-model-name",
    "outputDimensionality": 768,
    "maxChunkChars": 1000,
    "overlapChunkChars": 200,
    "extraHeaders": {}
  }
}
```

**可以通过文本编辑器直接修改此文件来配置 embedding。**

---

## 5. 搜索机制

### 5.1 混合搜索 (Hybrid Search)

当 `embeddingConfig.enabled = true` 时，使用混合搜索模式：
- **向量搜索** - 基于 embedding 的语义相似度
- **关键字搜索** - BM25/Token-based 搜索
- **RRF 融合** - Reciprocal Rank Fusion 合并两种结果

搜索实现位于 Rust 后端 (`src-tauri/src/commands/search.rs`)，通过 `search_project` Tauri 命令调用。

### 5.2 纯关键字搜索

当 `embeddingConfig.enabled = false` 时，仅使用关键字搜索。

---

## 6. 配置后操作流程

1. **配置 embedding** → 在 Settings → Embedding 填写:
   - Endpoint URL
   - API Key (如有)
   - Model 名称

2. **点击 "Reindex All"** → 重新嵌入所有已提取的 wiki 页面到 `.llm-wiki/lancedb/`

3. **搜索自动使用混合模式** - 无需额外操作

---

## 7. 项目 CivilCareer 当前状态

```
CivilCareer/
├── .llm-wiki/
│   ├── chats/
│   ├── lancedb/          ← 不存在（尚未生成向量）
│   ├── file-snapshot.json
│   ├── ingest-cache.json
│   ├── lint.json
│   ├── project.json
│   └── review.json
├── raw/                  # 原始数据
├── wiki/                 # 提取的 wiki 页面
├── .obsidian/
├── purpose.md
└── schema.md
```

**lancedb 目录不存在**，说明尚未为该项目配置和运行 embedding。

---

## 8. 关键代码位置速查

| 功能 | 文件路径 |
|------|---------|
| EmbeddingConfig 接口定义 | `src/stores/wiki-store.ts:89-119` |
| Embedding 核心逻辑 | `src/lib/embedding.ts` |
| Embedding UI 组件 | `src/components/settings/sections/embedding-section.tsx` |
| 向量存储 (Rust) | `src-tauri/src/commands/vectorstore.rs` |
| 搜索命令 (Rust) | `src-tauri/src/commands/search.rs` |
| 配置持久化 | `src/lib/project-store.ts:89-99` |
| 搜索 API | `src/lib/search.ts` |