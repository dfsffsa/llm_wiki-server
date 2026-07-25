"""Tests for the Finding dataclass and findings integration in ingest_check.py

These tests verify:
1. Finding dataclass instantiation and to_dict() serialization
2. check_schema_compliance emits findings for missing frontmatter fields
3. check_wikilink_density emits findings for broken wikilinks
4. compare_source_to_wiki emits findings for missing source pages
5. run_ingest_check collects findings from all sections
6. fixers registry works
"""

import importlib.util
import json
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


ingest_check = _load_module("ingest_check", EVAL_DIR / "ingest_check.py")


class FindingModelTest(unittest.TestCase):
    """Finding dataclass creation and serialization."""

    def test_finding_basic_creation(self):
        finding = ingest_check.Finding(
            page="wiki/sources/foo.md",
            severity="error",
            category="missing_frontmatter",
            message="Missing fields: ['type', 'title']",
        )
        self.assertEqual(finding.page, "wiki/sources/foo.md")
        self.assertEqual(finding.severity, "error")
        self.assertEqual(finding.category, "missing_frontmatter")
        self.assertEqual(finding.message, "Missing fields: ['type', 'title']")
        self.assertEqual(finding.detail, {})
        self.assertFalse(finding.auto_fixable)
        self.assertEqual(finding.fix_strategy, "")

    def test_finding_with_details(self):
        finding = ingest_check.Finding(
            page="wiki/sources/foo.md",
            severity="warning",
            category="broken_wikilink",
            message="Broken wikilink: [[bar]] does not exist",
            detail={"target": "bar", "link_text": "bar"},
            auto_fixable=False,
            fix_strategy="rule_wikilink",
        )
        self.assertEqual(finding.detail, {"target": "bar", "link_text": "bar"})
        self.assertFalse(finding.auto_fixable)
        self.assertEqual(finding.fix_strategy, "rule_wikilink")

    def test_finding_to_dict(self):
        finding = ingest_check.Finding(
            page="wiki/sources/foo.md",
            severity="error",
            category="missing_frontmatter",
            message="Missing fields: ['type']",
            detail={"missing_keys": ["type"], "existing_fm": {"title": "Foo"}},
            auto_fixable=True,
            fix_strategy="rule_frontmatter",
        )
        d = finding.to_dict()
        self.assertIsInstance(d, dict)
        self.assertEqual(d["page"], "wiki/sources/foo.md")
        self.assertEqual(d["severity"], "error")
        self.assertEqual(d["category"], "missing_frontmatter")
        self.assertEqual(d["message"], "Missing fields: ['type']")
        self.assertEqual(d["detail"], {"missing_keys": ["type"], "existing_fm": {"title": "Foo"}})
        self.assertTrue(d["auto_fixable"])
        self.assertEqual(d["fix_strategy"], "rule_frontmatter")

    def test_finding_to_dict_json_serializable(self):
        """to_dict() output must survive json.dumps without TypeError."""
        finding = ingest_check.Finding(
            page="wiki/sources/foo.md",
            severity="error",
            category="missing_frontmatter",
            message="Missing fields: ['type']",
            detail={"missing_keys": ["type"], "existing_fm": {"title": "Foo"}},
            auto_fixable=True,
            fix_strategy="rule_frontmatter",
        )
        json_str = json.dumps(finding.to_dict(), ensure_ascii=False)
        parsed = json.loads(json_str)
        self.assertEqual(parsed["page"], "wiki/sources/foo.md")
        self.assertEqual(parsed["category"], "missing_frontmatter")
        self.assertTrue(parsed["auto_fixable"])


class CheckSchemaComplianceFindingsTest(unittest.TestCase):
    """check_schema_compliance returns findings for missing frontmatter fields."""

    def test_no_findings_when_all_compliant(self):
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "foo.md").write_text("---\ntype: source\ntitle: Foo\n---\ncontent")
            (wiki / "bar.md").write_text("---\ntype: concept\ntitle: Bar\n---\ncontent")

            stats = ingest_check.check_schema_compliance(wiki_dir)
            self.assertIn("findings", stats)
            self.assertEqual(len(stats["findings"]), 0)

    def test_finding_for_missing_type_and_title(self):
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "no-frontmatter.md").write_text("content without frontmatter")

            stats = ingest_check.check_schema_compliance(wiki_dir)
            self.assertIn("findings", stats)
            self.assertEqual(len(stats["findings"]), 1)
            f = stats["findings"][0]
            self.assertEqual(f["category"], "missing_frontmatter")
            self.assertEqual(f["severity"], "error")
            self.assertTrue(f["auto_fixable"])
            self.assertEqual(f["fix_strategy"], "rule_frontmatter")
            self.assertIn("missing_keys", f["detail"])

    def test_finding_for_partial_missing(self):
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "partial.md").write_text("---\ntype: source\n---\ncontent")

            stats = ingest_check.check_schema_compliance(wiki_dir)
            self.assertEqual(len(stats["findings"]), 1)
            f = stats["findings"][0]
            self.assertEqual(f["category"], "missing_frontmatter")
            self.assertEqual(f["detail"]["missing_keys"], ["title"])

    def test_findings_count_matches_missing_pages(self):
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "good.md").write_text("---\ntype: source\ntitle: Good\n---\ncontent")
            (wiki["bad1.md"] if False else None)  # no-op
            (wiki / "bad1.md").write_text("content without fm")
            (wiki / "bad2.md").write_text("---\ntitle: OnlyTitle\n---\ncontent")

            stats = ingest_check.check_schema_compliance(wiki_dir)
            # good.md has no findings, bad1.md and bad2.md each have 1 finding
            self.assertEqual(len(stats["findings"]), 2)


class CheckWikilinkDensityFindingsTest(unittest.TestCase):
    """check_wikilink_density returns findings for broken wikilinks."""

    def test_no_broken_wikilinks(self):
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "a.md").write_text("[[b]]")
            (wiki / "b.md").write_text("[[c]]")
            (wiki / "c.md").write_text("tail")

            stats = ingest_check.check_wikilink_density(wiki_dir)
            self.assertIn("findings", stats)
            # All links resolve to existing pages
            self.assertEqual(len(stats["findings"]), 0)

    def test_broken_wikilink_detected(self):
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "a.md").write_text("[[b]]")
            (wiki / "b.md").write_text("[[nonexistent]]")

            stats = ingest_check.check_wikilink_density(wiki_dir)
            self.assertIn("findings", stats)
            self.assertGreaterEqual(len(stats["findings"]), 1)
            # Check that we found the broken link to nonexistent
            broken_targets = [f["detail"]["target"] for f in stats["findings"]]
            self.assertIn("nonexistent", broken_targets)

    def test_broken_wikilink_fields(self):
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "source.md").write_text("[[missing_page]]")

            stats = ingest_check.check_wikilink_density(wiki_dir)
            f = stats["findings"][0]
            self.assertEqual(f["category"], "broken_wikilink")
            self.assertEqual(f["severity"], "warning")
            self.assertFalse(f["auto_fixable"])
            self.assertEqual(f["fix_strategy"], "rule_wikilink")
            self.assertEqual(f["detail"]["target"], "missing_page")
            # page is relative to wiki_dir
            self.assertIn("source.md", f["page"])

    def test_index_page_not_reported_as_broken(self):
        """[[index]] is a common self-reference and should not be reported."""
        with tempfile.TemporaryDirectory() as wiki_dir:
            wiki = Path(wiki_dir)
            (wiki / "a.md").write_text("[[index]]")

            stats = ingest_check.check_wikilink_density(wiki_dir)
            self.assertIn("findings", stats)
            broken_targets = [f["detail"]["target"] for f in stats["findings"]]
            self.assertNotIn("index", broken_targets)


class CompareSourceToWikiFindingsTest(unittest.TestCase):
    """compare_source_to_wiki returns findings for missing source pages."""

    def test_no_missing_when_all_match(self):
        with tempfile.TemporaryDirectory() as project_dir:
            raw_sources = Path(project_dir) / "sources"
            wiki_sources = Path(project_dir) / "wiki" / "sources"
            raw_sources.mkdir(parents=True)
            wiki_sources.mkdir(parents=True)
            (raw_sources / "foo.md").write_text("foo")
            (wiki_sources / "foo.md").write_text("foo wiki")

            stats = ingest_check.compare_source_to_wiki(str(Path(project_dir) / "raw"), str(wiki_sources))
            self.assertIn("findings", stats)
            self.assertEqual(len(stats["findings"]), 0)

    def test_finding_for_missing_source(self):
        with tempfile.TemporaryDirectory() as project_dir:
            raw_dir = Path(project_dir) / "raw"
            raw_sources = raw_dir / "sources"
            wiki_sources = Path(project_dir) / "wiki" / "sources"
            raw_sources.mkdir(parents=True)
            wiki_sources.mkdir(parents=True)
            (raw_sources / "missing-in-wiki.md").write_text("source content")
            (raw_sources / "present.md").write_text("present")
            (wiki_sources / "present.md").write_text("wiki content")

            stats = ingest_check.compare_source_to_wiki(str(raw_dir), str(wiki_sources))
            self.assertIn("findings", stats)
            self.assertEqual(len(stats["findings"]), 1)
            f = stats["findings"][0]
            self.assertEqual(f["category"], "missing_source_page")
            self.assertEqual(f["severity"], "warning")
            self.assertFalse(f["auto_fixable"])
            self.assertIn("missing-in-wiki.md", f["page"])

    def test_finding_detail_contains_raw_source(self):
        with tempfile.TemporaryDirectory() as project_dir:
            raw_sources = Path(project_dir) / "raw" / "sources"
            wiki_sources = Path(project_dir) / "wiki" / "sources"
            raw_sources.mkdir(parents=True)
            wiki_sources.mkdir(parents=True)
            (raw_sources / "orphan.md").write_text("orphan")

            stats = ingest_check.compare_source_to_wiki(
                str(Path(project_dir) / "raw"), str(wiki_sources)
            )
            f = stats["findings"][0]
            self.assertEqual(f["detail"]["raw_source"], "orphan.md")


class RunIngestCheckFindingsTest(unittest.TestCase):
    """run_ingest_check collects findings from all sections."""

    def test_findings_in_output(self):
        with tempfile.TemporaryDirectory() as project_dir:
            wiki = Path(project_dir) / "wiki"
            raw = Path(project_dir) / "raw"
            wiki_sources = wiki / "sources"
            raw_sources = raw / "sources"
            wiki.mkdir()
            raw_sources.mkdir(parents=True)
            wiki_sources.mkdir(parents=True)

            # Create a page with missing frontmatter
            (wiki / "no-fm.md").write_text("content without frontmatter")
            # Create a page with a broken wikilink
            (wiki / "with-link.md").write_text("[[nonexistent]]")
            # Create a raw source that's missing from wiki
            (raw_sources / "missing-source.md").write_text("raw material")

            results = ingest_check.run_ingest_check(project_dir)
            self.assertIn("findings", results)
            self.assertIsInstance(results["findings"], list)
            self.assertGreaterEqual(len(results["findings"]), 2)
            self.assertIn("fixable_count", results)

    def test_fixing_count(self):
        with tempfile.TemporaryDirectory() as project_dir:
            wiki = Path(project_dir) / "wiki"
            raw = Path(project_dir) / "raw"
            raw_sources = raw / "sources"
            wiki.mkdir()
            raw_sources.mkdir(parents=True)

            (wiki / "no-fm.md").write_text("content without frontmatter")

            results = ingest_check.run_ingest_check(project_dir)
            # missing_frontmatter is auto_fixable=True
            self.assertGreaterEqual(results["fixable_count"], 1)

    def test_findings_json_serializable(self):
        with tempfile.TemporaryDirectory() as project_dir:
            wiki = Path(project_dir) / "wiki"
            raw = Path(project_dir) / "raw"
            raw_sources = raw / "sources"
            wiki_sources = wiki / "sources"
            wiki.mkdir()
            raw_sources.mkdir(parents=True)
            wiki_sources.mkdir(parents=True)

            (wiki / "no-fm.md").write_text("content without frontmatter")
            (raw_sources / "orphan.md").write_text("orphan")

            results = ingest_check.run_ingest_check(project_dir)
            # findings should be plain dicts (via to_dict()), not Finding instances
            for f in results["findings"]:
                self.assertIsInstance(f, dict)
            # Should survive JSON serialization
            json_str = json.dumps(results, ensure_ascii=False, indent=2)
            parsed = json.loads(json_str)
            self.assertIn("findings", parsed)
            self.assertIn("fixable_count", parsed)


class FixersInitTest(unittest.TestCase):
    """fixers/__init__.py exports the registry."""

    def setUp(self):
        # Import the fixers module
        fixers_dir = EVAL_DIR / "fixers"
        init_path = fixers_dir / "__init__.py"
        if init_path.exists():
            spec = importlib.util.spec_from_file_location("fixers", init_path)
            self.fixers = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(self.fixers)
        else:
            self.fixers = None

    def test_registry_exists(self):
        if self.fixers is None:
            self.skipTest("fixers/__init__.py does not exist yet")
        self.assertTrue(hasattr(self.fixers, "FIXER_REGISTRY"))
        self.assertIsInstance(self.fixers.FIXER_REGISTRY, dict)

    def test_register_decorator(self):
        if self.fixers is None:
            self.skipTest("fixers/__init__.py does not exist yet")
        self.assertTrue(hasattr(self.fixers, "register"))
        self.assertTrue(callable(self.fixers.register))

    def test_get_fixer(self):
        if self.fixers is None:
            self.skipTest("fixers/__init__.py does not exist yet")
        self.assertTrue(hasattr(self.fixers, "get_fixer"))
        self.assertTrue(callable(self.fixers.get_fixer))


if __name__ == "__main__":
    unittest.main()
