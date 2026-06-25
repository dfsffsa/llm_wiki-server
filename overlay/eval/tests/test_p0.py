"""P0 评测指标修复单元测试

覆盖 AUDIT_2026-06-23.md 中 P0-1 ~ P0-6 的核心修复点。
"""

import importlib.util
import os
import sys
import tempfile
import unittest
from pathlib import Path

EVAL_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EVAL_DIR))


def _load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


rag_eval = _load_module("rag_eval", EVAL_DIR / "rag_eval.py")
ingest_check = _load_module("ingest_check", EVAL_DIR / "ingest_check.py")
generate_test_cases = _load_module(
    "generate_test_cases", EVAL_DIR / "generate_test_cases.py"
)


class P01RecallAndMRRTest(unittest.TestCase):
    """P0-1: recall_at_k 改为真正的 Recall@K，同时报告 MRR。"""

    def test_perfect_retrieval(self):
        relevant = {"wiki/sources/foo.md"}
        retrieved = ["wiki/sources/foo.md", "wiki/sources/bar.md"]
        recall, mrr = rag_eval.compute_recall_and_mrr(retrieved, relevant, k=10)
        self.assertEqual(recall, 1.0)
        self.assertEqual(mrr, 1.0)

    def test_second_relevant(self):
        relevant = {"wiki/sources/foo.md"}
        retrieved = ["wiki/sources/bar.md", "wiki/sources/foo.md"]
        recall, mrr = rag_eval.compute_recall_and_mrr(retrieved, relevant, k=10)
        self.assertEqual(recall, 1.0)
        self.assertEqual(mrr, 0.5)

    def test_miss(self):
        relevant = {"wiki/sources/foo.md"}
        retrieved = ["wiki/sources/bar.md"]
        recall, mrr = rag_eval.compute_recall_and_mrr(retrieved, relevant, k=10)
        self.assertEqual(recall, 0.0)
        self.assertEqual(mrr, 0.0)

    def test_partial_multi_relevant(self):
        relevant = {"wiki/sources/foo1.md", "wiki/sources/foo2.md"}
        retrieved = ["wiki/sources/foo1.md"]
        recall, mrr = rag_eval.compute_recall_and_mrr(retrieved, relevant, k=10)
        self.assertEqual(recall, 0.5)
        self.assertEqual(mrr, 1.0)


class P01ExpandExpectedSourcesTest(unittest.TestCase):
    """P0-1: expected_sources 中的 glob 应展开为实际文件集合。"""

    def test_glob_expansion(self):
        with tempfile.TemporaryDirectory() as project_dir:
            wiki = Path(project_dir) / "wiki"
            wiki.mkdir()
            (wiki / "foo1.md").write_text("foo1")
            (wiki / "foo2.md").write_text("foo2")

            relevant = rag_eval.expand_expected_sources(
                ["wiki/foo*.md"], project_dir
            )
            self.assertEqual(
                relevant,
                {"wiki/foo1.md", "wiki/foo2.md"},
            )


class P02SourceCoverageTest(unittest.TestCase):
    """P0-2: source_coverage 不再因文件存在就送 0.5 分。"""

    def test_partial_coverage(self):
        coverage = rag_eval.check_source_coverage(
            ["a.md"], ["a.md", "b.md"]
        )
        self.assertEqual(coverage, 0.5)

    def test_zero_coverage(self):
        coverage = rag_eval.check_source_coverage(["b.md"], ["a.md"])
        self.assertEqual(coverage, 0.0)

    def test_empty_expected(self):
        coverage = rag_eval.check_source_coverage(["a.md"], [])
        self.assertEqual(coverage, 1.0)

    def test_no_false_points_for_existing_files(self):
        """即使 a.md 在项目中真实存在，只要没返回就不应给分。"""
        with tempfile.TemporaryDirectory() as project_dir:
            (Path(project_dir) / "a.md").write_text("content")
            coverage = rag_eval.check_source_coverage(
                ["other.md"], ["a.md"]
            )
            self.assertEqual(coverage, 0.0)


class P03KeywordMatchTest(unittest.TestCase):
    """P0-3: keyword_match 基于 snippet/title，而非文件路径。"""

    def test_match_on_snippet(self):
        results = [
            {"title": "维生素D", "snippet": "婴儿需要补充维生素D"},
        ]
        score = rag_eval.compute_keyword_match(results, ["维生素D", "补充"])
        self.assertEqual(score, 1.0)

    def test_no_match_when_only_path_contains_keyword(self):
        """文件名命中但 snippet/title 不含关键词，不应算匹配。"""
        results = [
            {"title": "其他", "snippet": " unrelated content"},
        ]
        score = rag_eval.compute_keyword_match(results, ["维生素D"])
        self.assertEqual(score, 0.0)


class P04OrphanedPagesTest(unittest.TestCase):
    """P0-4: orphaned_pages 比较文件 identity，而非 frontmatter title。"""

    def test_chain_orphan(self):
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "a.md").write_text("[[b]]")
            (wiki / "b.md").write_text("[[c]]")
            (wiki / "c.md").write_text("tail")

            stats = ingest_check.check_wikilink_density(wiki_dir)
            # a 没有被任何页面链接
            self.assertEqual(stats["orphaned_pages"], 1)

    def test_basename_link(self):
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "entities").mkdir()
            (wiki / "entities" / "foo.md").write_text("content")
            (wiki / "index.md").write_text("[[foo]] [[index]]")

            stats = ingest_check.check_wikilink_density(wiki_dir)
            # entities/foo 被 [[foo]] 覆盖，index 被自己覆盖
            self.assertEqual(stats["orphaned_pages"], 0)


class P05FrontmatterTest(unittest.TestCase):
    """P0-5: parse_frontmatter 应正确解析 YAML 数组、引号、多行值。"""

    SAMPLE = """---
title: "带引号的标题"
tags: [a, b, c]
related:
  - x
  - y
---
body
"""

    def _assert_parsed(self, fm):
        self.assertEqual(fm.get("title"), "带引号的标题")
        self.assertEqual(fm.get("tags"), ["a", "b", "c"])
        self.assertEqual(fm.get("related"), ["x", "y"])

    def test_ingest_check_parse_frontmatter(self):
        fm, body = ingest_check.parse_frontmatter(self.SAMPLE)
        self._assert_parsed(fm)
        self.assertEqual(body.strip(), "body")

    def test_generate_test_cases_parse_frontmatter(self):
        fm, body = generate_test_cases.parse_frontmatter(self.SAMPLE)
        self._assert_parsed(fm)
        self.assertEqual(body.strip(), "body")


class P06SourceCoverageSlugTest(unittest.TestCase):
    """P0-6: 原始材料 vs wiki 页面用归一化 slug 精确匹配，避免子串误匹配。"""

    def test_substring_no_longer_matches(self):
        with tempfile.TemporaryDirectory() as project_dir:
            raw_root = Path(project_dir) / "raw"
            raw_sources = raw_root / "sources"
            wiki_sources = Path(project_dir) / "wiki" / "sources"
            raw_sources.mkdir(parents=True)
            wiki_sources.mkdir(parents=True)
            (raw_sources / "01.md").write_text("one")
            (raw_sources / "101.md").write_text("one-o-one")
            (wiki_sources / "101.md").write_text("wiki one-o-one")

            stats = ingest_check.compare_source_to_wiki(str(raw_root), str(wiki_sources))
            # 只有 101.md 能精确匹配，01.md 不应因是子串而误匹配
            self.assertEqual(stats["matched_sources"], 1)
            self.assertEqual(stats["missing_wiki_pages"], ["01.md"])

    def test_normalized_slug_match(self):
        with tempfile.TemporaryDirectory() as project_dir:
            raw_root = Path(project_dir) / "raw"
            raw_sources = raw_root / "sources"
            wiki_sources = Path(project_dir) / "wiki" / "sources"
            raw_sources.mkdir(parents=True)
            wiki_sources.mkdir(parents=True)
            (raw_sources / "崔玉涛-01.md").write_text("source")
            (wiki_sources / "崔玉涛01.md").write_text("wiki")

            stats = ingest_check.compare_source_to_wiki(str(raw_root), str(wiki_sources))
            self.assertEqual(stats["matched_sources"], 1)


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
             "derived_hit@5": None, "derived_hit@10": None},
            {"case_id": "c4", "schema_version": "v2",
             "source_hit@5": False, "source_hit@10": False,
             "derived_hit@5": False, "derived_hit@10": False},
        ]
        s = rag_eval.summarize_v2_retrieval(retrieval_results)
        self.assertEqual(s["source_hit_rate@5"], 0.5)
        self.assertEqual(s["source_hit_rate@10"], 0.75)
        self.assertEqual(s["derived_hit_rate@5"], round(1/3, 3))
        self.assertEqual(s["derived_hit_rate@10"], round(2/3, 3))
        self.assertEqual(s["failures"]["source_miss@10"], ["c4"])
        self.assertEqual(s["failures"]["derived_miss@10"], ["c4"])

    def test_summarize_empty(self):
        s = rag_eval.summarize_v2_retrieval([])
        self.assertEqual(s["source_hit_rate@5"], 0.0)
        self.assertEqual(s["failures"]["source_miss@10"], [])


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

    def test_scan_handles_fallback_parser_string_sources(self):
        """当 PyYAML 不可用时，parse_frontmatter 返回的 sources 是 JSON 数组字符串。
        scan_derived_pages 应尝试 JSON 解析，得到正确的 source 文件名作为 key。"""
        import tempfile, os
        with tempfile.TemporaryDirectory() as td:
            wiki = os.path.join(td, "wiki")
            os.makedirs(os.path.join(wiki, "concepts"))
            # 直接构造 fallback parser 会产生的 frontmatter 格式
            with open(os.path.join(wiki, "concepts", "vd.md"), "w", encoding="utf-8") as f:
                f.write('---\nsources: ["source-01.md", "source-02.md"]\n---\n# VD\n')
            # 强制使用 fallback parser
            orig_yaml = generate_test_cases.yaml
            generate_test_cases.yaml = None
            try:
                mapping = generate_test_cases.scan_derived_pages(wiki)
            finally:
                generate_test_cases.yaml = orig_yaml
            # 应该解析出 source-01.md 和 source-02.md 作为 key
            self.assertIn("source-01.md", mapping)
            self.assertIn("source-02.md", mapping)
            self.assertIn(os.path.join("wiki", "concepts", "vd.md"), mapping["source-01.md"])


if __name__ == "__main__":
    unittest.main()
