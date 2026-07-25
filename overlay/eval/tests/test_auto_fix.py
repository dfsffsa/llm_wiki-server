import tempfile, os, unittest, sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from fixers.frontmatter import fix_frontmatter
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
                page=os.path.relpath(page, td),
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
                page=os.path.relpath(page, td),
                severity="info",
                category="missing_frontmatter",
                message="",
                detail={"missing_keys": [], "existing_fm": {}},
                auto_fixable=True,
                fix_strategy="rule_frontmatter",
            )
            result = fix_frontmatter(td, finding)
            self.assertFalse(result["fixed"])

if __name__ == "__main__":
    unittest.main()
