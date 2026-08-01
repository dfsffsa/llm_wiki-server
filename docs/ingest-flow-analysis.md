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
│                        (含 bgs- 办公室谋人谋事 150、sd- 职场手段 29、
│                         zcr- 职场人建议 9、laoa- 职场回忆录 125)
├── ParentingBooks/       ✅ 已完成 — 181 个源文件 → ~200+ wiki 页面
├── audio_transcripts/    🔄 CivilCareer 的原始音频转录文件夹
│                         (188 个 .txt，与 bgs/sd/zcr 源对应，已全部 ingest)
└── books/                ❌ 未启动 — 纯 .md 章节文件，无 wiki/.llm-wiki 结构
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

## 9. Ingest 结果评估

> 评估系统位于 `overlay/eval/`，独立于上游代码，是 overlay 层的自有实现。

```
overlay/eval/
├── ingest_check.py         # Layer 1: Ingest 质量检查（离线）
├── auto_fix.py             # Ingest 自动修复管线（规则驱动）
├── fixers/                 # 修复策略实现
│   ├── __init__.py         # Fixer 注册表
│   ├── frontmatter.py      # Frontmatter 字段修复
│   └── wikilink.py         # Broken wikilink 修复
├── llm_judge.py            # Layer 4: LLM 内容级质量评估
├── judge/                  # LLM Judge 核心模块
│   ├── __init__.py
│   ├── models.py           # JudgeReportItem, CoverageClaim, Hallucination
│   ├── llm_client.py       # LLM API 调用封装（OpenAI 兼容）
│   ├── extractor.py        # Role A: 从 source 提取关键陈述
│   └── evaluator.py        # Role B: 比对 wiki 页判定覆盖度
├── rag_eval.py             # Layer 2+3: RAG 检索 + Chat 生成评测（在线）
├── generate_test_cases.py  # LLM 辅助的测试用例生成器
├── test_cases/
│   ├── parenting_books.json      # v1 测试集（15 用例）
│   ├── parenting_books_v2.json   # v2 测试集（100 用例）
│   └── template.json             # 测试用例模板
├── scripts/run_eval.sh     # 批量运行脚本
├── tests/                  # 单元测试
│   ├── test_p0.py          # P0 指标修复测试（17 个）
│   ├── test_finding_model.py     # Finding 模型测试（21 个）
│   ├── test_auto_fix.py          # Auto-fix 管线测试（7 个）
│   └── test_llm_judge.py         # LLM Judge 测试（12 个）
├── README.md               # 使用说明
└── AUDIT_2026-06-23.md     # 历史审计报告
```

### 三层评估架构

| 层 | 工具 | 评估内容 | 核心指标 |
|---|---|---|---|
| **Layer 1: Ingest 质量** | `ingest_check.py` | 原始材料 vs Wiki 输出 | 结构合规率、Wikilink 密度、场景覆盖率、孤儿页面率、综合评分 |
| **Layer 2: RAG 检索** | `rag_eval.py --mode retrieval` | 问题 → 搜索结果是否命中期望来源 | Recall@K、MRR、关键词匹配度 |
| **Layer 3: Chat 生成** | `rag_eval.py --mode chat` | LLM 回答是否包含期望答案 | 答案准确率、来源覆盖率（关键词模糊匹配） |
| **Layer 4: LLM Judge** | `llm_judge.py` | Ingest 内容级质量评估 | 信息覆盖率 0-10、事实一致性 0-10、幻觉数 |

### llm_judge.py — LLM 内容级质量评估

离线工具，直接调用 LLM API（OpenAI 兼容接口），不依赖 llm-wiki-server。

```
# 评估全量 source（每个 source 两次 LLM 调用）
python overlay/eval/llm_judge.py --project ~/llm_wiki_projects/ParentingBooks \
  --config overlay/config/llm.judge.json [--sample 20] [--verbose]

# 评估 + 自动修复（coverage < 6 的 source 触发重生成）
python overlay/eval/llm_judge.py --project ... --config ... --auto-fix --threshold 6
```

**架构——两个角色同一模型：**

```
source 原文
     │
     ▼
┌─────────────────────────────┐
│ Role A: Extractor           │  ← 不知道 wiki 页存在
│ "提取所有关键陈述"           │
└──────────┬──────────────────┘
           │ 清单 A（冻结文本，不可修改）
           ▼
┌─────────────────────────────┐
│ Role B: Evaluator           │  ← 拿着清单 A 逐条比对
│ "哪些被 wiki 覆盖了？"       │
│ "wiki 多出什么？"            │
└──────────┬──────────────────┘
           │ JudgeReportItem
           ▼
  {coverage_claims[], hallucinations[], scores}
```

**调用流程：**
1. 从 `raw/sources/*.md` 和 `wiki/sources/*.md` 找到配对
2. **Extractor** — 将 source 原文发给 LLM，提取关键陈述清单（含原文位置）
3. **Evaluator** — 将清单 A（冻结）+ wiki 页发给 LLM，逐条判定覆盖度
4. 聚合结果，输出 summary（平均分、低质量列表、幻觉总数）

**输出格式：**

```json
{
  "project": "ParentingBooks",
  "config": {"model": "deepseek-v4-flash", "sample": 3},
  "reports": [{
    "source_file": "raw/sources/xxx.md",
    "wiki_page": "wiki/sources/xxx.md",
    "coverage_claims": [
      {"claim": "维D从出生后15天开始补",
       "source_location": "第3段", "wiki_coverage": "full",
       "wiki_excerpt": "应于出生后15天起补充400IU/d"}
    ],
    "hallucinations": [
      {"claim": "早产儿需加倍补充",
       "severity": "major",
       "judge_reasoning": "source 未提及早产儿剂量"}
    ],
    "scores": {"coverage": 7, "consistency": 10}
  }],
  "summary": {
    "sources_evaluated": 50,
    "avg_coverage": 7.2,
    "avg_consistency": 8.1,
    "total_hallucinations": 8,
    "low_quality": ["raw/sources/xxx.md"]
  }
}
```

**已知限制（Phase 2 现状）：**
- 评分依赖 LLM 输出一致性（通过字段别名容错已缓解）
- 无 Ground Truth 校准集
- 无对抗校验（单个 Judge）
- 自动修复为 Phase 3（方向：Repairer 第三次 LLM 调用，定向修补而非重新 ingest）

### ingest_check.py — Ingest 质量检查

离线工具，直接扫描项目目录，不依赖服务器。

```
python overlay/eval/ingest_check.py --project ~/llm_wiki_projects/ParentingBooks [--verbose]
```

检查项：
- **Frontmatter 合规**：每个 wiki 页面是否包含 `type`/`title`/`created`/`updated` 等必要字段
- **Schema 类型合规**：页面 `type` 是否在 schema.md 定义的允许范围内
- **Wikilink 密度**：统计页面内链数量/页面总数，衡量知识互联程度
- **场景页覆盖率**：有对应场景页的源文件比例
- **Source→Wiki 文件覆盖率**：`raw/sources/` 中有多少文件在 `wiki/sources/` 生成了摘要页
- **孤儿页面检测**：找出 `wiki/sources/` 中有但 `raw/sources/` 已删除的残留页面

输出综合评分（满分 100）及各维度单项分。

### rag_eval.py — 检索与生成评估

在线工具，需要运行中的 llm-wiki-server。

```
# 仅检索评估
python overlay/eval/rag_eval.py --project ~/llm_wiki_projects/ParentingBooks \
  --test-cases overlay/eval/test_cases/parenting_books.json --mode retrieval

# 含 Chat 生成评估
python overlay/eval/rag_eval.py --project ~/llm_wiki_projects/ParentingBooks --mode chat
```

评估流程：
1. 加载测试用例（JSON，含 `question`、`expected_sources`、`expected_answers`、`keywords`）
2. 向服务器发送搜索/聊天请求
3. 检查检索结果是否覆盖 `expected_sources`（支持 glob 通配符匹配）
4. 检查聊天回答是否包含 `expected_answers`（模糊关键词匹配）
5. 计算 Recall@K / MRR / 关键词匹配度

### 测试用例双层 Schema（v2）

v2 测试用例区分 **must**（必须满足）和 **should**（加分项）:

```json
{
  "id": "case_001",
  "question": "宝宝2个月纯母乳喂养要补充维生素D吗？",
  "must": {
    "expected_sources": ["wiki/sources/崔玉涛宝贝健康公开课-01-*.md"],
    "expected_answers": ["200-400国际单位", "生后15天"]
  },
  "should": {
    "expected_sources": ["wiki/concepts/婴儿维生素D补充指南.md"],
    "expected_entities": ["维生素D", "佝偻病"]
  }
}
```

### 现有局限

| 问题 | 说明 |
|------|------|
| **仅结构检查** | `ingest_check.py` 不评估内容事实准确性，综合分是格式指标而非质量指标 |
| **测试集覆盖面窄** | `test_cases/` 仅覆盖 ParentingBooks（v1 15 用例 + v2 100 用例），CivilCareer 用例数为 0 |
| **服务器参数硬编码** | `rag_eval.py` 中 `LLM_WIKI_SERVER` 和 `DEFAULT_TOKEN` 写死为 `127.0.0.1:8080` / `e2e-test-token` |
| **规划未实现** | 幻觉检测、NDCG、来源引用率、趋势追踪、回归/增量测试等已在 README 中规划但代码未实现 |

---

## 10. 设计决策备忘

1. **`process.exit(0)` 在 cmd-ingest.ts 中强制调用** — 上游代码持有文件监视器 / LanceDB 连接 / timers，不强制退出则父进程 Rust `Command::status()` 永远不返回，批量循环挂死
2. **`node <tsx cli.mjs>` 而非 `npx tsx`** — 消除中间进程，确保子进程退出干净
3. **缓存命中时图片管线仍运行** — 用户可能在旧版本 ingest 后新增了图片标注功能，需要让缓存结果逐步收敛到当前管线契约
4. **硬失败时不保存缓存** — 防止冻结部分写入结果，后续重试才能重新生成缺失文件
5. **并发保护通过 `withProjectLock()`** — 非数据库锁，而是 Promise 链 mutex，因为是单进程操作

---

## 11. 技术债务

| 问题 | 影响 | 说明 |
|------|------|------|
| **Ingest 仍需 Node** | 远端部署复杂 | 上游 `autoIngest()` 与 Zustand 紧耦合，无法直接纯 Rust 重写 |
| **tsx 是 devDep** | `npm ci --omit=dev` 不可用 | `ingest` 子进程依赖 `tsx` 运行 TypeScript |
| **事件循环泄漏** | 必须 `process.exit(0)` | upstream 代码的副作用导致 CLI 模式需要强制退出 |
| **评估系统不完整** | 无事实准确性检查 | `ingest_check.py` 只做结构检查；幻觉检测、NDCG、趋势追踪已规划未实现 |
| **评测覆盖面窄** | 只有 ParentingBooks 有测试用例 | 测试集未覆盖 CivilCareer 等其他项目 |

---

*注: 本文档基于 `llm_wiki-server` 代码库分析（第 1–8 节），overlay/eval/ 的评估系统独立于上游代码（第 9 节）。autoIngest 是唯一的生产级入口；交互式 ingest（startIngest + executeIngestWrites）仅用于 Tauri 桌面 UI。*
