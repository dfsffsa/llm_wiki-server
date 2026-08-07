import os
import sys
import tempfile
import unittest

# 让 overlay/eval 可 import
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from generate_test_cases import shuffled_sources  # noqa: E402


class TestShuffledSources(unittest.TestCase):
    def _mkdir_with_books(self, n_per_book=3):
        d = tempfile.mkdtemp()
        for b in ["养育女孩", "法伯睡眠宝典", "定本育儿百科"]:
            for i in range(n_per_book):
                with open(os.path.join(d, f"{b}-{i:02d}.md"), "w", encoding="utf-8") as f:
                    f.write("# x\n")
        return d

    def test_first_n_spans_multiple_books(self):
        d = self._mkdir_with_books()
        files = shuffled_sources(d)
        first = [os.path.basename(f).split("-")[0] for f in files[:4]]
        self.assertGreater(len(set(first)), 1, f"前4个源应跨多书: {first}")

    def test_deterministic(self):
        d = self._mkdir_with_books()
        self.assertEqual(shuffled_sources(d), shuffled_sources(d))


if __name__ == "__main__":
    unittest.main()
