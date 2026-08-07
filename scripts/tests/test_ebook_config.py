import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import ebook_config  # noqa: E402


def write_config(tmpdir, cfg):
    p = os.path.join(tmpdir, "batch.json")
    with open(p, "w", encoding="utf-8") as f:
        json.dump(cfg, f, ensure_ascii=False)
    return p


VALID = {
    "name": "test",
    "sourceDir": "/mnt/c/电子书",
    "books": [{"dir": "d1", "epub": "a.epub", "book": "书一", "source": "书一"}],
}


class TestValidate(unittest.TestCase):
    def test_valid(self):
        d = tempfile.mkdtemp()
        p = write_config(d, VALID)
        cfg = ebook_config.validate(ebook_config.load(p))
        self.assertEqual(cfg["name"], "test")

    def test_missing_source_dir(self):
        d = tempfile.mkdtemp()
        bad = {"name": "x", "books": [{"dir": "d", "epub": "a", "book": "b", "source": "s"}]}
        p = write_config(d, bad)
        with self.assertRaises(ValueError) as ctx:
            ebook_config.validate(ebook_config.load(p))
        self.assertIn("sourceDir", str(ctx.exception))

    def test_book_missing_field(self):
        d = tempfile.mkdtemp()
        bad = {"name": "x", "sourceDir": "/d", "books": [{"dir": "d", "book": "b"}]}
        p = write_config(d, bad)
        with self.assertRaises(ValueError) as ctx:
            ebook_config.validate(ebook_config.load(p))
        self.assertIn("books[0]", str(ctx.exception))

    def test_empty_books(self):
        d = tempfile.mkdtemp()
        p = write_config(d, {"name": "x", "sourceDir": "/d", "books": []})
        with self.assertRaises(ValueError):
            ebook_config.validate(ebook_config.load(p))

    def test_non_dict_book_entry(self):
        d = tempfile.mkdtemp()
        p = write_config(d, {"name": "t", "sourceDir": "/d", "books": ["not-a-dict"]})
        with self.assertRaises(ValueError) as ctx:
            ebook_config.validate(ebook_config.load(p))
        self.assertIn("books[0] must be an object", str(ctx.exception))

    def test_bad_json(self):
        d = tempfile.mkdtemp()
        p = os.path.join(d, "bad.json")
        with open(p, "w") as f:
            f.write("{not json")
        with self.assertRaises(json.JSONDecodeError):
            ebook_config.load(p)


class TestBooksTsv(unittest.TestCase):
    def test_preserves_pipe_and_spaces(self):
        cfg = {
            "name": "t", "sourceDir": "/d",
            "books": [{
                "dir": "291-西尔斯育儿经",
                "epub": "XiErSiYuErJing.epub",
                "book": "西尔斯育儿经",
                "source": "西尔斯育儿经",
                "headingRe": "^(CHAPTER [0-9]+|Part [IVX]+)　",
            }],
        }
        rows = ebook_config.books_tsv(cfg).split("\n")
        fields = rows[0].split("\t")
        self.assertEqual(len(fields), 5)
        self.assertEqual(fields[4], "^(CHAPTER [0-9]+|Part [IVX]+)　")

    def test_missing_heading_re_defaults_empty(self):
        cfg = {"name": "t", "sourceDir": "/d",
               "books": [{"dir": "d", "epub": "a", "book": "b", "source": "s"}]}
        fields = ebook_config.books_tsv(cfg).split("\t")
        self.assertEqual(fields[4], "")


class TestPropsTsv(unittest.TestCase):
    def test_defaults(self):
        cfg = {"name": "t", "sourceDir": "/d",
               "books": [{"dir": "d", "epub": "a", "book": "b", "source": "s"}]}
        fields = ebook_config.props_tsv(cfg).split("\t")
        self.assertEqual(fields, ["/d", ".tools/ebooks", "2500", ""])

    def test_override(self):
        cfg = {"name": "t", "sourceDir": "/d", "outBase": "/o", "maxChars": 3000,
               "project": "P", "books": [{"dir": "d", "epub": "a", "book": "b", "source": "s"}]}
        fields = ebook_config.props_tsv(cfg).split("\t")
        self.assertEqual(fields, ["/d", "/o", "3000", "P"])


if __name__ == "__main__":
    unittest.main()
