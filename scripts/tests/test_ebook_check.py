import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import ebook_check  # noqa: E402


def make_chunk(tmpdir, name, body):
    path = os.path.join(tmpdir, name)
    with open(path, "w", encoding="utf-8") as f:
        f.write(f"---\ntype: source_lesson\n---\n\n# t\n\n> 来源:x\n\n{body}")
    return path


class TestCheckChunk(unittest.TestCase):
    def test_caches_by_content_hash(self):
        d = tempfile.mkdtemp()
        p = make_chunk(d, "a.md", "内容" * 100)
        config = {"model": "m", "apiKey": "k", "endpoint": "e"}
        cache = {}
        calls = []

        def fake(prompt, cfg):
            calls.append(prompt)
            return '{"ok": true, "severity": "ok", "issue": ""}'

        _, c1 = ebook_check.check_chunk(p, config, cache, check_fn=fake)
        _, c2 = ebook_check.check_chunk(p, config, cache, check_fn=fake)
        self.assertFalse(c1)
        self.assertTrue(c2)          # 第二次命中缓存
        self.assertEqual(len(calls), 1)

    def test_parses_fenced_verdict(self):
        d = tempfile.mkdtemp()
        p = make_chunk(d, "b.md", "内容" * 100)
        config = {}
        cache = {}

        def fake(prompt, cfg):
            return '```json\n{"ok": false, "severity": "truncated", "issue": "结尾缺句号"}\n```'

        verdict, _ = ebook_check.check_chunk(p, config, cache, check_fn=fake)
        self.assertEqual(verdict["severity"], "truncated")


class TestCollectChunks(unittest.TestCase):
    def test_filters_only_long_by_char_count(self):
        d = tempfile.mkdtemp()
        make_chunk(d, "short.md", "短" * 50)
        make_chunk(d, "long.md", "长" * 3000)
        paths = ebook_check.collect_chunks(d, only_long=True)
        self.assertEqual([os.path.basename(p) for p in paths], ["long.md"])


class TestWriteReport(unittest.TestCase):
    def test_lists_manual_review(self):
        d = tempfile.mkdtemp()
        report = os.path.join(d, "report.md")
        cache = {
            "a.md": {"hash": "h", "verdict": {"ok": False, "severity": "dangling",
                                              "issue": "悬空指代"}},
        }
        ebook_check.write_report(report, [], cache)
        with open(report, encoding="utf-8") as f:
            content = f.read()
        self.assertIn("MANUAL_REVIEW", content)
        self.assertIn("a.md", content)

    def test_empty_results_still_writes_valid_report(self):
        d = tempfile.mkdtemp()
        report = os.path.join(d, "report.md")
        ebook_check.write_report(report, [], {})
        with open(report, encoding="utf-8") as f:
            content = f.read()
        self.assertIn("MANUAL_REVIEW", content)
        self.assertIn("(无)", content)

    def test_save_load_cache_roundtrip_preserves_chinese(self):
        d = tempfile.mkdtemp()
        cache_path = os.path.join(d, "cache.json")
        cache = {"a.md": {"hash": "h1", "verdict": {"ok": False, "severity": "dangling", "issue": "悬空指代"}}}
        ebook_check.save_cache(cache, cache_path)
        loaded = ebook_check.load_cache(cache_path)
        self.assertEqual(loaded, cache)

    def test_sample_applied_after_only_long_filter(self):
        d = tempfile.mkdtemp()
        # 3 long + 2 short files
        make_chunk(d, "01.md", "长" * 3000)
        make_chunk(d, "02.md", "长" * 3000)
        make_chunk(d, "03.md", "长" * 3000)
        make_chunk(d, "04.md", "短" * 50)
        make_chunk(d, "05.md", "短" * 50)
        paths = ebook_check.collect_chunks(d, only_long=True, sample=2)
        self.assertEqual(len(paths), 2)
        names = [os.path.basename(p) for p in paths]
        # 只保留前 2 个 long chunk (排序后),不含 short
        self.assertIn("01.md", names)
        self.assertIn("02.md", names)
        self.assertNotIn("04.md", names)
        self.assertNotIn("05.md", names)


if __name__ == "__main__":
    unittest.main()
