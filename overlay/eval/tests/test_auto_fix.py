import tempfile, os, unittest, sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from fixers.frontmatter import fix_frontmatter
from fixers.wikilink import fix_wikilink
from ingest_check import Finding

class TestFrontmatterFixer(unittest.TestCase):
    def test_adds_missing_type(self):
        with tempfile.TemporaryDirectory() as td:
            wiki = os.path.join(td, "wiki", "entities")
            os.makedirs(wiki)
            page = os.path.join(wiki, "test.md")
            with open(page, "w") as f:
                f.write("---\ntitle: Test\n---\n# Body\n")

            finding = Finding(
                page=os.path.relpath(page, os.path.join(td, "wiki")),
                severity="error",
                category="missing_frontmatter",
                message="Missing type",
                detail={"missing_keys": ["type"], "existing_fm": {"title": "Test"}},
                auto_fixable=True,
                fix_strategy="rule_frontmatter",
            )
            result = fix_frontmatter(td, finding)
            self.assertTrue(result["fixed"])

            # 验证
            with open(page) as f:
                content = f.read()
            self.assertIn("---", content)
            self.assertIn("type: note", content)
            self.assertIn("title: Test", content)  # 保留原有字段
            self.assertIn("# Body", content)  # 正文不变
            # 备份存在
            self.assertTrue(os.path.exists(os.path.join(td, "fix_backups")))

    def test_noop_when_complete(self):
        with tempfile.TemporaryDirectory() as td:
            wiki = os.path.join(td, "wiki")
            os.makedirs(wiki)
            page = os.path.join(wiki, "ok.md")
            with open(page, "w") as f:
                f.write("---\ntype: entity\ntitle: OK\ncreated: 2025-01-01\nupdated: 2025-01-01\n---\n# OK\n")
            finding = Finding(
                page=os.path.relpath(page, os.path.join(td, "wiki")),
                severity="info",
                category="missing_frontmatter",
                message="",
                detail={"missing_keys": [], "existing_fm": {}},
                auto_fixable=True,
                fix_strategy="rule_frontmatter",
            )
            result = fix_frontmatter(td, finding)
            self.assertFalse(result["fixed"])

class TestWikilinkFixer(unittest.TestCase):
    def test_repair_path(self):
        with tempfile.TemporaryDirectory() as td:
            wiki = os.path.join(td, "wiki")
            os.makedirs(os.path.join(wiki, "sources"))
            os.makedirs(os.path.join(wiki, "entities"))
            # 目标页面存在
            with open(os.path.join(wiki, "entities", "vitamind.md"), "w") as f:
                f.write("---\ntype: entity\ntitle: VD\n---\n# VD\n")
            # 有 broken link 的页面
            page = os.path.join(wiki, "sources", "test.md")
            with open(page, "w") as f:
                f.write("---\ntype: source\ntitle: T\n---\nSee [[nonexistent]] for details.\n")

            finding = Finding(
                page="sources/test.md",
                severity="warning",
                category="broken_wikilink",
                message="Broken wikilink",
                detail={"target": "nonexistent", "link_text": "nonexistent"},
                auto_fixable=True,
                fix_strategy="rule_wikilink",
            )
            result = fix_wikilink(td, finding)
            # nonexistent 没有模糊匹配，降级为纯文本
            self.assertTrue(result["fixed"])
            with open(page) as f:
                out = f.read()
            self.assertNotIn("[[nonexistent]]", out)
            self.assertIn("nonexistent", out)  # 保留文本

    def test_fuzzy_match(self):
        with tempfile.TemporaryDirectory() as td:
            wiki = os.path.join(td, "wiki")
            os.makedirs(os.path.join(wiki, "concepts"))
            with open(os.path.join(wiki, "concepts", "维生素D补充.md"), "w") as f:
                f.write("---\ntype: concept\n---\n# VD\n")
            page = os.path.join(wiki, "sources", "foo.md")
            os.makedirs(os.path.join(wiki, "sources"))
            with open(page, "w") as f:
                f.write("See [[维生素D]]\n")

            finding = Finding(
                page="sources/foo.md",
                severity="warning",
                category="broken_wikilink",
                message="",
                detail={"target": "维生素D", "link_text": "维生素D"},
                auto_fixable=True,
                fix_strategy="rule_wikilink",
            )
            result = fix_wikilink(td, finding)
            self.assertTrue(result["fixed"])
            with open(page) as f:
                out = f.read()
            self.assertIn("维生素D补充", out)  # 模糊匹配到正确页面


class TestAutoFixPipeline(unittest.TestCase):
    def test_dry_run_does_not_modify(self):
        with tempfile.TemporaryDirectory() as td:
            wiki = os.path.join(td, "wiki")
            os.makedirs(wiki)
            page = os.path.join(wiki, "test.md")
            content_orig = "---\ntitle: T\n---\n# Body\n"
            with open(page, "w") as f:
                f.write(content_orig)

            finding = Finding(
                page="test.md",
                severity="error",
                category="missing_frontmatter",
                message="Missing type",
                detail={"missing_keys": ["type"], "existing_fm": {"title": "T"}},
                auto_fixable=True,
                fix_strategy="rule_frontmatter",
            )
            import ingest_check
            orig_run = ingest_check.run_ingest_check
            def fake_run(*args, **kwargs):
                return {"findings": [finding.to_dict()], "overall_score": 50}
            ingest_check.run_ingest_check = fake_run
            try:
                from auto_fix import run_auto_fix
                report = run_auto_fix(td, dry_run=True)
                self.assertTrue(report.get("dry_run"))
                with open(page) as f:
                    self.assertEqual(f.read(), content_orig)
            finally:
                ingest_check.run_ingest_check = orig_run


if __name__ == "__main__":
    unittest.main()
