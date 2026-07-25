# Ingest 全流程分析

> 基于 `llm_wiki-server` 代码库和 `llm_wiki_projects` 研究。配合同级其他文档阅读。
> 关联: [远端服务器 ingest](./远端服务器ingest.md) | [代码结构总览](./代码结构总览.md)

---

## 1. 项目架构定位

```
llm_wiki-server/           # 集成层 — 本仓库
├── upstream/              # nashsu/llm_wiki 子模块 (v0.4.20, 只读)
│   ├── src/lib/ingest.ts  # → 核心 ingest 逻辑（6 步管线）
│   ├── src/lib/ingest-queue.ts
│   ├── src/lib/ingest-cache.ts
│   ├── src/lib/source-lifecycle.ts
│   └── src/lib/page-merge.ts
├── overlay/               # 100% 自定义代码
│   ├── cli/rust/src/cmd_ingest.rs   # Rust CLI → spawn Node 子进程
│   ├── cli/node/src/cmd-ingest.ts   # Node/TS shim → 调用上游 autoIngest()
│   ├── cli/node/src/setup-stores.ts # 填充 Zustand stores
│   ├── cli/node/src/load-config.ts  # 解析 config (${ENV_VAR} 展开)
│   └── cli/node/src/shims/          # Tauri API 存根
├── scripts/
│   ├── ingest-batch.sh    # 批量入口（可恢复）
│   └── llm-wiki           # bash wrapper → exec Rust CLI
└── docs/ingest-flow-analysis.md     # ← 本文档
```

**核心设计原则：**

| 规则 | 含义 |
|------|------|
| 上游零提交 | 自定义放 `overlay/` + 补丁 |
| HTTP UI 只读 | 写操作（ingest、reindex）只通过 CLI |
| Chat 纯 Rust | `/chat` 走 reqwest 直连 LLM，无 Node |
| Ingest 仍需 Node | 因上游 `autoIngest()` 与 Zustand store 紧耦合 |
| Wiki 数据外置 | 存在 `~/overseas-github/llm_wiki_projects/<name>/`，不在本仓库 |

---

## 2. Ingest 调用链

```
[用户/脚本]
   │
   ▼
scripts/ingest-batch.sh                        ← batch 入口
   │  遍历 raw/sources/*.md, 跳过已有 wiki 输出
   │  逐个调用 scripts/llm-wiki ingest ...
   ├──▶ scripts/llm-wiki                        ← bash wrapper (14行)
   │     设置 LLM_WIKI_REPO=$PWD; exec Rust 二进制
   │
   ▼
overlay/cli/rust/src/cmd_ingest.rs              ← Rust CLI
   │  验证路径, 定位 tsx CLI 模块
   │  spawn: node --no-warnings <tsx cli.mjs> cmd-ingest.ts
   │  传递 env: LLM_WIKI_BIN, LLM_WIKI_REPO
   │  ❗ 为什么不用 npx tsx?
   │    → npm exec 产生中间进程不随子退出, 批量会卡死
   │    → 直接 node <tsx_cli> 单进程, 干净退出
   │
   ▼
overlay/cli/node/src/cmd-ingest.ts              ← Node/TS shim (~40行)
   │  1. parseFlag(): 解析 --project --source --config
   │  2. loadConfigFile(): 加载 JSON + ${ENV_VAR} 展开
   │  3. hydrateStoresFromConfig(): 填充 Zustand stores
   │     - LlmConfig / EmbeddingConfig / MultimodalConfig
   │     - outputLanguage = "auto"
   │  4. autoIngest(project, source, llmConfig)
   │  5. process.exit(0) — 强制退出 (上游代码持有各种 handle)
   │
   ▼
upstream/src/lib/ingest.ts                      ← 上游核心
      autoIngest() → autoIngestImpl()
         6 步管线 (见第 3 节)
```

---

## 3. autoIngest 6 步管线

受 `withProjectLock()` 保护，同项目并发串行化。

```
autoIngestImpl()
│
├─ Step 0: 预检查 ——————————————————————————————
│  ├─ 读取 source 文件、schema.md、purpose.md、index.md、overview.md
│  ├─ checkIngestCache(): SHA-256 命中 → 跳到图片提取后返回
│  └─ 计算 sourceBudget (maxContextSize - 固定上下文)
│
├─ Step 0.5: 图片提取 ——————————————————————————
│  ├─ extractAndSaveSourceImages()  — PDF/DOCX/PPTX 内嵌
│  ├─ extractAndSaveMarkdownImages() — Markdown 引用
│  └─ 保存到 wiki/media/<source-slug>/
│
├─ Step 0.6: 图片标注 (可选) ———————————————————
│  ├─ multimodalConfig.enabled = true
│  │  → captionMarkdownImages() 用 VLM 生成 alt-text
│  └─ disabled
│     → 从 sourceContent 中剥离 ![](url) 引用
│
├─ Step 0.7: 长文档分块 ————————————————(if source > budget)
│  └─ analyzeLongSourceInChunks()
│     → 语义分块(heading 边界) + 渐进式摘要
│     → checkpoint 到 .llm-wiki/ingest-progress/<slug>-<hash>.json
│     → 支持中断后恢复
│
├─ Step 1: LLM 分析 (Stage 1) —————————————————
│  ├─ 系统提示: buildAnalysisPrompt()
│  ├─ 输出: 实体/概念/论点/矛盾/与现有 wiki 连接
│  ├─ 温度 0.1, reasoning off, max_tokens 4096
│  └─ 长文档直达: llm 没调用, 用 precomputedAnalysis
│
├─ Step 2: LLM 生成 (Stage 2) —————————————————
│  ├─ 系统提示: buildGenerationPrompt()
│  ├─ 用户提示: "Now emit the FILE blocks...NO PREAMBLE"
│  ├─ 输出格式: 纯 ---FILE:path--- / ---REVIEW:...--- 块
│  ├─ 可选审查阶段: 大型输出走 buildReviewSuggestionPrompt()
│  ├─ 温度 0.1, reasoning off, max_tokens 按预算计算
│  └─ 容错: 流失败抛异常, 超时/中断可恢复
│
├─ Step 3: 写文件 ——————————————————————————————
│  ├─ migrateLegacySourceSummaryIfSafe()
│  ├─ writeFileBlocks():
│  │   parseFileBlocks() → 容错解析 LLM 输出
│  │   sanitizeIngestedFileContent() → 清理代码块/frontmatter
│  │   mergePageContent() → LLM 合并已有页面 (三保险)
│  │    ① frontmatter 数组字段 union
│  │    ② 正文 LLM 合并 (body ≥ 70% max input)
│  │    ③ 锁定字段(type/title/created) 回写
│  │   contentMatchesTargetLanguage() → 语言守卫
│  ├─ fallback: LLM 未生成 source-summary → 补写 stub
│  └─ 文件写入 + 刷新 fileTree
│
├─ Step 3.5: 图片注入 (可选) ——————————————————
│  └─ injectImagesIntoSourceSummary()
│     → source-summary 页追加 ## Embedded Images 节
│
├─ Step 4: Review items ———————————————————————
│  ├─ 解析 ---REVIEW:--- 块 (生成阶段 + 审查阶段)
│  └─ useReviewStore.addItems()
│
├─ Step 5: 缓存保存 ———————————————————————————
│  ├─ saveIngestCache() SHA-256 + 文件列表
│  └─ 有硬失败时不保存 (防止冻结不完整结果)
│
└─ Step 6: 向量嵌入 (可选) ———————————————————
   └─ embedPage() 对每个输出文件
      跳过 index/log/overview
```

---

## 4. 关键子系统

| 子系统 | 文件 | 作用 |
|--------|------|------|
| **Ingest Queue** | `upstream/src/lib/ingest-queue.ts` | 后台任务队列，持久化 `.llm-wiki/ingest-queue.json`，支持暂停/恢复/取消（3 次重试） |
| **Ingest Cache** | `upstream/src/lib/ingest-cache.ts` | SHA-256 内容寻址，缓存到 `.llm-wiki/ingest-cache.json` |
| **Source Lifecycle** | `upstream/src/lib/source-lifecycle.ts` | 源文件导入/删除，级联清理孤立的 wiki 页面 |
| **Page Merge** | `upstream/src/lib/page-merge.ts` | LLM 驱动的页面合并，保护已有内容不被覆盖 |
| **Sanitize** | `upstream/src/lib/ingest-sanitize.ts` | LLM 输出清理（代码块剥离、frontmatter 修复、wikilink 列表修复） |
| **Long Source** | `ingest.ts` 内部 | 语义分块 + 渐进式摘要 + 检查点恢复 |
| **Image Pipeline** | `ingest.ts` + `caption.ts` | 图片提取 → VLM 标注 → SHA-256 缓存 → 注入 source-summary |
| **Project Mutex** | `upstream/src/lib/project-mutex.ts` | 项目级 Promise 锁，防止 index.md 竞争条件 |

---

## 5. 数据流：输入 → 输出

```
输入: <project>/raw/sources/<file>.md
      <project>/schema.md       ← 页面类型约定
      <project>/purpose.md      ← 项目目标
      <project>/wiki/index.md   ← 现有索引（被读取并更新）
      <project>/wiki/overview.md

  │
  │  autoIngest(schema, purpose, index, sourceContent)
  ▼

输出 (<project>/):

  ├─ wiki/sources/<slug>.md          ← 源文件摘要页 (必写, LLM/fallback)
  ├─ wiki/entities/<name>.md         ← 实体页 (疾病/症状/营养素/器官...)
  ├─ wiki/concepts/<name>.md         ← 概念页 (方法/原则/定义...)
  ├─ wiki/scenarios/<name>.md        ← 场景问答页
  ├─ wiki/lessons/<name>.md          ← 警示/教训/禁忌页
  ├─ wiki/index.md                   ← 更新后的全库索引
  ├─ wiki/log.md                     ← 更新日志 (含新建页面列表)
  ├─ wiki/overview.md                ← 更新后的库概述
  ├─ wiki/media/<slug>/              ← 提取的图片文件
  ├─ .llm-wiki/ingest-cache.json     ← SHA-256 缓存记录
  └─ .llm-wiki/ingest-progress/      ← 长文档断点 (仅大文件)
```

### 输出文件格式

每个 wiki 页面带 YAML frontmatter:

```markdown
---
type: entity                    # source | entity | concept | scenario | lesson
title: "肺炎链球菌感染"
created: 2026-01-15
updated: 2026-04-29
sources: ["《毓园》肺炎链球菌的困惑.txt"]
tags: ["疾病", "细菌感染", ...]
related: ["相关页面slug"]       # wikilink 数组
---

# 正文内容...
```

---

## 6. CLI Shims 机制

非 Tauri 环境不能直接调用上游代码，需要存根：

| Tauri 依赖 | Shim 路径 | 策略 |
|------------|-----------|------|
| `@/commands/fs` | `overlay/cli/node/src/shims/fs.ts` | 重写为 `node:fs` |
| `@/commands/file-sync` | `./shims/file-sync.ts` | 空实现 |
| `@tauri-apps/api/core` | `./shims/tauri-core.ts` | 空实现 |
| `@tauri-apps/plugin-http` | `./shims/tauri-http.ts` | 空实现 |
| `@/lib/tauri-fetch` | `./shims/tauri-fetch.ts` | 空实现 |
| `@/lib/project-store` | `./shims/project-store-noop.ts` | 空实现 |

存储在 `tsconfig.json` 的 `paths` 映射:

```json
{
  "paths": {
    "@/*": ["../../../upstream/src/*"],
    "@tauri-apps/api/core": ["./src/shims/tauri-core.ts"],
    "@/commands/fs": ["./src/shims/fs.ts"]
  }
}
```

---

## 7. 上游同步维护

| 方案 | 说明 |
|------|------|
| Upstream 子模块 | 指向 `nashsu/llm_wiki` tag `v0.4.20` (commit `9712d43`) |
| 自定义方案 | `overlay/` + 补丁 `0002-http-ui-bootstrap.patch` |
| 构建时打补丁 | `apply-patches.sh` 修改本地 upstream 工作树 |
| 升级流程 | `sync-upstream.sh vX.Y.Z` → 合并冲突 → `cd upstream && git diff > ../overlay/patches/0002-http-ui-bootstrap.patch` |

---

## 8. 当前 Wiki 项目状态

```text
~/overseas-github/llm_wiki_projects/
├── CivilCareer/          ✅ 已完成 — 313 个源文件 → ~300+ wiki 页面
├── ParentingBooks/       ✅ 已完成 — 181 个源文件 → ~200+ wiki 页面
├── audio_transcripts/    ❌ 未启动 — 纯 .txt 转录文件
└── books/                ❌ 未启动 — 纯 .md 章节文件
```

每个已处理项目结构一致：

```
<project>/
├── purpose.md              # LLM 读取的项目目标
├── schema.md               # LLM 读取的页面类型约定
├── raw/sources/            # 原始素材（不可变输入）
├── wiki/                   # LLM 生成的知识页面
│   ├── sources/            # 每原始文件一个摘要
│   ├── entities/
│   ├── concepts/
│   ├── scenarios/
│   ├── lessons/
│   └── index.md, log.md, overview.md
├── .llm-wiki/
│   ├── project.json
│   ├── ingest-cache.json   # SHA-256 缓存
│   ├── ingest-queue.json
│   └── lancedb/            # 向量索引
└── .obsidian/              # (CivilCareer 专属)
```

---

## 9. 设计决策备忘

1. **`process.exit(0)` 在 cmd-ingest.ts 中强制调用** — 上游代码持有文件监视器 / LanceDB 连接 / timers，不强制退出则父进程 Rust `Command::status()` 永远不返回，批量循环挂死
2. **`node <tsx cli.mjs>` 而非 `npx tsx`** — 消除中间进程，确保子进程退出干净
3. **缓存命中时图片管线仍运行** — 用户可能在旧版本 ingest 后新增了图片标注功能，需要让缓存结果逐步收敛到当前管线契约
4. **硬失败时不保存缓存** — 防止冻结部分写入结果，后续重试才能重新生成缺失文件
5. **并发保护通过 `withProjectLock()`** — 非数据库锁，而是 Promise 链 mutex，因为是单进程操作

---

## 10. 技术债务

| 问题 | 影响 | 说明 |
|------|------|------|
| **Ingest 仍需 Node** | 远端部署复杂 | 上游 `autoIngest()` 与 Zustand 紧耦合，无法直接纯 Rust 重写 |
| **tsx 是 devDep** | `npm ci --omit=dev` 不可用 | `ingest` 子进程依赖 `tsx` 运行 TypeScript |
| **事件循环泄漏** | 必须 `process.exit(0)` | upstream 代码的副作用导致 CLI 模式需要强制退出 |

---

*注: 本文档基于 `llm_wiki-server` 代码库分析，autoIngest 是唯一的生产级入口。交互式 ingest（startIngest + executeIngestWrites）仅用于 Tauri 桌面 UI。*
