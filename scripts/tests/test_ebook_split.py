import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import ebook_split  # noqa: E402


class TestCleanTitle(unittest.TestCase):
    def test_strips_chapter_prefix_and_quotes(self):
        self.assertEqual(
            ebook_split.clean_title('第1章　解决孩子"天生"的睡眠问题'),
            "解决孩子天生的睡眠问题",
        )

    def test_truncates_long_title(self):
        t = ebook_split.clean_title("第1章　" + "长" * 40)
        self.assertLessEqual(len(t), 30)


class TestFindChapterHeads(unittest.TestCase):
    def test_splits_chapters_and_front(self):
        lines = [
            "版权信息", "", "目录", "",
            "第1章　睡眠入门", "正文甲", "",
            "第2章　夜间哺乳", "正文乙",
        ]
        front, chapters = ebook_split.find_chapter_heads(lines)
        self.assertIn("版权信息", front)
        self.assertEqual(len(chapters), 2)
        self.assertEqual(chapters[0][0], "第1章　睡眠入门")
        self.assertEqual(chapters[1][1], ["正文乙"])

    def test_toc_lines_stay_in_front_with_default_re(self):
        # TOC 用半角空格,正文标题用全角空格 → 默认正则只认正文标题
        lines = ["第2章 关于睡眠 非快速眼动睡眠", "第1章　真正正文"]
        front, chapters = ebook_split.find_chapter_heads(lines)
        self.assertEqual(len(chapters), 1)
        self.assertEqual(chapters[0][0], "第1章　真正正文")
        self.assertIn("第2章 关于睡眠 非快速眼动睡眠", front)


class TestSubsplit(unittest.TestCase):
    def test_cuts_at_paragraph_boundary(self):
        paras = ["一" * 1500, "二" * 1500, "三" * 1500]
        text = "\n\n".join(paras)
        chunks = ebook_split.subsplit(text, max_chars=2000)
        self.assertEqual(chunks, paras)  # 三个整段各自成块,不腰斩
        for c in chunks:
            self.assertLessEqual(len(c), 2000)

    def test_sentence_fallback_never_mid_sentence(self):
        p = "。".join("句" * 800 for _ in range(6)) + "。"
        chunks = ebook_split.subsplit(p, max_chars=1000)
        self.assertGreater(len(chunks), 1)
        for c in chunks:
            self.assertTrue(c.endswith("。"))

    def test_all_paragraphs_preserved(self):
        text = "\n\n".join(f"P{i}" * 300 for i in range(5))
        joined = "".join(ebook_split.subsplit(text, max_chars=500))
        for i in range(5):
            self.assertIn(f"P{i}" * 300, joined)


if __name__ == "__main__":
    unittest.main()
