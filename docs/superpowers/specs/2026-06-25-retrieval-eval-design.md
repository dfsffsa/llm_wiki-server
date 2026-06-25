# 召回评测设计：ParentingBooks v2

**日期**: 2026-06-25
**范围**: 仅 ParentingBooks 项目
**目标**: 评估 ingest 后召回环节质量，区分源页召回与衍生页召回两层

## 背景与动机

现有 15 个手工用例存在两个问题：

1. **`expected_sources` 是 glob 数组**（如 `wiki/sources/崔玉涛宝贝健康公开课-01-*.md`），但实际检索召回大量落在 `wiki/concepts/`、`wiki/scenarios/`、`wiki/lessons/` 等衍生页。`source_coverage` 指标把这些都当 miss 计算，导致 ingest 调整后的真实效果被掩盖。
2. **规模太小**：15 个用例不足以覆盖 1203 个 wiki 页面的话题分布，调整 ingest 提示词后看不出统计差异。

ingest 至少有两步产出：
- **源页**（`wiki/sources/<原文件名>.md`）—— 原始材料的章节摘要
- **衍生页**（`concepts/`、`scenarios/`、`lessons/`、`entities/`）—— LLM 提炼出的主题页

召回评测应该分别评估这两层，才能定位是检索问题还是 ingest 提示词问题。

## 整体流程

```
[Step 1: 生成]
  generate_test_cases.py（改造）
    └─ for each raw/sources/*.md:
         ├─ LLM 读取源材料 → 生成 1-2 个 question
         ├─ must = ["wiki/sources/<对应源页>.md"]  ← 从文件名推
         └─ LLM 看该源材料的 wiki 衍生页标题 → 选 1-5 个 should
    └─ 输出 test_cases/parenting_books_v2.json（100 个用例）

[Step 2: 评测]
  rag_eval.py（改造）
    └─ for each case:
         ├─ POST /api/v1/projects/{id}/search (topK=10)
         ├─ retrieved_files = [r.path for r in results]
         ├─ source_hit@K = 任意 must 命中 retrieved[:K]
         ├─ derived_hit@K = 任意 should 命中 retrieved[:K]
         └─ 同时算 K=5 和 K=10
    └─ 输出 results/<project>_<timestamp>.json

[Step 3: 汇总]
  汇总 source_hit_rate@5, source_hit_rate@10,
        derived_hit_rate@5, derived_hit_rate@10
  + 失败用例列表（debug 用）
```

## 测试用例 Schema

```json
{
  "id": "pbv2_001",
  "question": "宝宝2个月纯母乳喂养，需要补维生素D吗？每天多少？",
  "category": "fact|number|scenario|concept",
  "difficulty": "easy|medium|hard",
  "expected_sources": {
    "must": ["wiki/sources/崔玉涛宝贝健康公开课-01-*.md"],
    "should": [
      "wiki/concepts/婴儿维生素D补充指南.md",
      "wiki/scenarios/纯母乳喂养宝宝营养补充.md"
    ]
  },
  "keywords": ["维生素D", "剂量", "母乳"],
  "note": "考察营养素补充时机与剂量",
  "source_file": "崔玉涛宝贝健康公开课-01-宝宝维生素D补充.md"
}
```

### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 唯一 ID，前缀 `pbv2_` |
| `question` | string | 用户问题 |
| `category` | enum | `fact` / `number` / `scenario` / `concept` |
| `difficulty` | enum | `easy` / `medium` / `hard` |
| `expected_sources.must` | string[] | 源页 glob，至少 1 个；命中即 source_hit |
| `expected_sources.should` | string[] | 衍生页精确路径或 glob，0-5 个；命中即 derived_hit |
| `keywords` | string[] | 检索关键词（辅助调试） |
| `note` | string | 测试目的说明 |
| `source_file` | string | 生成时对应的 raw 源文件名 |

### 与旧 schema 的差异

- `expected_sources` 从 `["glob"]` 数组 → `{must: [...], should: [...]}` 对象
- 新增 `source_file` 字段
- 删除旧字段 `expected_answers`、`expected_entities`（召回评测用不到）

### 匹配规则

- `must` / `should` 都支持 glob（`*.md`）和精确路径
- 一个用例至少有 1 个 `must`；`should` 可空（少数源材料可能没衍生页）
- 复用现有 `glob_to_regex` + `expand_expected_sources` 逻辑

### 向后兼容

- 旧 `parenting_books.json` 保留，评测脚本检测 schema 版本（数组→v1，对象→v2）分别处理
- 评测 v1 用例时 `must = expected_sources`、`should = []`，避免误判衍生页
- 15 个旧用例可继续作为历史 baseline 对照

## 评测指标与输出

### Per-case 输出

```json
{
  "case_id": "pbv2_001",
  "question": "...",
  "retrieved_files": ["wiki/concepts/...", "wiki/sources/...", ...],
  "source_hit@5": true,
  "source_hit@10": true,
  "derived_hit@5": true,
  "derived_hit@10": false,
  "must_matched": ["wiki/sources/崔玉涛宝贝健康公开课-01-*.md"],
  "should_matched": ["wiki/concepts/婴儿维生素D补充指南.md"],
  "should_missing": ["wiki/scenarios/纯母乳喂养宝宝营养补充.md"]
}
```

### 汇总输出

```json
{
  "project": "ParentingBooks",
  "schema_version": "v2",
  "total_cases": 100,
  "timestamp": "2026-06-25T16:00:00",
  "summary": {
    "source_hit_rate@5": 0.91,
    "source_hit_rate@10": 0.95,
    "derived_hit_rate@5": 0.78,
    "derived_hit_rate@10": 0.86
  },
  "failures": {
    "source_miss@10": ["pbv2_023", "pbv2_047"],
    "derived_miss@10": ["pbv2_012", "pbv2_034"]
  }
}
```

### 指标定义

- `source_hit@K` = `must` 中任意 pattern 命中 `retrieved[:K]` → `true`
- `derived_hit@K` = `should` 非空时，任意 pattern 命中 `retrieved[:K]` → `true`；`should` 为空时该 case 不计入 derived 分母
- `rate@K` = 命中数 / 总数（`should` 空的 case 在 derived 分母中排除）

### failures 列表作用

- `source_miss@10` → 该源页没被召回，看检索关键词是否过偏 / 源页标题是否清晰
- `derived_miss@10` → 衍生页未召回或未生成，需要检查 ingest 提示词与页面命名

### 与旧版兼容

- 旧 schema v1 用例跑同一脚本：`must = expected_sources`、`should = []`、`derived_hit_*` 跳过、`source_hit_*` 仍正常计算
- 15 个旧用例可继续作为历史 baseline 对照

## 生成器细节

### 流程

```
Step 1: 枚举 raw/sources/*.md
  for each source_file:
    ├─ 推导 must: "wiki/sources/<basename>.md"
    ├─ 找该源对应的衍生页（frontmatter source-refs 引用该源）
    │   → 候选 derived_pages[]
    └─ LLM 调用 1 次：
         输入 = 源材料摘要 + 候选衍生页标题列表
         输出 = 1-2 个 {question, category, difficulty, should[], keywords, note}
              should 从候选衍生页中选 0-5 个（无候选时为空），
              避免 LLM 凭空捏造路径

Step 2: 过滤与平衡
  ├─ 删除 must 无法 glob 命中实际文件的用例
  ├─ category 分布检查（避免全是 fact）：scenario≥30%, fact≤40%
  └─ 总数到 100 即停（按源文件顺序遍历）

Step 3: 输出 parenting_books_v2.json
```

### 关键约束

- `should` 必须从**实际存在的衍生页**中选，不接受 LLM 自创路径（防止路径不存在导致永远 derived_miss）
- 推导衍生页的方法：扫描所有 `wiki/{concepts,scenarios,lessons,entities}/*.md` 的 frontmatter `source-refs` 字段（ingest 时已写入），看是否引用当前源文件
- 若某源材料无衍生页（罕见），`should = []`，仍保留该用例（只评 source_hit）
- 一次 LLM 调用同时生成问题与选 `should`，避免多次调用成本

### category 分布约束

- `scenario` ≥ 30%（场景应对是核心使用场景）
- `fact` ≤ 40%（避免都是定义类问题）
- 不满足时跳过部分 `fact` 用例，继续从后续源文件生成

## 文件改动清单

| 文件 | 改动 |
|------|------|
| `overlay/eval/generate_test_cases.py` | 新增 v2 schema 生成逻辑、`source-refs` 扫描器、`should` 路径验证、`--target-count` 参数 |
| `overlay/eval/rag_eval.py` | 改造 `eval_retrieval` 支持 v2 schema 双层匹配、新增 `source_hit@K` / `derived_hit@K`、保留 v1 兼容路径 |
| `overlay/eval/test_cases/parenting_books_v2.json` | 生成产物（100 用例） |
| `overlay/eval/tests/test_p0.py` | 新增 v2 schema 单元测试（双层匹配、glob、`should` 空处理、v1 兼容） |

## CLI 用法

### 生成 100 个 v2 用例

```bash
python overlay/eval/generate_test_cases.py \
  --project ~/overseas-github/llm_wiki_projects/ParentingBooks \
  --config overlay/config/server.local.json \
  --output overlay/eval/test_cases/parenting_books_v2.json \
  --mode auto \
  --target-count 100
```

### 跑召回评测

```bash
python overlay/eval/rag_eval.py \
  --project ParentingBooks \
  --test-cases overlay/eval/test_cases/parenting_books_v2.json \
  --mode retrieval \
  --output overlay/eval/results
```

## 不在本次范围内

- 生成结果人工抽检流程（用户选了"全自动"）
- 跨版本自动 diff（用户选了"单点"）
- CivilCareer 用例生成（本次只 ParentingBooks）
- chat 评测改造（本次只召回）

## 验收标准

1. `parenting_books_v2.json` 包含 100 个用例，schema 符合 §3 定义
2. `should` 中所有路径在 `wiki/` 下实际存在
3. `category` 分布满足 `scenario≥30%`、`fact≤40%`
4. `rag_eval.py` 跑 v2 用例输出 `source_hit_rate@5/@10` + `derived_hit_rate@5/@10`
5. 旧 `parenting_books.json` 仍能跑通，`source_hit_rate@10` 与 baseline 73.3% 量级一致
6. 新增单元测试全部通过
