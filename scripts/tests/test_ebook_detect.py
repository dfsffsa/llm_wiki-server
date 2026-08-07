import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import ebook_detect  # noqa: E402


class TestAnalyze(unittest.TestCase):
    def test_detects_fullwidth_chapter(self):
        lines = ["版权", "第1章　喂养", "正文甲", "第2章　睡眠", "正文乙"]
        results = ebook_detect.analyze(lines)
        cn = [r for r in results if "第N章" in r["desc"]]
        self.assertTrue(cn, "应检出 第N章 候选")
        self.assertEqual(cn[0]["samples"][:2], ["第1章　喂养", "第2章　睡眠"])

    def test_detects_chapter_n(self):
        lines = ["TOC", "CHAPTER 1　sleep", "CHAPTER 2　feed", "正文"]
        results = ebook_detect.analyze(lines)
        cn = [r for r in results if "CHAPTER" in r["desc"]]
        self.assertTrue(cn)
        self.assertIn("CHAPTER 1　sleep", cn[0]["samples"])

    def test_detects_numbered(self):
        lines = ["59.从1周到半个月", "60.新生儿", "正文"]
        results = ebook_detect.analyze(lines)
        num = [r for r in results if "数字+点" in r["desc"]]
        self.assertTrue(num)

    def test_no_match_returns_empty(self):
        lines = ["纯散文", "没有标题", "只有内容"]
        self.assertEqual(ebook_detect.analyze(lines), [])

    def test_sorted_by_count(self):
        lines = (["第1章　a", "第2章　b", "CHAPTER 1　x"]
                 + ["第" + str(i) + "章　c" for i in range(3, 20)])
        results = ebook_detect.analyze(lines)
        self.assertTrue(results[0]["count"] >= results[-1]["count"])
        self.assertIn("第N章", results[0]["desc"])


if __name__ == "__main__":
    unittest.main()
