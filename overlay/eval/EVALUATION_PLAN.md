# RAG 评测体系设计方案

> 最后更新：2026-06-08  
> 适用项目：`llm_wiki-server`（Overlay 定制）

---

## 1. 背景与目标

### 1.1 问题描述

LLM Wiki 项目面临两个关键问题：

| 问题 | 描述 |
|------|------|
| **Ingest 效果未知** | 原始材料是否都已处理？Wiki 页面质量如何？ |
| **Chat 效果未知** | 检索是否命中正确页面？回答是否准确？是否存在幻觉？ |

### 1.2 评测目标

| 目标 | 说明 |
|------|------|
| **可量化** | 用数值指标衡量质量 |
| **可复现** | 相同测试用例产生相同结果 |
| **可迭代** | 发现问题后可修复并重新评测 |
| **自动化** | 减少人工介入，快速反馈 |

---

## 2. 评测架构

### 2.1 三层评测体系

```
┌─────────────────────────────────────────────────────────────────┐
│                    三层评测体系                                  │
├─────────────────────────────────────────────────────────────────┤
│ │
│  Layer 1: Ingest 质量（离线）                                    │
│  ├── Schema 合规性 │
│  ├── 结构完整性（概念、实体、场景）                              │
│  ├── Wikilink 密度                                              │
│  └── 内容覆盖率                                                  │
│                                                                 │
│  Layer 2: RAG 检索（离线）                                      │
│  ├── 召回率 Recall@K                                            │
│  ├── 来源覆盖度                                                  │
│  └── 关键词匹配                                                  │
│                                                                 │
│  Layer 3: Chat 生成（在线）                                     │
│  ├── 答案准确率 │
│  ├── 引用准确性                                                  │
│  └── 幻觉检测                                                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. 测试用例设计

### 3.1 测试用例结构

```json
{
  "id": "case_001",
  "question": "宝宝2个月，纯母乳喂养，要补充维生素D吗？每天多少剂量？",
  "expected_sources": ["wiki/sources/崔玉涛宝贝健康公开课-01-*.md"],
  "expected_answers": ["生后15天开始补充", "200-400国际单位"],
  "expected_entities": ["维生素D", "佝偻病"],
  "keywords": ["维生素D", "剂量", "补充", "纯母乳"],
  "category": "fact",
  "difficulty": "easy",
  "note": "考察：关键营养素补充时机与剂量"
}
```

### 3.2 分类定义

| Category | 说明 | 示例 |
|----------|------|------|
| `fact` | 事实、定义问答 | 「什么是初乳？」 |
| `number` | 数值、剂量问答 | 「每天补充多少剂量？」 |
| `scenario` | 场景应对 | 「宝宝吐奶怎么办？」 |
| `concept` | 概念理解 | 「厌奶期原理？」 |

### 3.3 难度分布

| 难度 | 比例 | 说明 |
|------|------|------|
| `easy` | 40% | 简单事实型，检索即可命中 |
| `medium` | 40% | 部分检索+推理 |
| `hard` | 20% | 复杂场景，需综合多来源 |

---

## 4. 测试用例生成方法

### 4.1 三种生成模式

| 模式 | 优点 | 缺点 |
|------|------|------|
| **纯人工** |质量高、精确 | 慢、成本高 |
| **LLM 全自动** | 快速、大规模 | 需验证 |
| **LLM + 人工审核** ⭐ | 平衡效率与质量 | 需审核流程 |

### 4.2 LLM 生成原理

```
输入材料（Wiki 页面）
       │
       ▼
┌─────────────────────────────┐
│ LLM Prompt:                │
│ "根据材料生成测试用例，      │
│  expected_answers 必须      │
│  在原文中找到支持" │
└─────────────┬───────────────┘
              │
              ▼
┌─────────────────────────────┐
│ LLM Output (JSON):         │
│ [ │
│   {                         │
│     "question": "...",      │
│     "expected_answers": │
│       ["生后15天", "200-400"]│
│   }                         │
│ ] │
└─────────────────────────────┘
```

### 4.3 质量保证四层体系

| Layer | 时机 | 检查项 |
|-------|------|--------|
| **Layer 1** | 生成时 | 必填字段、JSON 格式、文件路径 |
| **Layer 2** | 生成时 | Prompt 要求、LLM temperature 设置 |
| **Layer 3** | 生成后 | category/difficulty 分布、页面覆盖率 |
| **Layer 4** | hybrid模式 | 人工抽样审核（分层抽样） |

### 4.4 质量指标

| 指标 | 目标值 |
|------|--------|
| 问题可回答率 | > 90% |
| 分类覆盖均衡 | 各 category ≥ 15% |
| 难度分布 | 4:4:2 (易:中:难) |
| 页面覆盖率 | > 60% |
| 人工审核通过率 | > 80% |

---

## 5. 评测执行

### 5.1 环境要求

```bash
# 启动 llm-wiki-server
./overlay/server/target/release/llm-wiki-server &

# 确认运行
curl http://127.0.0.1:8080/api/v1/health
```

### 5.2 执行命令

```bash
# 完整评测
./overlay/eval/scripts/run_eval.sh ParentingBooks

# 仅 Ingest 质量检查
python3 overlay/eval/ingest_check.py --project <项目路径>

# 仅 RAG 检索
python3 overlay/eval/rag_eval.py --project ParentingBooks --mode retrieval

# 仅 Chat 生成
python3 overlay/eval/rag_eval.py --project ParentingBooks --mode chat

# LLM 生成测试用例
python3 overlay/eval/generate_test_cases.py --project <项目路径> --mode hybrid
```

### 5.3 结果判定

| 综合得分 | 判定 | 行动 |
|----------|------|------|
| ≥ 80 分 | ✅ 良好 | 可投入使用 |
| 60-79 分 | ⚠️ 一般 | 建议优化 |
| < 60 分 | ❌ 较差 | 需改进 |

---

## 6. 持续迭代

### 6.1 迭代流程

```
初始生成 → 评测运行 → 失败分析 → 补充生成 → 再评测
    ↑                                              │
    └──────────────────────────────────────────────┘

失败案例分析:
├── 检索失败 → 补充关键词 / 检查 expected_sources
├── 回答不准确 → 修正 expected_answers
└── 问题不可答 → 删除 / 重写问题
```

### 6.2 触发时机

| 触发事件 | 操作 |
|----------|------|
| 新增材料 ingest 后 | 增量评测 |
| 修改 Ingest prompt 后 | 完整评测 |
| 发现 Chat 质量问题 | 分析失败案例 |
| 上线前验证 | 完整评测 |

---

## 7. 目录结构

```
overlay/eval/
├── README.md # 快速开始
├── EVALUATION_PLAN.md         # 本文档
├── test_cases/                # 测试用例
│   ├── template.json          # 模板
│   └── parenting_books.json   # 育儿书籍测试集
├── ingest_check.py # Ingest 质量检查
├── rag_eval.py                # RAG + Chat 评测
├── generate_test_cases.py     # LLM 测试用例生成
├── scripts/
│   └── run_eval.sh            # 批量评测
└── results/                   # 结果输出
```

---

## 8. 快速开始

```bash
# Step 1: 检查 Ingest 质量
python3 overlay/eval/ingest_check.py \
    --project ~/overseas-github/llm_wiki_projects/ParentingBooks

# Step 2: 启动 server
./overlay/server/target/release/llm-wiki-server &

# Step 3: 运行评测
./overlay/eval/scripts/run_eval.sh ParentingBooks
```

---

## 9. 相关文档

| 文档 | 内容 |
|------|------|
| [README.md](README.md) | 快速开始指南 |
| [日常运维.md](../../docs/日常运维.md) | Ingest 与运维 |
| [新项目指引.md](../../docs/新项目指引.md) | 从材料到知识库 |
