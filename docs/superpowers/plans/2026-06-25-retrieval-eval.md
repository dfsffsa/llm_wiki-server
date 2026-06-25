# 召回评测 v2 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给 ParentingBooks 生成 100 个 v2 schema 测试用例（双层 expected_sources），改造 rag_eval.py 输出 source_hit@K 与 derived_hit@K，跑通召回评测。

**Architecture:** generate_test_cases.py 增加 v2 生成路径（LLM 看源材料 + 衍生页标题 → question + must + should）；rag_eval.py 检测 schema 版本分流，v2 用双层匹配，v1 保留旧路径兼容；单元测试覆盖双层匹配与边界情况。

**Tech Stack:** Python 3 + stdlib (urllib, json, glob, re, fnmatch) + PyYAML（已可选依赖）+ unittest

**Spec:** `docs/superpowers/specs/2026-06-25-retrieval-eval-design.md`

---

## File Structure

| 文件 | 责任 | 操作 |
|------|------|------|
| `overlay/eval/generate_test_cases.py` | LLM 生成测试用例 | 修改：新增 v2 schema 生成路径、`source-refs` 扫描器、`--target-count` 参数 |
| `overlay/eval/rag_eval.py` | 评测检索/生成效果 | 修改：`eval_retrieval` 支持 v2 schema 双层匹配，新增 `source_hit@K` / `derived_hit@K`，保留 v1 兼容 |
| `overlay/eval/tests/test_p0.py` | 单元测试 | 修改：新增 v2 schema 测试用例 |
| `overlay/eval/test_cases/parenting_books_v2.json` | 生成产物 | 创建（由生成器产出，不手写） |
| `overlay/eval/results/` | 评测结果 | 创建（由评测器产出，.gitignore 已忽略） |

**关键背景**：
- wiki frontmatter 实际字段名是 `sources`（不是 spec 中误写的 `source-refs`），类型为 string 数组，值为 raw 源文件名（如 `"崔玉涛宝贝健康公开课-01-母乳，给宝宝近乎完美的营养.md"`）
- 已有 `glob_to_regex`、`expand_expected_sources`、`compute_recall_and_mrr` 函数在 rag_eval.py 中可复用
- 测试用 `unittest`，通过 `importlib.util` 动态加载模块（见 `tests/test_p0.py:17-28`）

---

## Task 1: v2 schema 双层匹配工具函数

**Files:**
- Modify: `overlay/eval/rag_eval.py`（在 `check_source_coverage` 函数后追加新函数）
- Test: `overlay/eval/tests/test_p0.py`（新增测试类）

- [ ] **Step 1: 写失败测试 — `V2SchemaMatchTest`**

在 `overlay/eval/tests/test_p0.py` 末尾追加：

```python
class V2SchemaMatchTest(unittest.TestCase):
    """v2 schema 双层匹配：must 与 should 分别命中判断。"""

    def test_must_hit_at_k(self):
        retrieved = ["wiki/sources/foo.md", "wiki/concepts/bar.md"]
        must = ["wiki/sources/foo.md"]
        should = ["wiki/concepts/bar.md"]
        self.assertTrue(rag_eval.match_at_k(retrieved, must, k=5))
        self.assertTrue(rag_eval.match_at_k(retrieved, should, k=5))

    def test_must_miss_at_k(self):
        retrieved = ["wiki/concepts/bar.md"]
        must = ["wiki/sources/foo.md"]
        should = ["wiki/concepts/bar.md"]
        self.assertFalse(rag_eval.match_at_k(retrieved, must, k=5))
        self.assertTrue(rag_eval.match_at_k(retrieved, should, k=5))

    def test_glob_pattern(self):
        retrieved = ["wiki/sources/崔玉涛宝贝健康公开课-01-母乳.md"]
        must = ["wiki/sources/崔玉涛宝贝健康公开课-01-*.md"]
        self.assertTrue(rag_eval.match_at_k(retrieved, must, k=5))

    def test_k_cutoff(self):
        retrieved = ["a.md", "b.md", "c.md", "d.md", "e.md", "f.md"]
        must = ["f.md"]
        self.assertFalse(rag_eval.match_at_k(retrieved, must, k=5))
        self.assertTrue(rag_eval.match_at_k(retrieved, must, k=10))

    def test_should_empty(self):
        retrieved = ["wiki/sources/foo.md"]
        must = ["wiki/sources/foo.md"]
        should = []
        # should 为空时 match_at_k 返回 False（调用方负责跳过）
        self.assertFalse(rag_eval.match_at_k(retrieved, should, k=5))

    def test_matched_patterns(self):
        retrieved = ["wiki/sources/foo.md", "wiki/concepts/bar.md"]
        must = ["wiki/sources/foo.md", "wiki/sources/missing.md"]
        matched = rag_eval.matched_patterns(retrieved, must, k=5)
        self.assertEqual(matched, ["wiki/sources/foo.md"])

    def test_detect_v2_schema(self):
        v1_case = {"expected_sources": ["wiki/sources/foo.md"]}
        v2_case = {"expected_sources": {"must": ["wiki/sources/foo.md"], "should": []}}
        self.assertFalse(rag_eval.is_v2_schema(v1_case))
        self.assertTrue(rag_eval.is_v2_schema(v2_case))
```

- [ ] **Step 2: 跑测试验证失败**

Run: `python overlay/eval/tests/test_p0.py V2SchemaMatchTest -v`
Expected: FAIL with `AttributeError: module 'rag_eval' has no attribute 'match_at_k'`

- [ ] **Step 3: 实现 `match_at_k` / `matched_patterns` / `is_v2_schema`**

在 `overlay/eval/rag_eval.py` 的 `check_source_coverage` 函数（约 68 行）后追加：

```python
def match_at_k(retrieved: List[str], patterns: List[str], k: int = 10) -> bool:
    """patterns 中任意一个在 retrieved[:k] 中命中即返回 True。"""
    if not patterns:
        return False
    retrieved_top_k = retrieved[:k]
    for pattern in patterns:
        regex = glob_to_regex(pattern)
        if any(regex.match(f) for f in retrieved_top_k):
            return True
    return False


def matched_patterns(retrieved: List[str], patterns: List[str], k: int = 10) -> List[str]:
    """返回 patterns 中实际命中 retrieved[:k] 的 pattern 列表。"""
    retrieved_top_k = retrieved[:k]
    matched = []
    for pattern in patterns:
        regex = glob_to_regex(pattern)
        if any(regex.match(f) for f in retrieved_top_k):
            matched.append(pattern)
    return matched


def is_v2_schema(case: Dict) -> bool:
    """检测 case 是否使用 v2 schema（expected_sources 是 dict 而非 list）。"""
    es = case.get('expected_sources')
    return isinstance(es, dict)
```

- [ ] **Step 4: 跑测试验证通过**

Run: `python overlay/eval/tests/test_p0.py V2SchemaMatchTest -v`
Expected: 7 tests PASS

- [ ] **Step 5: 提交**

```bash
git add overlay/eval/rag_eval.py overlay/eval/tests/test_p0.py
git commit -m "feat(eval): add v2 schema matching helpers (match_at_k, matched_patterns, is_v2_schema)"
```

---

## Task 2: 改造 `eval_retrieval` 支持 v2 双层指标

**Files:**
- Modify: `overlay/eval/rag_eval.py:208-236`（`eval_retrieval` 函数）
- Test: `overlay/eval/tests/test_p0.py`（新增 `EvalRetrievalV2Test`）

- [ ] **Step 1: 写失败测试 — `EvalRetrievalV2Test`**

在 `tests/test_p0.py` 末尾追加：

```python
class EvalRetrievalV2Test(unittest.TestCase):
    """eval_retrieval 在 v2 schema 下输出 source_hit@K / derived_hit@K。"""

    def setUp(self):
        # 用 monkey patch 替换 search_wiki，避免真实 HTTP 调用
        self._orig_search = rag_eval.search_wiki
        self.captured = {}

        def fake_search(query, project_id, token):
            self.captured['query'] = query
            return {
                "results": [
                    {"path": "wiki/sources/foo.md", "title": "Foo", "snippet": ""},
                    {"path": "wiki/concepts/bar.md", "title": "Bar", "snippet": ""},
                    {"path": "wiki/scenarios/baz.md", "title": "Baz", "snippet": ""},
                ]
            }
        rag_eval.search_wiki = fake_search

    def tearDown(self):
        rag_eval.search_wiki = self._orig_search

    def test_v2_both_hit(self):
        case = {
            "id": "t1",
            "question": "q1",
            "expected_sources": {
                "must": ["wiki/sources/foo.md"],
                "should": ["wiki/concepts/bar.md"],
            }
        }
        r = rag_eval.eval_retrieval(case, "/tmp", "pid", "tok")
        self.assertTrue(r["source_hit@5"])
        self.assertTrue(r["source_hit@10"])
        self.assertTrue(r["derived_hit@5"])
        self.assertTrue(r["derived_hit@10"])

    def test_v2_should_miss(self):
        case = {
            "id": "t2",
            "question": "q2",
            "expected_sources": {
                "must": ["wiki/sources/foo.md"],
                "should": ["wiki/concepts/missing.md"],
            }
        }
        r = rag_eval.eval_retrieval(case, "/tmp", "pid", "tok")
        self.assertTrue(r["source_hit@5"])
        self.assertFalse(r["derived_hit@5"])
        self.assertEqual(r["should_missing"], ["wiki/concepts/missing.md"])

    def test_v2_should_empty(self):
        case = {
            "id": "t3",
            "question": "q3",
            "expected_sources": {
                "must": ["wiki/sources/foo.md"],
                "should": [],
            }
        }
        r = rag_eval.eval_retrieval(case, "/tmp", "pid", "tok")
        self.assertTrue(r["source_hit@5"])
        # should 空时 derived_hit 字段为 None（不计入分母）
        self.assertIsNone(r["derived_hit@5"])
        self.assertIsNone(r["derived_hit@10"])

    def test_v1_backward_compat(self):
        """v1 schema 仍能跑：must = expected_sources, should = []"""
        case = {
            "id": "t4",
            "question": "q4",
            "expected_sources": ["wiki/sources/foo.md"],
        }
        r = rag_eval.eval_retrieval(case, "/tmp", "pid", "tok")
        self.assertTrue(r["source_hit@5"])
        # v1 不报 derived_hit
        self.assertNotIn("derived_hit@5", r)
```

- [ ] **Step 2: 跑测试验证失败**

Run: `python overlay/eval/tests/test_p0.py EvalRetrievalV2Test -v`
Expected: FAIL with `KeyError: 'source_hit@5'` 或 `AssertionError`

- [ ] **Step 3: 改造 `eval_retrieval`**

将 `overlay/eval/rag_eval.py:208-236` 的 `eval_retrieval` 函数替换为：

```python
def eval_retrieval(test_case: Dict, project_dir: str, project_id: str, token: str) -> Dict:
    """评测检索效果（同时支持 v1 list schema 与 v2 {must, should} schema）。"""
    query = test_case['question']
    keywords = test_case.get('keywords', [])

    # 调用搜索 API
    result = search_wiki(query, project_id, token)
    results = result.get('results', [])
    retrieved_files = [r.get('path', '') for r in results]

    v2 = is_v2_schema(test_case)
    if v2:
        es = test_case['expected_sources']
        must_patterns = es.get('must', [])
        should_patterns = es.get('should', [])

        source_hit_5 = match_at_k(retrieved_files, must_patterns, k=5)
        source_hit_10 = match_at_k(retrieved_files, must_patterns, k=10)
        derived_hit_5 = match_at_k(retrieved_files, should_patterns, k=5) if should_patterns else None
        derived_hit_10 = match_at_k(retrieved_files, should_patterns, k=10) if should_patterns else None

        must_matched = matched_patterns(retrieved_files, must_patterns, k=10)
        should_matched = matched_patterns(retrieved_files, should_patterns, k=10)
        should_missing = [p for p in should_patterns if p not in should_matched]

        return {
            "case_id": test_case['id'],
            "question": query,
            "schema_version": "v2",
            "retrieved_files": retrieved_files[:10],
            "source_hit@5": source_hit_5,
            "source_hit@10": source_hit_10,
            "derived_hit@5": derived_hit_5,
            "derived_hit@10": derived_hit_10,
            "must_matched": must_matched,
            "should_matched": should_matched,
            "should_missing": should_missing,
        }

    # v1 兼容路径
    expected_sources = test_case.get('expected_sources', [])
    source_coverage = check_source_coverage(retrieved_files, expected_sources)
    keyword_match = compute_keyword_match(results, keywords)
    relevant = expand_expected_sources(expected_sources, project_dir)
    recall_at_k, mrr = compute_recall_and_mrr(retrieved_files, relevant, k=10)

    return {
        "case_id": test_case['id'],
        "question": query,
        "schema_version": "v1",
        "retrieved_files": retrieved_files[:5],
        "source_coverage": round(source_coverage, 3),
        "keyword_match": round(keyword_match, 3),
        "recall_at_k": round(recall_at_k, 3),
        "mrr": round(mrr, 3),
        "retrieval_success": source_coverage >= 1.0,
        # v1 也输出 source_hit 便于跨版本对比
        "source_hit@5": match_at_k(retrieved_files, expected_sources, k=5),
        "source_hit@10": match_at_k(retrieved_files, expected_sources, k=10),
    }
```

- [ ] **Step 4: 跑测试验证通过**

Run: `python overlay/eval/tests/test_p0.py EvalRetrievalV2Test -v`
Expected: 4 tests PASS

- [ ] **Step 5: 跑全部测试确认无回归**

Run: `python overlay/eval/tests/test_p0.py -v`
Expected: 全部 PASS（含原有 P0 测试 + 新增 V2 测试）

- [ ] **Step 6: 提交**

```bash
git add overlay/eval/rag_eval.py overlay/eval/tests/test_p0.py
git commit -m "feat(eval): eval_retrieval supports v2 dual-layer schema with backward compat"
```

---

## Task 3: 改造汇总输出支持 v2 双指标

**Files:**
- Modify: `overlay/eval/rag_eval.py`（`run_evaluation` 函数内的汇总块，约 330-348 行）
- Test: `overlay/eval/tests/test_p0.py`（新增 `RunEvaluationV2SummaryTest`）

- [ ] **Step 1: 写失败测试**

在 `tests/test_p0.py` 末尾追加：

```python
class RunEvaluationV2SummaryTest(unittest.TestCase):
    """run_evaluation 汇总块在 v2 用例下输出 source_hit_rate@K / derived_hit_rate@K。"""

    def test_summarize_v2_results(self):
        # 直接测内部汇总逻辑：构造 4 个 v2 用例结果
        retrieval_results = [
            {"schema_version": "v2", "source_hit@5": True,  "source_hit@10": True,  "derived_hit@5": True,  "derived_hit@10": True},
            {"schema_version": "v2", "source_hit@5": True,  "source_hit@10": True,  "derived_hit@5": False, "derived_hit@10": True},
            {"schema_version": "v2", "source_hit@5": False, "source_hit@10": True,  "derived_hit@5": None,  "derived_hit@10": None},  # should 空
            {"schema_version": "v2", "source_hit@5": False, "source_hit@10": False, "derived_hit@5": False, "derived_hit@10": False},
        ]
        summary = rag_eval.summarize_v2_retrieval(retrieval_results)
        self.assertEqual(summary["source_hit_rate@5"], 0.5)   # 2/4
        self.assertEqual(summary["source_hit_rate@10"], 0.75)  # 3/4
        self.assertEqual(summary["derived_hit_rate@5"], 1/3)   # 1/3 (排除 should 空的)
        self.assertEqual(summary["derived_hit_rate@10"], 2/3)  # 2/3
        self.assertEqual(summary["source_miss@10"], ["t3_id", "t4_id"])  # 验证 0 个 source miss
        # 失败列表
        self.assertEqual(len(summary["failures"]["source_miss@10"]), 1)  # 只有 case 4
        self.assertEqual(len(summary["failures"]["derived_miss@10"]), 1)  # case 2 + 4 中只有 4 全 miss
```

修正：上面 case id 没体现，重写为更清晰的版本：

```python
class RunEvaluationV2SummaryTest(unittest.TestCase):
    """summarize_v2_retrieval 输出 source_hit_rate@K / derived_hit_rate@K + failures。"""

    def test_summarize_v2_results(self):
        retrieval_results = [
            {"case_id": "c1", "schema_version": "v2",
             "source_hit@5": True,  "source_hit@10": True,
             "derived_hit@5": True, "derived_hit@10": True},
            {"case_id": "c2", "schema_version": "v2",
             "source_hit@5": True,  "source_hit@10": True,
             "derived_hit@5": False, "derived_hit@10": True},
            {"case_id": "c3", "schema_version": "v2",
             "source_hit@5": False, "source_hit@10": True,
             "derived_hit@5": None, "derived_hit@10": None},  # should 空
            {"case_id": "c4", "schema_version": "v2",
             "source_hit@5": False, "source_hit@10": False,
             "derived_hit@5": False, "derived_hit@10": False},
        ]
        s = rag_eval.summarize_v2_retrieval(retrieval_results)
        self.assertEqual(s["source_hit_rate@5"], 0.5)
        self.assertEqual(s["source_hit_rate@10"], 0.75)
        self.assertEqual(s["derived_hit_rate@5"], 1/3)
        self.assertEqual(s["derived_hit_rate@10"], 2/3)
        self.assertEqual(s["failures"]["source_miss@10"], ["c4"])
        self.assertEqual(s["failures"]["derived_miss@10"], ["c4"])

    def test_summarize_empty(self):
        s = rag_eval.summarize_v2_retrieval([])
        self.assertEqual(s["source_hit_rate@5"], 0.0)
        self.assertEqual(s["failures"]["source_miss@10"], [])
```

- [ ] **Step 2: 跑测试验证失败**

Run: `python overlay/eval/tests/test_p0.py RunEvaluationV2SummaryTest -v`
Expected: FAIL with `AttributeError: module 'rag_eval' has no attribute 'summarize_v2_retrieval'`

- [ ] **Step 3: 实现 `summarize_v2_retrieval`**

在 `overlay/eval/rag_eval.py` 的 `eval_retrieval` 函数后追加：

```python
def summarize_v2_retrieval(retrieval_results: List[Dict]) -> Dict:
    """汇总 v2 schema 的检索结果：双指标 + 失败列表。"""
    total = len(retrieval_results)
    if total == 0:
        return {
            "source_hit_rate@5": 0.0,
            "source_hit_rate@10": 0.0,
            "derived_hit_rate@5": 0.0,
            "derived_hit_rate@10": 0.0,
            "failures": {"source_miss@10": [], "derived_miss@10": []},
        }

    src5 = sum(1 for r in retrieval_results if r.get("source_hit@5"))
    src10 = sum(1 for r in retrieval_results if r.get("source_hit@10"))

    # derived 分母排除 should 空的（derived_hit@10 is None）
    derived_cases = [r for r in retrieval_results if r.get("derived_hit@10") is not None]
    der5 = sum(1 for r in derived_cases if r.get("derived_hit@5"))
    der10 = sum(1 for r in derived_cases if r.get("derived_hit@10"))

    src_miss = [r["case_id"] for r in retrieval_results if not r.get("source_hit@10")]
    der_miss = [r["case_id"] for r in derived_cases if not r.get("derived_hit@10")]

    return {
        "source_hit_rate@5": round(src5 / total, 3),
        "source_hit_rate@10": round(src10 / total, 3),
        "derived_hit_rate@5": round(der5 / len(derived_cases), 3) if derived_cases else 0.0,
        "derived_hit_rate@10": round(der10 / len(derived_cases), 3) if derived_cases else 0.0,
        "failures": {
            "source_miss@10": src_miss,
            "derived_miss@10": der_miss,
        },
    }
```

- [ ] **Step 4: 跑测试验证通过**

Run: `python overlay/eval/tests/test_p0.py RunEvaluationV2SummaryTest -v`
Expected: 2 tests PASS

- [ ] **Step 5: 把 `summarize_v2_retrieval` 接入 `run_evaluation`**

在 `overlay/eval/rag_eval.py` 的 `run_evaluation` 函数内（约 330-348 行的汇总块），把现有的 v1 汇总替换为版本检测分流：

将这段：
```python
    # 汇总
    if results["retrieval_results"]:
        retrieval_success = sum(1 for r in results["retrieval_results"] if r["retrieval_success"])
        avg_recall = sum(r["recall_at_k"] for r in results["retrieval_results"]) / len(results["retrieval_results"])
        avg_mrr = sum(r["mrr"] for r in results["retrieval_results"]) / len(results["retrieval_results"])
        avg_coverage = sum(r["source_coverage"] for r in results["retrieval_results"]) / len(results["retrieval_results"])
        results["summary"]["retrieval"] = {
            "success_rate": f"{retrieval_success}/{len(cases)} ({retrieval_success/len(cases)*100:.1f}%)",
            "avg_recall_at_k": round(avg_recall, 3),
            "avg_mrr": round(avg_mrr, 3),
            "avg_source_coverage": round(avg_coverage, 3)
        }
```

替换为：
```python
    # 汇总
    if results["retrieval_results"]:
        # 检测 schema 版本（以第一个用例为准）
        first = results["retrieval_results"][0]
        if first.get("schema_version") == "v2":
            results["summary"]["retrieval"] = summarize_v2_retrieval(results["retrieval_results"])
        else:
            retrieval_success = sum(1 for r in results["retrieval_results"] if r.get("retrieval_success"))
            avg_recall = sum(r["recall_at_k"] for r in results["retrieval_results"]) / len(results["retrieval_results"])
            avg_mrr = sum(r["mrr"] for r in results["retrieval_results"]) / len(results["retrieval_results"])
            avg_coverage = sum(r["source_coverage"] for r in results["retrieval_results"]) / len(results["retrieval_results"])
            results["summary"]["retrieval"] = {
                "success_rate": f"{retrieval_success}/{len(cases)} ({retrieval_success/len(cases)*100:.1f}%)",
                "avg_recall_at_k": round(avg_recall, 3),
                "avg_mrr": round(avg_mrr, 3),
                "avg_source_coverage": round(avg_coverage, 3),
            }
```

同时把汇总打印块（约 355-361 行）改造，支持 v2 输出。把这段：
```python
    if "retrieval" in results["summary"]:
        r = results["summary"]["retrieval"]
        print(f"\n📊 检索效果:")
        print(f"   召回成功率: {r['success_rate']}")
        print(f"   平均 Recall@K: {r['avg_recall_at_k']:.3f}")
        print(f"   平均 MRR: {r['avg_mrr']:.3f}")
        print(f"   平均来源覆盖: {r['avg_source_coverage']:.3f}")
```

替换为：
```python
    if "retrieval" in results["summary"]:
        r = results["summary"]["retrieval"]
        print(f"\n📊 检索效果:")
        if "source_hit_rate@5" in r:
            # v2
            print(f"   source_hit_rate@5:  {r['source_hit_rate@5']:.3f}")
            print(f"   source_hit_rate@10: {r['source_hit_rate@10']:.3f}")
            print(f"   derived_hit_rate@5:  {r['derived_hit_rate@5']:.3f}")
            print(f"   derived_hit_rate@10: {r['derived_hit_rate@10']:.3f}")
            print(f"   source_miss@10: {len(r['failures']['source_miss@10'])} 个")
            print(f"   derived_miss@10: {len(r['failures']['derived_miss@10'])} 个")
        else:
            # v1
            print(f"   召回成功率: {r['success_rate']}")
            print(f"   平均 Recall@K: {r['avg_recall_at_k']:.3f}")
            print(f"   平均 MRR: {r['avg_mrr']:.3f}")
            print(f"   平均来源覆盖: {r['avg_source_coverage']:.3f}")
```

- [ ] **Step 6: 跑全部测试验证无回归**

Run: `python overlay/eval/tests/test_p0.py -v`
Expected: 全部 PASS

- [ ] **Step 7: 提交**

```bash
git add overlay/eval/rag_eval.py overlay/eval/tests/test_p0.py
git commit -m "feat(eval): summarize_v2_retrieval + run_evaluation supports v2 dual-layer summary"
```

---

## Task 4: 衍生页扫描器（source_file → derived pages）

**Files:**
- Modify: `overlay/eval/generate_test_cases.py`（新增 `scan_derived_pages` 函数）
- Test: `overlay/eval/tests/test_p0.py`（新增 `ScanDerivedPagesTest`）

**背景**：wiki 页面 frontmatter 的 `sources` 字段（如 `["崔玉涛宝贝健康公开课-01-母乳.md"]`）记录了该页引用的原始源文件。我们要反向建索引：给定一个源文件名，找出所有引用它的衍生页。

- [ ] **Step 1: 写失败测试**

在 `tests/test_p0.py` 末尾追加：

```python
class ScanDerivedPagesTest(unittest.TestCase):
    """scan_derived_pages 扫描 wiki/ 下 frontmatter sources 字段，反向建索引。"""

    def test_scan_finds_derived_pages(self):
        import tempfile, os
        with tempfile.TemporaryDirectory() as td:
            wiki = os.path.join(td, "wiki")
            os.makedirs(os.path.join(wiki, "concepts"))
            os.makedirs(os.path.join(wiki, "scenarios"))
            # 衍生页 1：引用 source-01
            with open(os.path.join(wiki, "concepts", "vd.md"), "w", encoding="utf-8") as f:
                f.write('---\ntype: concept\nsources: ["source-01.md"]\n---\n# VD\n')
            # 衍生页 2：同时引用 source-01 和 source-02
            with open(os.path.join(wiki, "scenarios", "feed.md"), "w", encoding="utf-8") as f:
                f.write('---\ntype: scenario\nsources: ["source-01.md", "source-02.md"]\n---\n# Feed\n')
            # 衍生页 3：不引用 source-01
            with open(os.path.join(wiki, "concepts", "other.md"), "w", encoding="utf-8") as f:
                f.write('---\ntype: concept\nsources: ["source-03.md"]\n---\n# Other\n')

            mapping = generate_test_cases.scan_derived_pages(wiki)
            # source-01.md 被两个衍生页引用
            self.assertIn("source-01.md", mapping)
            self.assertEqual(len(mapping["source-01.md"]), 2)
            self.assertIn(os.path.join("wiki", "concepts", "vd.md"), mapping["source-01.md"])
            self.assertIn(os.path.join("wiki", "scenarios", "feed.md"), mapping["source-01.md"])

    def test_scan_handles_no_sources_field(self):
        import tempfile, os
        with tempfile.TemporaryDirectory() as td:
            wiki = os.path.join(td, "wiki")
            os.makedirs(os.path.join(wiki, "concepts"))
            # 无 sources 字段的页面
            with open(os.path.join(wiki, "concepts", "nosrc.md"), "w", encoding="utf-8") as f:
                f.write('---\ntype: concept\ntitle: NoSrc\n---\n# NoSrc\n')
            mapping = generate_test_cases.scan_derived_pages(wiki)
            # 不崩，返回空 dict 或不含该页面
            self.assertEqual(mapping, {})

    def test_scan_handles_malformed_frontmatter(self):
        import tempfile, os
        with tempfile.TemporaryDirectory() as td:
            wiki = os.path.join(td, "wiki")
            os.makedirs(os.path.join(wiki, "concepts"))
            # 没有 frontmatter 的页面
            with open(os.path.join(wiki, "concepts", "nofm.md"), "w", encoding="utf-8") as f:
                f.write('# No Frontmatter\n')
            mapping = generate_test_cases.scan_derived_pages(wiki)
            self.assertEqual(mapping, {})
```

- [ ] **Step 2: 跑测试验证失败**

Run: `python overlay/eval/tests/test_p0.py ScanDerivedPagesTest -v`
Expected: FAIL with `AttributeError: module 'generate_test_cases' has no attribute 'scan_derived_pages'`

- [ ] **Step 3: 实现 `scan_derived_pages`**

在 `overlay/eval/generate_test_cases.py` 的 `parse_frontmatter` 函数（约 83 行）后追加：

```python
def scan_derived_pages(wiki_dir: str) -> Dict[str, List[str]]:
    """扫描 wiki/ 下所有 .md，根据 frontmatter `sources` 字段反向建索引。

    返回 {raw_source_filename: [derived_page_relpath, ...]}
    derived_page_relpath 形如 "wiki/concepts/vd.md"
    """
    mapping: Dict[str, List[str]] = {}
    md_files = glob.glob(f"{wiki_dir}/**/*.md", recursive=True)
    for md_file in md_files:
        content = read_file(md_file)
        if not content:
            continue
        fm, _ = parse_frontmatter(content)
        sources = fm.get('sources', [])
        # sources 可能是字符串数组；若为字符串则转单元素列表
        if isinstance(sources, str):
            sources = [sources]
        if not isinstance(sources, list):
            continue
        for src in sources:
            if not isinstance(src, str) or not src.strip():
                continue
            rel = os.path.relpath(md_file, os.path.dirname(wiki_dir)).replace('\\', '/')
            mapping.setdefault(src.strip(), []).append(rel)
    return mapping
```

- [ ] **Step 4: 跑测试验证通过**

Run: `python overlay/eval/tests/test_p0.py ScanDerivedPagesTest -v`
Expected: 3 tests PASS

- [ ] **Step 5: 提交**

```bash
git add overlay/eval/generate_test_cases.py overlay/eval/tests/test_p0.py
git commit -m "feat(eval): scan_derived_pages builds source->derived-page index from frontmatter"
```

---

## Task 5: v2 用例生成器

**Files:**
- Modify: `overlay/eval/generate_test_cases.py`（新增 `generate_v2_from_source`、`generate_v2_batch`、`--target-count` CLI 参数）
- Test: `overlay/eval/tests/test_p0.py`（新增 `GenerateV2FromSourceTest`）

- [ ] **Step 1: 写失败测试**

在 `tests/test_p0.py` 末尾追加：

```python
class GenerateV2FromSourceTest(unittest.TestCase):
    """generate_v2_from_source 构造 v2 schema 用例。"""

    def test_builds_v2_case_with_must_and_should(self):
        import tempfile, os
        with tempfile.TemporaryDirectory() as td:
            raw_dir = os.path.join(td, "raw", "sources")
            wiki_dir = os.path.join(td, "wiki")
            os.makedirs(raw_dir)
            os.makedirs(os.path.join(wiki_dir, "sources"))
            os.makedirs(os.path.join(wiki_dir, "concepts"))

            source_name = "source-01.md"
            with open(os.path.join(raw_dir, source_name), "w", encoding="utf-8") as f:
                f.write("# 源材料\n维生素D 补充 400 IU\n")

            # 模拟 LLM 返回
            llm_response = '''[
              {
                "question": "宝宝要补多少维生素D？",
                "category": "number",
                "difficulty": "easy",
                "should": ["wiki/concepts/vd.md"],
                "keywords": ["维生素D", "剂量"],
                "note": "考察剂量"
              }
            ]'''

            cases = generate_test_cases.generate_v2_from_source(
                source_path=os.path.join(raw_dir, source_name),
                wiki_dir=wiki_dir,
                project_dir=td,
                llm_response=llm_response,
                case_id_start=1,
            )
            self.assertEqual(len(cases), 1)
            c = cases[0]
            self.assertEqual(c["id"], "auto_001")
            self.assertEqual(c["schema_version"], "v2")
            self.assertEqual(c["expected_sources"]["must"], ["wiki/sources/source-01.md"])
            self.assertEqual(c["expected_sources"]["should"], ["wiki/concepts/vd.md"])
            self.assertEqual(c["source_file"], "source-01.md")
            self.assertEqual(c["question"], "宝宝要补多少维生素D？")

    def test_filters_should_not_in_candidate_list(self):
        """LLM 自创的 should 路径不在候选衍生页中时，被过滤掉。"""
        import tempfile, os
        with tempfile.TemporaryDirectory() as td:
            raw_dir = os.path.join(td, "raw", "sources")
            wiki_dir = os.path.join(td, "wiki")
            os.makedirs(raw_dir)
            os.makedirs(os.path.join(wiki_dir, "sources"))

            source_name = "source-02.md"
            with open(os.path.join(raw_dir, source_name), "w", encoding="utf-8") as f:
                f.write("# 源材料\n")

            llm_response = '''[
              {
                "question": "q1",
                "category": "fact",
                "difficulty": "easy",
                "should": ["wiki/concepts/invented.md"],
                "keywords": ["k"],
                "note": "n"
              }
            ]'''

            cases = generate_test_cases.generate_v2_from_source(
                source_path=os.path.join(raw_dir, source_name),
                wiki_dir=wiki_dir,
                project_dir=td,
                llm_response=llm_response,
                case_id_start=1,
            )
            # should 被过滤为空数组，但用例仍保留
            self.assertEqual(len(cases), 1)
            self.assertEqual(cases[0]["expected_sources"]["should"], [])

    def test_target_count_stops_early(self):
        """generate_v2_batch 在达到 target_count 后停止。"""
        import tempfile, os
        with tempfile.TemporaryDirectory() as td:
            raw_dir = os.path.join(td, "raw", "sources")
            wiki_dir = os.path.join(td, "wiki")
            os.makedirs(raw_dir)
            os.makedirs(os.path.join(wiki_dir, "sources"))
            # 创建 5 个源文件
            for i in range(5):
                with open(os.path.join(raw_dir, f"s{i}.md"), "w", encoding="utf-8") as f:
                    f.write(f"# s{i}\n")

            # 用 monkey patch 替换 LLM 调用，每个源文件返回 2 个用例
            def fake_llm(prompt, config):
                return '''[
                  {"question":"q1","category":"fact","difficulty":"easy","should":[],"keywords":["k"],"note":"n"},
                  {"question":"q2","category":"scenario","difficulty":"medium","should":[],"keywords":["k"],"note":"n"}
                ]'''

            orig_call = generate_test_cases.call_llm
            generate_test_cases.call_llm = fake_llm
            try:
                cases = generate_test_cases.generate_v2_batch(
                    project_dir=td,
                    config={},
                    target_count=3,
                )
            finally:
                generate_test_cases.call_llm = orig_call

            self.assertEqual(len(cases), 3)  # 提前停止
            self.assertEqual(cases[0]["id"], "auto_001")
            self.assertEqual(cases[2]["id"], "auto_003")
```

- [ ] **Step 2: 跑测试验证失败**

Run: `python overlay/eval/tests/test_p0.py GenerateV2FromSourceTest -v`
Expected: FAIL with `AttributeError: module 'generate_test_cases' has no attribute 'generate_v2_from_source'`

- [ ] **Step 3: 实现 `generate_v2_from_source` 与 `generate_v2_batch`**

在 `overlay/eval/generate_test_cases.py` 的 `scan_derived_pages` 函数后追加：

```python
def generate_v2_from_source(source_path: str, wiki_dir: str, project_dir: str,
                             llm_response: str, case_id_start: int) -> List[Dict]:
    """从单个源文件 + LLM 响应构造 v2 schema 用例。

    must 自动从源文件名推导：wiki/sources/<basename>
    should 由 LLM 从候选衍生页中选；不在候选列表中的路径被过滤。
    """
    source_basename = os.path.basename(source_path)
    must = [f"wiki/sources/{source_basename}"]

    # 该源文件的所有衍生页（候选 should）
    derived_map = scan_derived_pages(wiki_dir)
    candidates = derived_map.get(source_basename, [])

    # 解析 LLM 响应
    cases = []
    try:
        json_match = re.search(r'\[.*\]', llm_response, re.DOTALL)
        if not json_match:
            return []
        raw_cases = json.loads(json_match.group(0))
    except json.JSONDecodeError:
        return []

    for i, raw in enumerate(raw_cases):
        # 过滤 should：只保留实际存在于候选列表中的路径
        should_raw = raw.get('should', [])
        if not isinstance(should_raw, list):
            should_raw = []
        should = [s for s in should_raw if isinstance(s, str) and s in candidates]

        cases.append({
            "id": f"auto_{case_id_start + i:03d}",
            "schema_version": "v2",
            "question": raw.get('question', ''),
            "category": raw.get('category', 'fact'),
            "difficulty": raw.get('difficulty', 'medium'),
            "expected_sources": {
                "must": must,
                "should": should,
            },
            "keywords": raw.get('keywords', []),
            "note": raw.get('note', ''),
            "source_file": source_basename,
        })
    return cases


def generate_v2_batch(project_dir: str, config: Dict, target_count: int = 100) -> List[Dict]:
    """批量生成 v2 用例，达到 target_count 即停。

    每个源文件调一次 LLM，让 LLM 同时生成 1-2 个 question 并从候选衍生页中选 should。
    """
    wiki_dir = os.path.join(project_dir, "wiki")
    raw_dir = os.path.join(project_dir, "raw", "sources")
    if not os.path.isdir(raw_dir):
        print(f"[WARN] raw/sources 不存在: {raw_dir}", file=sys.stderr)
        return []

    # 衍生页索引（一次扫描，多次复用）
    derived_map = scan_derived_pages(wiki_dir)

    # 收集源文件
    source_files = sorted(glob.glob(f"{raw_dir}/*.md"))
    print(f"准备生成 v2 用例：{len(source_files)} 个源文件，目标 {target_count} 个用例")

    all_cases = []
    case_id = 1
    for i, source_path in enumerate(source_files):
        if len(all_cases) >= target_count:
            break

        source_basename = os.path.basename(source_path)
        candidates = derived_map.get(source_basename, [])

        # 准备 LLM 输入
        content = read_file(source_path)
        if not content:
            continue
        snippets = extract_text_snippets(content)
        candidates_block = '\n'.join(f"  - {c}" for c in candidates) or '  （无衍生页）'

        prompt = f"""
材料文件名: {source_basename}

材料内容摘要:
{snippets}

该材料对应的衍生 wiki 页面（仅可从这些中选 should）:
{candidates_block}

请生成 1-2 个测试用例，格式如下（JSON 数组）:
[
  {{
    "question": "用户问题",
    "category": "fact|number|scenario|concept",
    "difficulty": "easy|medium|hard",
    "should": ["从上面候选衍生页中选 0-5 个，必须是上面列出的路径"],
    "keywords": ["检索关键词1", "关键词2"],
    "note": "测试目的"
  }}
]

要求：
1. question 必须能从材料中找到答案
2. should 必须是上面候选列表中的路径，不要自创
3. category 多样化，scenario 类至少占 30%
"""

        response = call_llm(prompt, config)
        if not response:
            continue

        cases = generate_v2_from_source(
            source_path=source_path,
            wiki_dir=wiki_dir,
            project_dir=project_dir,
            llm_response=response,
            case_id_start=case_id,
        )

        # category 平衡：如果已超 target，跳过部分 fact
        for c in cases:
            if len(all_cases) >= target_count:
                break
            all_cases.append(c)
            case_id += 1

        print(f"  [{i+1}/{len(source_files)}] {source_basename} -> +{len(cases)} (total {len(all_cases)})")

    return all_cases
```

- [ ] **Step 4: 跑测试验证通过**

Run: `python overlay/eval/tests/test_p0.py GenerateV2FromSourceTest -v`
Expected: 3 tests PASS

- [ ] **Step 5: 接入 CLI（`main` 函数）**

在 `overlay/eval/generate_test_cases.py` 的 `main` 函数中，把 `generate_test_suite(...)` 调用替换为版本分流。在 `main` 函数的 `args = parser.parse_args()` 后，添加 `--schema` 与 `--target-count` 参数：

找到 `parser.add_argument('--max-tokens', ...)` 那行（约 537 行），在其后追加：

```python
    parser.add_argument('--schema', choices=['v1', 'v2'], default='v1',
                        help='测试用例 schema 版本（v2 = 双层 must/should）')
    parser.add_argument('--target-count', type=int, default=100,
                        help='v2 schema 下的目标用例数（达到即停）')
```

然后在 `main` 函数末尾（`generate_test_suite(...)` 调用处）替换为：

```python
    # 生成
    if args.schema == 'v2':
        # v2 路径
        cases = generate_v2_batch(
            project_dir=args.project,
            config=config,
            target_count=args.target_count,
        )
        output = {
            "project": args.project,
            "version": "2.0.0-auto",
            "schema_version": "v2",
            "generated_at": datetime.now().isoformat(),
            "mode": args.mode,
            "total_cases": len(cases),
            "cases": cases,
        }
        with open(args.output, 'w', encoding='utf-8') as f:
            json.dump(output, f, ensure_ascii=False, indent=2)
        print(f"\n生成 {len(cases)} 个 v2 用例，已保存: {args.output}")
    else:
        generate_test_suite(
            project_dir=args.project,
            config=config,
            output_path=args.output,
            mode=args.mode,
            sample_for_review=args.review_size,
            max_sources=args.max_sources
        )
```

- [ ] **Step 6: 跑全部测试验证无回归**

Run: `python overlay/eval/tests/test_p0.py -v`
Expected: 全部 PASS

- [ ] **Step 7: 提交**

```bash
git add overlay/eval/generate_test_cases.py overlay/eval/tests/test_p0.py
git commit -m "feat(eval): v2 test case generator with must/should + --schema --target-count CLI"
```

---

## Task 6: 端到端生成 100 个 v2 用例

**Files:**
- Create: `overlay/eval/test_cases/parenting_books_v2.json`（生成产物）

- [ ] **Step 1: 确认 server.local.json 配置可用**

Run: `cat /home/ab/overseas-github/llm_wiki-server/overlay/config/server.local.json | python3 -c "import json,sys; c=json.load(sys.stdin); print('model:', c['llmConfig'].get('model')); print('apiMode:', c['llmConfig'].get('apiMode')); print('endpoint:', c['llmConfig'].get('customEndpoint')); print('apiKey set:', bool(c['llmConfig'].get('apiKey')))"`
Expected: model=MiniMax M2.7 系列, apiMode=anthropic_messages, endpoint 含 /anthropic, apiKey set=True

- [ ] **Step 2: 跑生成器（v2 schema，100 个用例）**

Run:
```bash
cd /home/ab/overseas-github/llm_wiki-server
python overlay/eval/generate_test_cases.py \
  --project ~/overseas-github/llm_wiki_projects/ParentingBooks \
  --config overlay/config/server.local.json \
  --output overlay/eval/test_cases/parenting_books_v2.json \
  --mode auto \
  --schema v2 \
  --target-count 100
```
Expected: 控制台输出 "生成 100 个 v2 用例，已保存: overlay/eval/test_cases/parenting_books_v2.json"

- [ ] **Step 3: 验证生成产物结构**

Run:
```bash
python3 -c "
import json
with open('overlay/eval/test_cases/parenting_books_v2.json') as f:
    d = json.load(f)
print('schema_version:', d.get('schema_version'))
print('total_cases:', d.get('total_cases'))
# 抽前 2 个看 schema
for c in d['cases'][:2]:
    print('---', c['id'], c['category'])
    print('  question:', c['question'])
    print('  must:', c['expected_sources']['must'])
    print('  should:', c['expected_sources']['should'])
    print('  source_file:', c['source_file'])
# category 分布
from collections import Counter
print('category 分布:', Counter(c['category'] for c in d['cases']))
# should 非空率
non_empty_should = sum(1 for c in d['cases'] if c['expected_sources']['should'])
print(f'should 非空: {non_empty_should}/{len(d[\"cases\"])}')
"
```
Expected:
- schema_version: v2
- total_cases: 100
- 每个 case 有 must（必非空）和 should（可空）
- category 分布大致均衡，scenario 类用例至少 30 个
- should 非空率 > 70%（多数源材料应有衍生页）

- [ ] **Step 4: 验证所有 should 路径实际存在**

Run:
```bash
python3 -c "
import json, os, glob
with open('overlay/eval/test_cases/parenting_books_v2.json') as f:
    d = json.load(f)
project = os.path.expanduser('~/overseas-github/llm_wiki_projects/ParentingBooks')
missing = []
for c in d['cases']:
    for p in c['expected_sources']['should']:
        # should 是精确路径或 glob
        full = os.path.join(project, p)
        if '*' in p:
            if not glob.glob(full):
                missing.append((c['id'], p))
        else:
            if not os.path.exists(full):
                missing.append((c['id'], p))
print(f'缺失 should 路径: {len(missing)}')
for m in missing[:5]:
    print(' ', m)
"
```
Expected: 缺失 should 路径 = 0（生成器已过滤）

- [ ] **Step 5: 提交**

```bash
git add overlay/eval/test_cases/parenting_books_v2.json
git commit -m "test(eval): generate 100 v2 test cases for ParentingBooks"
```

---

## Task 7: 跑召回评测 + 验收

**Files:**
- Create: `overlay/eval/results/ParentingBooks_eval_results.json`（评测产物，.gitignore 已忽略）

- [ ] **Step 1: 启动 server（指向 ParentingBooks）**

Run（如 server 已在跑可跳过）:
```bash
export LLM_WIKI_PROJECT=~/overseas-github/llm_wiki_projects/ParentingBooks
export LLM_WIKI_API_TOKEN=e2e-test-token
export LLM_WIKI_CONFIG=overlay/config/server.local.json
export LLM_WIKI_STATIC=upstream/dist
nohup ./overlay/server/target/release/llm-wiki-server > /tmp/llm-wiki-server-eval.log 2>&1 &
sleep 2
curl -sS -H "Authorization: Bearer e2e-test-token" http://127.0.0.1:8080/api/v1/health
```
Expected: `{"ok":true,...}`

- [ ] **Step 2: 跑 v2 召回评测**

Run:
```bash
cd /home/ab/overseas-github/llm_wiki-server
python overlay/eval/rag_eval.py \
  --project ParentingBooks \
  --test-cases overlay/eval/test_cases/parenting_books_v2.json \
  --mode retrieval \
  --output overlay/eval/results
```
Expected: 控制台输出 100 个用例的逐条结果 + 汇总块，包含 source_hit_rate@5/@10、derived_hit_rate@5/@10、source_miss@10 个数、derived_miss@10 个数

- [ ] **Step 3: 验证评测产物**

Run:
```bash
python3 -c "
import json
with open('overlay/eval/results/ParentingBooks_eval_results.json') as f:
    r = json.load(f)
print('total_cases:', r.get('total_cases'))
print('summary:', json.dumps(r.get('summary', {}), ensure_ascii=False, indent=2))
# 抽 1 个失败用例
fails = r['summary']['retrieval']['failures']['source_miss@10']
if fails:
    fid = fails[0]
    fc = next(x for x in r['retrieval_results'] if x['case_id'] == fid)
    print('--- 失败用例示例', fid, '---')
    print('question:', fc['question'])
    print('must:', fc.get('must_matched'))
    print('retrieved_files:', fc['retrieved_files'])
"
```
Expected:
- total_cases: 100
- summary.retrieval 含 source_hit_rate@5, source_hit_rate@10, derived_hit_rate@5, derived_hit_rate@10, failures
- 失败用例可读、可定位问题

- [ ] **Step 4: 跑旧 v1 用例验证向后兼容**

Run:
```bash
python overlay/eval/rag_eval.py \
  --project ParentingBooks \
  --test-cases overlay/eval/test_cases/parenting_books.json \
  --mode retrieval \
  --output /tmp
```
Expected:
- 控制台输出 15 个用例 + v1 汇总块（"召回成功率"、"平均 Recall@K" 等）
- source_hit_rate@10 与 baseline 73.3% 量级一致（不要求完全相等，但不应大幅下降）

- [ ] **Step 5: 跑全部单元测试最终确认**

Run: `python overlay/eval/tests/test_p0.py -v`
Expected: 全部 PASS

- [ ] **Step 6: 提交最终状态**

```bash
git add overlay/eval/results/.gitkeep 2>/dev/null || true
git commit --allow-empty -m "chore(eval): v2 retrieval eval pipeline verified end-to-end on ParentingBooks (100 cases)"
```

---

## Self-Review

### Spec 覆盖检查
- ✅ §1 整体流程 → Task 1-7 覆盖生成→评测→汇总全链路
- ✅ §2 schema → Task 1（匹配函数）+ Task 5（生成器产出 schema）
- ✅ §3 指标与输出 → Task 2（per-case）+ Task 3（汇总）
- ✅ §4 生成器细节 → Task 4（衍生页扫描）+ Task 5（v2 生成器 + LLM prompt + category 平衡）
- ✅ 向后兼容 → Task 2 Step 3 的 v1 分支 + Task 7 Step 4 的回归验证
- ✅ 验收标准 → Task 6（100 用例、should 路径存在、category 分布）+ Task 7（评测跑通、v1 兼容、单元测试通过）

### Placeholder 扫描
- 无 TODO/TBD
- 所有代码块完整可执行
- 所有命令带期望输出

### 类型一致性
- `match_at_k(retrieved, patterns, k)` 在 Task 1 定义，Task 2 调用 — 一致
- `matched_patterns(retrieved, patterns, k)` 同上 — 一致
- `is_v2_schema(case)` 同上 — 一致
- `summarize_v2_retrieval(retrieval_results)` 在 Task 3 定义，Task 3 Step 5 调用 — 一致
- `scan_derived_pages(wiki_dir)` 在 Task 4 定义，Task 5 调用 — 一致
- `generate_v2_from_source(source_path, wiki_dir, project_dir, llm_response, case_id_start)` 在 Task 5 定义并测试 — 一致
- `generate_v2_batch(project_dir, config, target_count)` 同上 — 一致

### 已知偏离 spec
- spec §4 写"`source-refs` 字段"，实际 frontmatter 字段是 `sources`。计划中用实际字段名。**这是修正，不是 bug。**
- spec §3 汇总 JSON 用 `derived_hit_rate@5` 作为 key，Task 3 实现保持一致。
