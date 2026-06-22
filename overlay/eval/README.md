# RAG 评测体系

> 适用项目：`llm_wiki-server`（Overlay 定制）  
> 最后更新：2026-06-08

---

## 1. 评测架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                          评测体系                                   │
├─────────────────────────┬───────────────────────────────────────────┤
│ Layer 1: Ingest 质量     │ 离线评估：原始材料 vs Wiki 页面           │
│ Layer 2: RAG 检索        │ 离线评估：问题 → 期望来源 是否命中        │
│ Layer 3: Chat 生成       │ 在线评估：回答质量 vs 参考答案            │
└─────────────────────────┴───────────────────────────────────────────┘
```

## 2. 目录结构

```
overlay/eval/
├── README.md              # 本文件
├── test_cases/            # 测试集
│   ├── parenting_books.json   # 育儿书籍测试集
│   └── civil_career.json      # 职业发展测试集
├── ingest_check.py        # Ingest 质量检查工具
├── rag_eval.py            # RAG + Chat 在线评测
├── results/               # 评测结果
└── scripts/
    └── run_eval.sh        # 批量评测脚本
```

## 3. 测试集格式

测试集为 JSON 文件，每个测试用例包含：

```json
{
  "id": "case_001",
  "question": "宝宝2个月要补充维生素D吗？每天多少剂量？",
  "project": "ParentingBooks",
  "expected_sources": [
    "wiki/sources/崔玉涛宝贝健康公开课-01-*.md"
  ],
  "expected_entities": [
    "维生素D",
    "初乳"
  ],
  "expected_answers": [
    "生后15天开始补充",
    "200-400国际单位"
  ],
  "keywords": ["维生素D", "剂量", "补充"],
  "note": "考察：关键营养素补充时机与剂量"
}
```

## 4. 使用方法

### 4.1 构建测试集

```bash
# 为项目创建测试集
cp overlay/eval/test_cases/template.json \
   overlay/eval/test_cases/parenting_books.json
# 编辑测试用例（见 §5）
```

### 4.2 运行评测

```bash
# 完整评测（Ingest 检查 + RAG + Chat）
./overlay/eval/scripts/run_eval.sh ParentingBooks

# 仅 RAG 检索评测
python overlay/eval/rag_eval.py --project ~/overseas-github/llm_wiki_projects/ParentingBooks --mode retrieval

# 仅 Chat 生成评测
python overlay/eval/rag_eval.py --project ~/overseas-github/llm_wiki_projects/ParentingBooks --mode chat
```

### 4.3 查看结果

```bash
cat overlay/eval/results/parenting_books_*.json
```

---

## 5. 测试用例设计指南

### 5.1 来源覆盖原则

每个项目建议 20-50 个测试用例，覆盖：

| 类型 | 数量 | 说明 |
|------|------|------|
| 事实性问答 | 40% | 「XX是什么」「XX怎么办」|
| 数值型问答 | 20% | 「剂量多少」「月龄多大」|
| 场景应对 | 30% | 「宝宝XX怎么办」|
| 概念解释 | 10% | 「XX的原理是什么」|

### 5.2 设计步骤

1. **通读 wiki/index.md 和 overview.md**，了解知识库覆盖范围
2. **从用户角度思考**，列出常见问题
3. **对照原始材料**，确认每个问题有对应答案
4. **填写 expected_sources**（支持 glob 模式）
5. **填写 expected_answers**（关键词匹配）

### 5.3 示例

```json
{
  "id": "case_001",
  "question": "宝宝2个月，纯母乳喂养，要补充维生素D吗？每天多少？",
  "project": "ParentingBooks",
  "expected_sources": [
    "wiki/sources/崔玉涛宝贝健康公开课-01-*.md",
    "wiki/concepts/婴儿维生素D补充指南.md"
  ],
  "expected_entities": ["维生素D", "佝偻病"],
  "expected_answers": [
    "生后15天",
    "200-400国际单位"
  ],
  "keywords": ["维生素D", "补充", "国际单位"],
  "note": "考察：关键营养素补充时机与剂量"
}
```

---

## 6. 评测指标

### 6.1 Ingest 质量

| 指标 | 计算方法 |
|------|----------|
| 信息覆盖率 | 原始材料关键点 / wiki 页面覆盖点 |
| 结构合规率 | 符合 schema 的页面数 / 总页面数 |
| Wikilink 密度 | wiki 内链数 / 页面数 |
| 场景页覆盖率 | 有场景页的原始材料数 / 总材料数 |

### 6.2 RAG 检索

| 指标 | 计算方法 |
|------|----------|
| 召回率 (Recall@K) | 检索到的相关文档 / 总相关文档 |
| MRR | 首个相关文档的排名倒数均值 |
| NDCG | 归一化折损累积增益 |

### 6.3 Chat 生成

| 指标 | 计算方法 |
|------|----------|
| 答案准确率 | 包含 expected_answers 的回答数 / 总回答数 |
| 来源引用率 | 引用了 expected_sources 的回答数 / 总回答数 |
| 幻觉率 | 超出 wiki 范围的回答数 / 总回答数（人工判断）|
| 用户满意度 | 1-5 分（人工评分）|

---

## 7. 持续监控

### 7.1 回归测试

每次 ingest 新材料后运行：

```bash
./overlay/eval/scripts/run_eval.sh ParentingBooks --regression
```

### 7.2 增量测试

新加测试用例后：

```bash
./overlay/eval/scripts/run_eval.sh ParentingBooks --incremental
```

### 7.3 趋势追踪

查看历史结果：

```bash
cat overlay/eval/results/parenting_books_trend.json
```

---

## 8. 相关文档

| 文档 | 内容 |
|------|------|
| [新项目指引.md](../../docs/新项目指引.md) | 从材料到知识库的完整流程 |
| [日常运维.md](../../docs/日常运维.md) | Ingest 与运维操作 |
| [代码结构总览.md](../../docs/代码结构总览.md) | 架构与模块关系 |
