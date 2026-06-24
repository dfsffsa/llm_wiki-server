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


if __name__ == "__main__":
    unittest.main()
