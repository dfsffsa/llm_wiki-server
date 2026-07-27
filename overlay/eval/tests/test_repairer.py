"""Tests for repairer module (Phase 3)."""

import json
import os
import sys
import tempfile
import unittest
from unittest.mock import patch, MagicMock

EVAL_DIR = os.path.join(os.path.dirname(__file__), "..")
sys.path.insert(0, EVAL_DIR)


def make_report(source_file="raw/sources/test.md", wiki_page="wiki/sources/test.md",
                coverage=5, hallucinations=None):
    from judge.models import JudgeReportItem, CoverageClaim, Hallucination
    halls = hallucinations or []
    return JudgeReportItem(
        source_file=source_file,
        wiki_page=wiki_page,
        coverage_claims=[
            CoverageClaim(claim="维D从出生补", source_location="L1",
                          wiki_coverage="missing", wiki_excerpt=""),
            CoverageClaim(claim="每天400单位", source_location="L2",
                          wiki_coverage="full", wiki_excerpt="已写"),
        ],
        hallucinations=[
            Hallucination(claim="需要补钙", wiki_location="第3段",
                          severity="major", judge_reasoning="source中无依据"),
        ] if hallucinations else [],
        scores={"coverage": coverage, "consistency": 8},
    )


class TestShouldRepair(unittest.TestCase):
    def test_below_threshold(self):
        from judge.repairer import should_repair
        self.assertTrue(should_repair({"coverage": 4}, [], threshold=6))

    def test_at_threshold(self):
        from judge.repairer import should_repair
        self.assertFalse(should_repair({"coverage": 6}, [], threshold=6))

    def test_above_threshold(self):
        from judge.repairer import should_repair
        self.assertFalse(should_repair({"coverage": 8}, [], threshold=6))

    def test_with_hallucinations(self):
        from judge.repairer import should_repair
        self.assertTrue(should_repair({"coverage": 8}, [{"claim": "x"}], threshold=6))

    def test_default_threshold(self):
        from judge.repairer import should_repair
        self.assertTrue(should_repair({"coverage": 4}, []))  # default=6

    def test_no_scores_defaults_high(self):
        from judge.repairer import should_repair
        # 分数不存在时 default coverage=10, 不触发
        self.assertFalse(should_repair({}, [], threshold=6))


class TestFormatGaps(unittest.TestCase):
    def test_formats_missing_and_partial(self):
        from judge.repairer import format_content_gaps
        from judge.models import CoverageClaim
        claims = [
            CoverageClaim(claim="C1", source_location="", wiki_coverage="missing"),
            CoverageClaim(claim="C2", source_location="", wiki_coverage="partial"),
            CoverageClaim(claim="C3", source_location="", wiki_coverage="full"),
        ]
        result = format_content_gaps(claims)
        self.assertIn("C1", result)
        self.assertIn("C2", result)
        self.assertNotIn("C3", result)

    def test_empty_list(self):
        from judge.repairer import format_content_gaps
        self.assertEqual(format_content_gaps([]), "(无)")


class TestFormatHallucinations(unittest.TestCase):
    def test_formats_halls(self):
        from judge.repairer import format_hallucinations
        from judge.models import Hallucination
        halls = [
            Hallucination(claim="需要补钙", wiki_location="第3段",
                          severity="major", judge_reasoning="无故"),
        ]
        result = format_hallucinations(halls)
        self.assertIn("需要补钙", result)
        self.assertIn("major", result)

    def test_empty_list(self):
        from judge.repairer import format_hallucinations
        self.assertEqual(format_hallucinations([]), "(无)")


class TestStripCodeFence(unittest.TestCase):
    def test_strips_markdown_fence(self):
        from judge.repairer import _strip_code_fence
        text = "```markdown\n# 修复后的内容\n```"
        self.assertEqual(_strip_code_fence(text), "# 修复后的内容")

    def test_strips_md_fence(self):
        from judge.repairer import _strip_code_fence
        text = "```md\n内容\n```"
        self.assertEqual(_strip_code_fence(text), "内容")

    def test_strips_plain_fence(self):
        from judge.repairer import _strip_code_fence
        text = "```\n内容\n```"
        self.assertEqual(_strip_code_fence(text), "内容")

    def test_no_fence(self):
        from judge.repairer import _strip_code_fence
        text = "普通内容"
        self.assertEqual(_strip_code_fence(text), "普通内容")


class TestRepairPage(unittest.TestCase):
    @patch("judge.repairer.call_llm")
    def test_repair_calls_llm_with_prompt(self, mock_call):
        mock_call.return_value = "# 修复后的 wiki 页面"
        report = make_report(hallucinations=[{"mock": "hall"}])

        from judge.repairer import repair_page
        result = repair_page("source", "当前wiki", report, {})
        self.assertEqual(result, "# 修复后的 wiki 页面")

    @patch("judge.repairer.call_llm")
    def test_repair_prompt_contains_source_and_report(self, mock_call):
        """验证 LLM prompt 包含 source 和评估反馈的关键信息"""
        mock_call.return_value = "# 修复后"
        report = make_report(hallucinations=[{"mock": "hall"}])  # coverage=5

        from judge.repairer import repair_page
        repair_page("source_text", "wiki_text", report, {})

        # call_llm 的第二个参数是 prompt
        prompt_arg = mock_call.call_args[0][0]
        self.assertIn("source_text", prompt_arg)
        self.assertIn("wiki_text", prompt_arg)
        self.assertIn("维D从出生补", prompt_arg)  # missing claim

    @patch("judge.repairer.call_llm")
    def test_repair_strips_fence(self, mock_call):
        mock_call.return_value = "```markdown\n# 修复后\n```"
        report = make_report(hallucinations=[{"mock": "hall"}])

        from judge.repairer import repair_page
        result = repair_page("src", "wiki", report, {})
        self.assertEqual(result, "# 修复后")

    @patch("judge.repairer.call_llm")
    def test_repair_returns_none_on_error(self, mock_call):
        mock_call.side_effect = RuntimeError("API down")
        report = make_report(hallucinations=[{"mock": "hall"}])

        from judge.repairer import repair_page
        result = repair_page("src", "wiki", report, {})
        self.assertIsNone(result)


class TestWriteRepairedPage(unittest.TestCase):
    def setUp(self):
        self.td = tempfile.TemporaryDirectory()
        self.wiki_dir = os.path.join(self.td.name, "wiki", "sources")
        os.makedirs(self.wiki_dir)
        self.wiki_path = os.path.join(self.wiki_dir, "test.md")
        with open(self.wiki_path, "w", encoding="utf-8") as f:
            f.write("# 原始内容")

    def tearDown(self):
        self.td.cleanup()

    def test_writes_new_content(self):
        from judge.repairer import write_repaired_page
        report = make_report()
        result = write_repaired_page(self.td.name, report, "# 修复后内容")
        self.assertTrue(result["success"])
        with open(self.wiki_path, encoding="utf-8") as f:
            self.assertEqual(f.read(), "# 修复后内容")

    def test_creates_backup(self):
        from judge.repairer import write_repaired_page
        report = make_report()
        write_repaired_page(self.td.name, report, "# 修复后内容")
        backup_dir = os.path.join(self.td.name, "fix_backups")
        self.assertTrue(os.path.isdir(backup_dir))
        # backup key: wiki_sources_test.md.bak
        backup_path = os.path.join(backup_dir, "wiki_sources_test.md.bak")
        self.assertTrue(os.path.isfile(backup_path))
        with open(backup_path, encoding="utf-8") as f:
            self.assertEqual(f.read(), "# 原始内容")

    def test_backup_not_overwritten(self):
        """多次修复不应覆盖备份"""
        from judge.repairer import write_repaired_page
        report = make_report()
        write_repaired_page(self.td.name, report, "# v1")
        # 修改原始文件
        with open(self.wiki_path, "w") as f:
            f.write("# v2")
        write_repaired_page(self.td.name, report, "# v2")
        backup_dir = os.path.join(self.td.name, "fix_backups")
        backup_path = os.path.join(backup_dir, "wiki_sources_test.md.bak")
        with open(backup_path, encoding="utf-8") as f:
            self.assertEqual(f.read(), "# 原始内容")  # 第一次备份

    def test_rejects_path_traversal(self):
        from judge.repairer import write_repaired_page
        from judge.models import JudgeReportItem
        evil_report = JudgeReportItem(
            source_file="x.md",
            wiki_page="../../etc/passwd",
            scores={},
        )
        result = write_repaired_page(self.td.name, evil_report, "x")
        self.assertFalse(result["success"])
        self.assertIn("path traversal", result["error"])

    def test_missing_file(self):
        from judge.repairer import write_repaired_page
        from judge.models import JudgeReportItem
        report = JudgeReportItem(
            source_file="x.md",
            wiki_page="wiki/sources/nonexistent.md",
            scores={},
        )
        result = write_repaired_page(self.td.name, report, "x")
        self.assertFalse(result["success"])


class TestVerifyRepair(unittest.TestCase):
    @patch("judge.repairer.extract_claims")
    @patch("judge.repairer.parse_extracted_claims")
    @patch("judge.repairer.evaluate_wiki")
    def test_verify_passed(self, mock_eval, mock_parse, mock_extract):
        # 模拟完整 judge 管线返回修复后评分上升
        mock_extract.return_value = "claims json"
        mock_parse.return_value = [{"claim": "C1", "location": "L1"}]
        from judge.models import JudgeReportItem, CoverageClaim, Hallucination
        after_report = JudgeReportItem(
            source_file="", wiki_page="",
            coverage_claims=[
                CoverageClaim("C1", "L1", "full", "ok"),
            ],
            hallucinations=[],
            scores={"coverage": 8, "consistency": 9},
        )
        mock_eval.return_value = after_report

        report = make_report(hallucinations=[])  # coverage=5

        from judge.repairer import verify_repair
        import tempfile
        with tempfile.TemporaryDirectory() as td:
            src_dir = os.path.join(td, "raw", "sources")
            os.makedirs(src_dir)
            src_path = os.path.join(src_dir, "t.md")
            with open(src_path, "w") as f:
                f.write("# source")

            result = verify_repair(td, src_path, report, "# 修复后内容", {})

        self.assertTrue(result["success"])
        self.assertEqual(result["verdict"], "passed")
        self.assertEqual(result["before"]["coverage"], 5)
        self.assertEqual(result["after"]["coverage"], 8)

    @patch("judge.repairer.extract_claims")
    @patch("judge.repairer.parse_extracted_claims")
    @patch("judge.repairer.evaluate_wiki")
    def test_verify_rollback_on_degradation(self, mock_eval, mock_parse, mock_extract):
        """修复后评分下降 → 回退"""
        mock_extract.return_value = "claims"
        mock_parse.return_value = [{"claim": "C1", "location": "L1"}]
        from judge.models import JudgeReportItem, CoverageClaim, Hallucination
        after_report = JudgeReportItem(
            source_file="", wiki_page="",
            coverage_claims=[
                CoverageClaim("C1", "L1", "missing", ""),
            ],
            hallucinations=[Hallucination("新幻觉", "第1段", "major", "假")],
            scores={"coverage": 3, "consistency": 5},
        )
        mock_eval.return_value = after_report

        report = make_report(hallucinations=[{"mock": "hall"}])  # coverage=5

        from judge.repairer import verify_repair
        import tempfile
        with tempfile.TemporaryDirectory() as td:
            wiki_dir = os.path.join(td, "wiki", "sources")
            os.makedirs(os.path.join(td, "raw", "sources"))
            os.makedirs(wiki_dir)
            wiki_path = os.path.join(wiki_dir, "test.md")
            with open(wiki_path, "w") as f:
                f.write("# 原始")
            src_path = os.path.join(td, "raw", "sources", "test.md")
            with open(src_path, "w") as f:
                f.write("# source")

            # 先备份
            from judge.repairer import write_repaired_page
            write_repaired_page(td, report, "# 新内容")

            result = verify_repair(td, src_path, report, "# 新内容", {})

            self.assertFalse(result["success"])
            self.assertEqual(result["verdict"], "failed")
            self.assertTrue(result.get("rollback"))
            # verify rollback restored original content
            with open(wiki_path, encoding="utf-8") as f:
                original = f.read()
            self.assertIn("原始", original)

    @patch("judge.repairer.extract_claims")
    def test_verify_fails_on_unreadable_source(self, mock_extract):
        mock_extract.side_effect = Exception("no source")
        report = make_report(hallucinations=[])
        from judge.repairer import verify_repair
        result = verify_repair("/tmp", "/nonexistent", report, "x", {})
        self.assertFalse(result["success"])


if __name__ == "__main__":
    unittest.main()
