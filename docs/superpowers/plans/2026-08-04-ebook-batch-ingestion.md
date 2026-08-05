# 电子书批量入库实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 8 本育儿电子书(EPUB)经 ebook-convert → 章节切分 → LLM 语义检查 → 落入 `raw/sources/` → ingest → eval/fix 全流程,扩大 ParentingBooks 知识库覆盖。

**Architecture:** 两个可复用 Python 工具(`ebook_split.py` 转换+切分、`ebook_check.py` LLM 语义检查)+ 一个 bash 编排脚本(`ebook_run.sh` 含书单,subcommand split/check/promote)。切分按章节 + 超长章(>2500 字)在段落边界再切;LLM 检查复用 `overlay/eval/judge/llm_client.py::call_llm`,断点续跑。落地后走既有 `ingest-batch.sh` 与 `run_eval.sh`。

**Tech Stack:** Python 3.12(stdlib + requests + PyYAML,均已有)、calibre 7.6.0(`ebook-convert`)、bash;LLM 用 Ark `deepseek-v4-flash`(`overlay/config/llm.judge.a.json`,env `TENCENT_TOKEN`)。

**设计 spec:** [2026-08-04-ebook-batch-ingestion-design.md](../specs/2026-08-04-ebook-batch-ingestion-design.md)

---

## 执行状态(2026-08-05)

- [x] **Task 1–7 已完成**(工具构建 + 8 本全量切分,1256 块,27 单测全过)
- [x] **Task 8 已完成**(全量 LLM 检查 + `--fix` 修 114 块;复核清单 `.tools/ebooks/MANUAL_REVIEW.md`)
- [x] **Task 9 已完成**(promote 到 raw/sources,1437 文件;purpose.md 已更新)
- [ ] **Task 10 进行中**(并行入库 4 workers 后台跑 1265 文件,预计 ~10h;LLM 已切 deepseek-v4-flash-202605)
- [ ] **Task 11 待执行**(server :8080 → 新书 v2 测试用例 → run_eval all --fix → rag_eval)

> 交接详情:`docs/notes/2026-08-04-ebook-ingestion-progress.md`(2026-08-05 有续跑更新)。
> ⚠️ 实际工作产物在主 checkout `/home/ab/overseas-github/llm_wiki-server`(branch `main`),不在 worktree `feat+ebook-ingestion`(空,可删)。

---

## 关键事实(执行前必读)

- **calibre txt 结构**:正文章节标题是独立行 `第1章　<标题>`(**全角空格**);目录(TOC)条目是 `第1章 <标题> <小节> <页码>`(**半角空格**)、紧挨在书前部。正文起点前的版权/目录/序言为 front matter。
  - 因此默认章节正则 `^第[0-9]+章　`(**要求全角空格**)能区分正文标题与 TOC 行。个别书可能不同 → 用 `--heading-re` 覆盖(见 Task 7)。
- **8 本书**(跳过「52-崔玉涛:宝贝健康公开课」),epub 均在 `/mnt/c/Users/Lenovo/Downloads/电子书/<目录>/`:
  | 简化书名(文件名前缀) | epub 相对路径 | 原书名(frontmatter source) |
  |---|---|---|
  | 法伯睡眠宝典 | `1454-法伯睡眠宝典/法伯睡眠宝典.epub` | 法伯睡眠宝典 |
  | 崔玉涛自然养育法 | `11063-崔玉涛自然养育法/CuiYuTaoZiRanYangYuFa(J.epub` | 崔玉涛自然养育法 |
  | 好孕从卵子开始 | `11153-好孕，从卵子开始/HaoYunCongLuanZiKaiShi.epub` | 好孕，从卵子开始 |
  | 成就好爸爸 | `2548-成就好爸爸：男人一生最重要的工作/成就好爸爸：男人一生最重要的工作.epub` | 成就好爸爸：男人一生最重要的工作 |
  | 定本育儿百科 | `290-定本育儿百科/DingBenYuErBaiKe.epub` | 定本育儿百科 |
  | 西尔斯育儿经 | `291-西尔斯育儿经/XiErSiYuErJing.epub` | 西尔斯育儿经 |
  | 第一次当奶爸 | `738-第一次当奶爸/第一次当奶爸.epub` | 第一次当奶爸 |
  | 养育女孩 | `9028-养育女孩（成长版）/YangYuNuHai.epub` | 养育女孩（成长版） |
- **中间产物**放 `.tools/ebooks/`(已 gitignore)。staging:`<book>/book.txt` + `<book>/chunks/`。
- **ingest**:`ingest-batch.sh` 按 `wiki/sources/$base` 存在与否跳过旧文件;新文件前缀与旧 181 个(`崔玉涛宝贝健康公开课`/`郑玉巧婴儿卷`)不冲突。
- **eval**:`generate_test_cases.py --config overlay/config/server.local.json --schema v2`(llmConfig 有 `customEndpoint/apiMode/apiKey/model`,可读);`rag_eval.py --test-cases` 接受自定义测试集;`run_eval.sh` 的测试集是写死的 `parenting_books.json`。
- **环境**:`TENCENT_TOKEN` 已设(len 54);server 二进制 `overlay/server/target/release/llm-wiki-server` 已存在。

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `scripts/ebook_split.py` | 转换(ebook-convert)+ 按章切分 + 超长再切 + 写 frontmatter/命名。纯函数 + CLI。 |
| `scripts/ebook_check.py` | 逐块 LLM 语义检查 + 内容 hash 缓存 + 报告 + `--fix` 截断修复。 |
| `scripts/ebook_run.sh` | 编排驱动:书单、subcommand `split`/`check`/`promote`。 |
| `scripts/tests/test_ebook_split.py` | splitter 单测(unittest,不依赖真实文件/LLM)。 |
| `scripts/tests/test_ebook_check.py` | checker 单测(mock `call_llm`)。 |

---

## Phase A — 工具构建

### Task 1: `ebook_split.py` 核心纯函数(章节解析/标题清洗/超长切分)

**Files:**
- Create: `scripts/ebook_split.py`
- Test: `scripts/tests/test_ebook_split.py`

- [ ] **Step 1: 写失败测试**

```python
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
        text = "\n\n".join(["一" * 1500, "二" * 1500, "三" * 1500])
        chunks = ebook_split.subsplit(text, max_chars=2000)
        self.assertGreater(len(chunks), 1)
        for c in chunks:
            self.assertLessEqual(len(c), 2000)
        # 段落不被腰斩
        self.assertNotIn("一" * 1499 + "二", "".join(chunks))

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
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest scripts.tests.test_ebook_split -v`
Expected: FAIL —— `ModuleNotFoundError: No module named 'ebook_split'`(导入即失败)

- [ ] **Step 3: 实现核心函数**

```python
#!/usr/bin/env python3
"""将 ebook-convert 输出的书籍 txt 按章节切分为带 frontmatter 的源文件。

用法:
  python3 scripts/ebook_split.py --epub X.epub --book 法伯睡眠宝典 \
      --source 法伯睡眠宝典 --out .tools/ebooks/法伯睡眠宝典/chunks
  python3 scripts/ebook_split.py --txt X.txt --book 法伯睡眠宝典 \
      --source 法伯睡眠宝典 --out .tools/ebooks/法伯睡眠宝典/chunks
"""
import argparse
import os
import re
import subprocess

# 正文章节标题:第X章 + 全角空格(区分 TOC 的半角空格)
DEFAULT_CHAPTER_RE = r"^第[0-9]+章　"
# 目录行(半角空格 或 第X部分),从 front 里剔除
TOC_LINE_RE = re.compile(r"^第[0-9]+章\s|^第[一二三四五六七八九十百]+部分")
SENTENCE_END = "。！？!?…"
FRONT_MIN_CHARS = 200  # front 多于该字符数视为「前言」保留


def convert_epub(epub_path: str, txt_path: str) -> None:
    """用 calibre 把 EPUB 转成纯文本。"""
    subprocess.run(["ebook-convert", epub_path, txt_path], check=True)


def find_chapter_heads(lines, chapter_re=DEFAULT_CHAPTER_RE):
    """切成 (front_lines, [(heading, content_lines), ...])。

    第一个章节标题(默认全角空格)之前全部算 front(版权/目录/序言);
    之后按章节标题行切分,标题行本身作为 heading。
    """
    front = []
    chapters = []
    cur_heading = None
    cur_content = []
    started = False
    for ln in lines:
        if re.match(chapter_re, ln):
            if started:
                chapters.append((cur_heading, cur_content))
            else:
                started = True
            cur_heading = ln
            cur_content = []
        else:
            if started:
                cur_content.append(ln)
            else:
                front.append(ln)
    if started:
        chapters.append((cur_heading, cur_content))
    return front, chapters


def clean_title(heading: str, max_len: int = 30) -> str:
    """从章节标题行提取纯标题:去「第X章」前缀、去引号、截断、去非法字符。"""
    t = re.sub(r"^第[0-9]+章[　\s]+", "", heading).strip()
    t = re.sub(r"[\"'“”「」]", "", t).strip()
    if len(t) > max_len:
        t = t[:max_len].rstrip()
    t = re.sub(r'[\\/:*?"<>|\r\n]', "", t)
    return t


def split_paragraphs(text: str):
    """按空行切段落。"""
    return [p.strip() for p in re.split(r"\n\s*\n", text) if p.strip()]


def subsplit(text: str, max_chars: int):
    """超长文本在段落边界切成 <= max_chars 的块;单段超长时在句尾标点后硬切。

    绝不把句子腰斩:段落内只允许在句尾标点后切。
    """
    paras = split_paragraphs(text)
    chunks = []
    cur = ""
    for p in paras:
        if len(cur) + len(p) + 1 <= max_chars:
            cur = (cur + "\n\n" + p).strip()
            continue
        if cur:
            chunks.append(cur)
        if len(p) <= max_chars:
            cur = p
            continue
        # 单段超长:按句尾标点硬切
        sentences = re.split(r"(?<=[。！？!?…])", p)
        cur = ""
        for s in sentences:
            if not s:
                continue
            if len(cur) + len(s) + 1 <= max_chars:
                cur = (cur + s).strip()
            else:
                if cur:
                    chunks.append(cur)
                cur = s
    if cur:
        chunks.append(cur)
    return chunks
```

- [ ] **Step 4: 运行确认通过**

Run: `python3 -m unittest scripts.tests.test_ebook_split -v`
Expected: `Ran 6 tests ... OK`

注意:若 `python -m unittest scripts.tests.test_ebook_split` 找不到 `scripts` 包,改用:
Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -p 'test_ebook_split.py' -v`(此时测试内 `sys.path.insert` 需指向 repo 根,见下方修正)
Expected: OK

> 修正说明:测试 `sys.path.insert(0, <scripts/../>)` = repo 根;`ebook_split.py` 在 repo 根的 `scripts/` 下。若用 `discover -s scripts/tests`,脚本自身目录(scripts/)已在 sys.path,`import ebook_split` 直接成功;若用 `-m unittest scripts.tests...`,需 `scripts/` 有 `__init__.py` 或改 import 方式。**统一采用 `discover -s scripts/tests` 方式跑测试**,并给 `scripts/` 加空 `__init__.py`(见 Task 2 Step 3)。

- [ ] **Step 5: 提交**

```bash
git add scripts/ebook_split.py scripts/tests/test_ebook_split.py
git commit -m "feat(ebook): chapter parsing, title cleaning, sub-split core functions"
```

---

### Task 2: `ebook_split.py` 写文件 + frontmatter + 命名 + main CLI

**Files:**
- Modify: `scripts/ebook_split.py`(追加 `build_frontmatter`/`write_chunks`/`main`)
- Create: `scripts/__init__.py`(空文件,让 discover 稳定)
- Modify: `scripts/tests/test_ebook_split.py`(追加命名/frontmatter 测试)

- [ ] **Step 1: 写失败测试(追加到 test_ebook_split.py)**

```python
class TestWriteChunks(unittest.TestCase):
    def test_naming_and_frontmatter(self):
        import tempfile
        d = tempfile.mkdtemp()
        front = ["前言内容" * 100]
        chapters = [("第1章　喂养", ["正文" * 500])]
        written = ebook_split.write_chunks(front, chapters, "好孕从卵子开始",
                                           "好孕，从卵子开始", d)
        self.assertEqual(written, [
            "好孕从卵子开始-00-前言.md",
            "好孕从卵子开始-01-喂养.md",
        ])
        content = open(os.path.join(d, written[1]), encoding="utf-8").read()
        self.assertIn("type: source_lesson", content)
        self.assertIn("source: 好孕，从卵子开始", content)
        self.assertIn("split_status: ebook_split", content)
        self.assertIn("chapter: 第1章　喂养", content)

    def test_toc_lines_removed_from_front(self):
        import tempfile
        d = tempfile.mkdtemp()
        front = ["版权信息", "第2章 关于睡眠 非快速眼动睡眠", "第3章 尿床 尿床的原因"]
        chapters = [("第1章　睡眠", ["正文" * 500])]
        written = ebook_split.write_chunks(front, chapters, "法伯睡眠宝典",
                                           "法伯睡眠宝典", d)
        self.assertEqual(written, ["法伯睡眠宝典-01-睡眠.md"])
        # front 全是 TOC 行,过滤后 < FRONT_MIN_CHARS → 不出「前言」块

    def test_subchunk_naming(self):
        import tempfile
        d = tempfile.mkdtemp()
        text = "\n\n".join("内容" * 800 for _ in range(6))
        chapters = [("第5章　百科", [text])]
        written = ebook_split.write_chunks([], chapters, "定本育儿百科",
                                           "定本育儿百科", d, max_chars=1000)
        self.assertEqual(written[0], "定本育儿百科-05-百科.md")
        self.assertEqual(written[1], "定本育儿百科-05-百科-2.md")
        self.assertGreater(len(written), 2)

    def test_empty_chapter_skipped(self):
        import tempfile
        d = tempfile.mkdtemp()
        chapters = [("第1章　有内容", ["正文" * 500]), ("第2章　空章", ["", ""])]
        written = ebook_split.write_chunks([], chapters, "西尔斯育儿经",
                                           "西尔斯育儿经", d)
        self.assertEqual(len(written), 1)
        self.assertNotIn("第2章", written[0])
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -v`
Expected: FAIL —— `AttributeError: module 'ebook_split' has no attribute 'write_chunks'`

- [ ] **Step 3: 实现 write_chunks/build_frontmatter/main**

```python
def build_frontmatter(source: str, chapter: str, title: str) -> str:
    return (
        "---\n"
        "type: source_lesson\n"
        f"source: {source}\n"
        f"chapter: {chapter}\n"
        f"title_text: {title}\n"
        "tags:\n"
        "  - source_lesson\n"
        f"  - {source}\n"
        "split_status: ebook_split\n"
        "---\n"
    )


def write_chunks(front, chapters, book, source, out_dir, max_chars=2500, dry_run=False):
    """生成并(非 dry_run)写出所有块;返回文件名列表。"""
    written = []
    # 前言块:剔除 TOC 行后仍 >= FRONT_MIN_CHARS 才保留
    front_text = "\n".join(l for l in front if not TOC_LINE_RE.match(l)).strip()
    if len(front_text) >= FRONT_MIN_CHARS:
        fname = f"{book}-00-前言.md"
        fm = build_frontmatter(source, "前言", "前言")
        body = f"# 前言\n\n{front_text}\n"
        if not dry_run:
            os.makedirs(out_dir, exist_ok=True)
            with open(os.path.join(out_dir, fname), "w", encoding="utf-8") as f:
                f.write(fm + body)
        written.append(fname)

    for idx, (heading, content) in enumerate(chapters, start=1):
        text = "\n".join(content).strip()
        if len(text) < 30:  # 空/过短章节(残留目录行)跳过
            continue
        title = clean_title(heading)
        parts = subsplit(text, max_chars)
        for pi, part in enumerate(parts, start=1):
            suffix = "" if pi == 1 else f"-{pi}"
            fname = f"{book}-{idx:02d}-{title}{suffix}.md"
            fm = build_frontmatter(source, heading, title)
            body = f"# {title}\n\n> 来源:{source} / {heading}\n\n{part}\n"
            if not dry_run:
                os.makedirs(out_dir, exist_ok=True)
                with open(os.path.join(out_dir, fname), "w", encoding="utf-8") as f:
                    f.write(fm + body)
            written.append(fname)
    return written


def main():
    ap = argparse.ArgumentParser(description="ebook txt 按章节切分为源文件")
    ap.add_argument("--epub", help="EPUB 路径(转换后切分)")
    ap.add_argument("--txt", help="已转换的 txt 路径(跳过转换)")
    ap.add_argument("--book", required=True, help="简化书名,用于文件名前缀")
    ap.add_argument("--source", help="原书名(默认同 --book),写入 frontmatter")
    ap.add_argument("--out", required=True, help="chunks 输出目录")
    ap.add_argument("--max-chars", type=int, default=2500)
    ap.add_argument("--heading-re", default=DEFAULT_CHAPTER_RE, help="章节标题正则")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    if args.epub:
        txt = args.txt or os.path.join(os.path.dirname(args.out), "book.txt")
        os.makedirs(os.path.dirname(txt), exist_ok=True)
        convert_epub(args.epub, txt)
    elif args.txt:
        txt = args.txt
    else:
        ap.error("需要 --epub 或 --txt 之一")

    with open(txt, encoding="utf-8") as f:
        lines = f.read().splitlines()

    source = args.source or args.book
    front, chapters = find_chapter_heads(lines, args.heading_re)
    print(f"[{args.book}] front={len(front)} lines, chapters={len(chapters)}")
    written = write_chunks(front, chapters, args.book, source, args.out,
                           args.max_chars, args.dry_run)
    for w in written:
        print("  + " + w)
    print(f"[{args.book}] total files: {len(written)}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: 运行确认通过**

Run: `cd /home/ab/overseas-github/llm_wiki-server && touch scripts/__init__.py && python3 -m unittest discover -s scripts/tests -v`
Expected: `Ran 10 tests ... OK`

- [ ] **Step 5: CLI 冒烟(--dry-run,不写盘)**

Run: `python3 scripts/ebook_split.py --txt /tmp/ebook-test/book.txt --book 法伯睡眠宝典 --source 法伯睡眠宝典 --out /tmp/ebook-test/chunks --dry-run`
Expected: 打印 `chapters=N` 与 `total files: M`,N 应接近 18(法伯正文 18 章),M ≈ N + 前言(±子块)。

- [ ] **Step 6: 提交**

```bash
git add scripts/ebook_split.py scripts/__init__.py scripts/tests/test_ebook_split.py
git commit -m "feat(ebook): write chunk files with frontmatter + naming + main CLI"
```

---

### Task 3: 真实书冒烟 —— 转换+切分《法伯睡眠宝典》

**Files:** 无代码改动;验证真实数据。

- [ ] **Step 1: 对真实 EPUB 切分(非 dry-run)**

Run: `python3 scripts/ebook_split.py --epub "/mnt/c/Users/Lenovo/Downloads/电子书/1454-法伯睡眠宝典/法伯睡眠宝典.epub" --book 法伯睡眠宝典 --source 法伯睡眠宝典 --out .tools/ebooks/法伯睡眠宝典/chunks`
Expected: 转换成功(输出 `front=... chapters=18` 左右),chunks 目录生成文件。

- [ ] **Step 2: 抽查输出质量**

Run: `ls .tools/ebooks/法伯睡眠宝典/chunks/ | head -25 && echo '---' && head -12 .tools/ebooks/法伯睡眠宝典/chunks/*-01-*.md`
Expected:
- 文件名形如 `法伯睡眠宝典-01-解决孩子天生的睡眠问题.md`(标题清洗后)。
- frontmatter 含 `type/source/chapter/title_text/split_status`;正文 `# 标题` + `> 来源:...`。
- 最后一章序号 ≈ 18,无遗漏大章。

- [ ] **Step 3: 核对字符数(防转换丢内容)**

Run: `wc -m .tools/ebooks/法伯睡眠宝典/book.txt`
Expected: ≈ 15.5 万字符(与之前探测一致);若明显偏小(如 <5 万)说明转换丢内容,停下检查。

- [ ] **Step 4: 提交(若 Step 2 发现需修,先改再提交)**

```bash
git add -A scripts/ && git commit -m "test(ebook): verify split output on real 法伯睡眠宝典"
```
(仅当有脚本改动时;纯验证无改动则跳过)

**检查点 A:** 到此处两个核心工具可用、真实书切分正确,再进入 Phase B。

---

### Task 4: `ebook_check.py` 检查 + 缓存 + 报告

**Files:**
- Create: `scripts/ebook_check.py`
- Test: `scripts/tests/test_ebook_check.py`

- [ ] **Step 1: 写失败测试**

```python
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
        content = open(report, encoding="utf-8").read()
        self.assertIn("MANUAL_REVIEW", content)
        self.assertIn("a.md", content)


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -v`
Expected: FAIL —— `ModuleNotFoundError: No module named 'ebook_check'`

- [ ] **Step 3: 实现 check/cache/report**

```python
#!/usr/bin/env python3
"""用 LLM 检查切分块语义完整性,输出报告,可自动修复截断块(--fix)。

用法:
  python3 scripts/ebook_check.py --chunks .tools/ebooks/法伯睡眠宝典/chunks \
      --cache .tools/ebooks/法伯睡眠宝典/check-cache.json \
      [--config overlay/config/llm.judge.a.json] \
      [--report .tools/ebooks/法伯睡眠宝典/report.md] \
      [--only-long] [--sample N] [--fix]
"""
import argparse
import hashlib
import json
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO_ROOT, "overlay", "eval"))
from judge.llm_client import load_llm_config, call_llm, parse_json_response  # noqa: E402

SENTENCE_END = "。！？!?…"
PROMPT_TEMPLATE = """你是文本切分质检员。下面是从育儿书中切出的一段文本。判断:
1. 是否被截断:句子/段落是否在中间断开(结尾无句号/引号/完整意思)。
2. 是否语义自包含:脱离上下文能否独立理解;有无「见上文」「如前所述」等悬空指代。
3. 是否与相邻块重复或缺失整段。
只输出 JSON,格式: {{"ok": true|false, "severity": "ok"|"truncated"|"dangling"|"duplicate", "issue": "一句话说明"}}。

当前块:
=====
{chunk}
=====

若为纯目录/版权/作者信息噪声,输出 {{"ok": true, "severity": "ok", "issue": "noise"}}。
"""


def load_cache(path):
    if path and os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    return {}


def save_cache(cache, path):
    if path:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            json.dump(cache, f, ensure_ascii=False, indent=2)


def check_chunk(path, config, cache, check_fn=call_llm):
    """检查单块;按内容 hash 命中缓存直接返回 (verdict, was_cached)。"""
    with open(path, encoding="utf-8") as f:
        content = f.read()
    fname = os.path.basename(path)
    h = hashlib.sha256(content.encode("utf-8")).hexdigest()
    cached = cache.get(fname)
    if cached and cached.get("hash") == h:
        return cached["verdict"], True
    resp = check_fn(PROMPT_TEMPLATE.format(chunk=content[:8000]), config)
    verdict = parse_json_response(resp)
    cache[fname] = {"hash": h, "verdict": verdict}
    return verdict, False


def collect_chunks(chunks_dir, only_long=False, sample=None):
    paths = sorted(os.path.join(chunks_dir, f) for f in os.listdir(chunks_dir)
                   if f.endswith(".md"))
    if only_long:
        def _charcount(p):
            with open(p, encoding="utf-8") as f:
                return len(f.read())
        paths = [p for p in paths if _charcount(p) > 2500]
    if sample:
        paths = paths[:sample]
    return paths


def write_report(report_path, results, cache):
    os.makedirs(os.path.dirname(report_path), exist_ok=True)
    manual = [k for k, v in cache.items()
              if v["verdict"].get("severity") in ("dangling", "duplicate")]
    lines = ["# 切分语义检查报告\n", "## 需人工复核(MANUAL_REVIEW)\n"]
    if manual:
        for k in manual:
            v = cache[k]["verdict"]
            lines.append(f"- **{k}**: {v.get('severity')} — {v.get('issue')}")
    else:
        lines.append("(无)")
    lines.append("\n## 全部结果\n")
    for path, verdict, cached in results:
        mark = "OK" if verdict.get("ok") else verdict.get("severity", "?")
        lines.append(f"- [{'C' if cached else ' '}] {os.path.basename(path)}: "
                     f"{mark} {verdict.get('issue', '')}")
    with open(report_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


def main():
    ap = argparse.ArgumentParser(description="LLM 切分语义检查")
    ap.add_argument("--chunks", required=True)
    ap.add_argument("--config", default=os.path.join(REPO_ROOT, "overlay/config/llm.judge.a.json"))
    ap.add_argument("--cache", required=True)
    ap.add_argument("--report")
    ap.add_argument("--only-long", action="store_true")
    ap.add_argument("--sample", type=int)
    ap.add_argument("--fix", action="store_true")
    args = ap.parse_args()

    config = load_llm_config(args.config)
    cache = load_cache(args.cache)
    paths = collect_chunks(args.chunks, args.only_long, args.sample)
    report = args.report or os.path.join(os.path.dirname(args.cache), "report.md")
    print(f"checking {len(paths)} chunks ...")
    results = []
    for p in paths:
        verdict, cached = check_chunk(p, config, cache)
        results.append((p, verdict, cached))
        print(f"  {os.path.basename(p)}: {verdict.get('severity')} {verdict.get('issue', '')}")
    save_cache(cache, args.cache)
    if args.fix:
        # fix_truncated 由 Task 5 定义;批处理用到 --fix 时必已实现
        n = fix_truncated(paths, cache, config)
        print(f"fixed {n} truncated chunk(s)")
        save_cache(cache, args.cache)
    write_report(report, results, cache)
    print(f"report -> {report}")


if __name__ == "__main__":
    main()
```

> 注意:Task 5 会实现 `fix_truncated`;若 Task 4 先跑 CLI 且没实现 `fix_truncated`,`--fix` 分支会 ImportError——Task 4 验收用 `--sample` 即可,`--fix` 在 Task 5 之后用。

- [ ] **Step 4: 运行确认通过**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -v`
Expected: `Ran 15 tests ... OK`(Task 1–4 合计)

- [ ] **Step 5: 提交**

```bash
git add scripts/ebook_check.py scripts/tests/test_ebook_check.py
git commit -m "feat(ebook): LLM semantic check with hash cache + report"
```

---

### Task 5: `ebook_check.py` `--fix` 截断修复

**Files:**
- Modify: `scripts/ebook_check.py`(追加 `fix_truncated`)
- Modify: `scripts/tests/test_ebook_check.py`(追加测试)

- [ ] **Step 1: 写失败测试(追加)**

```python
class TestFixTruncated(unittest.TestCase):
    def test_moves_partial_tail_to_next_chunk(self):
        d = tempfile.mkdtemp()
        p1 = make_chunk(d, "a.md", "第一段完整。\n\n第二段未")
        p2 = make_chunk(d, "b.md", "完结的内容。")
        config = {}

        def fake(prompt, cfg):
            if "第一段完整" in prompt:
                return '{"ok": false, "severity": "truncated", "issue": "结尾缺句号"}'
            return '{"ok": true, "severity": "ok", "issue": ""}'

        changed = ebook_check.fix_truncated([p1, p2], {}, config, check_fn=fake)
        self.assertEqual(changed, 1)
        self.assertIn("第二段未", open(p2, encoding="utf-8").read())
        self.assertNotIn("第二段未", open(p1, encoding="utf-8").read())

    def test_complete_tail_not_moved(self):
        d = tempfile.mkdtemp()
        p1 = make_chunk(d, "a.md", "第一段完整。\n\n第二段也完整。")
        p2 = make_chunk(d, "b.md", "完结。")
        config = {}

        def fake(prompt, cfg):
            return '{"ok": false, "severity": "truncated", "issue": "x"}'

        changed = ebook_check.fix_truncated([p1, p2], {}, config, check_fn=fake)
        self.assertEqual(changed, 0)  # 尾段以句号结尾 → 不搬,交给人工复核
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -v`
Expected: FAIL —— `AttributeError: module 'ebook_check' has no attribute 'fix_truncated'`

- [ ] **Step 3: 实现 fix_truncated**

```python
def fix_truncated(paths, cache, config, check_fn=call_llm):
    """把「截断」块的末尾未完结段落到下一块正文开头。返回被改动块数。

    仅当尾段不以句尾标点结尾时搬移(那才是真正被切开的句子);
    其余截断判定(悬空/重复/可疑)留给 MANUAL_REVIEW。
    """
    changed = 0
    for i, p in enumerate(paths[:-1]):
        verdict, _ = check_chunk(p, config, cache, check_fn)
        if verdict.get("severity") != "truncated":
            continue
        with open(p, encoding="utf-8") as f:
            text = f.read()
        paras = text.split("\n\n")
        tail = paras[-1].rstrip()
        if tail.endswith(tuple(SENTENCE_END)):
            continue  # 尾段其实是完整句子 → 人工复核,不自动搬
        nxt = paths[i + 1]
        with open(nxt, encoding="utf-8") as f:
            nxt_text = f.read()
        # 下一块正文起点:frontmatter+标题(块0) / > 来源(块1)之后 → 插到 index 2
        blocks = nxt_text.split("\n\n")
        blocks.insert(2, tail)
        with open(nxt, "w", encoding="utf-8") as f:
            f.write("\n\n".join(blocks))
        paras.pop()
        with open(p, "w", encoding="utf-8") as f:
            f.write("\n\n".join(paras))
        changed += 1
        cache.pop(os.path.basename(p), None)
        cache.pop(os.path.basename(nxt), None)
    return changed
```

- [ ] **Step 4: 运行确认通过**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -v`
Expected: `Ran 17 tests ... OK`

- [ ] **Step 5: 提交**

```bash
git add scripts/ebook_check.py scripts/tests/test_ebook_check.py
git commit -m "feat(ebook): --fix moves truncated tail to next chunk + tests"
```

---

### Task 6: 真实 LLM 冒烟 —— 检查《法伯睡眠宝典》chunks

**Files:** 无代码改动;验证真实 LLM 链路。

- [ ] **Step 1: 抽样检查(2 块,快速验证链路)**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 scripts/ebook_check.py --chunks .tools/ebooks/法伯睡眠宝典/chunks --cache .tools/ebooks/法伯睡眠宝典/check-cache.json --sample 2`
Expected: 打印 2 行 `xxx.md: ok ...`(或 `truncated/dangling/duplicate`),并生成 `report.md`;`TENCENT_TOKEN` 已注入,无需手动设。

- [ ] **Step 2: 确认缓存生效(第二次不调 LLM)**

Run: 同样的命令再跑一次
Expected: 结果行带 `[C]`(缓存命中),且很快返回(无 LLM 调用)。

- [ ] **Step 3: 全量检查该本书(真实耗时/成本观察)**

Run: `python3 scripts/ebook_check.py --chunks .tools/ebooks/法伯睡眠宝典/chunks --cache .tools/ebooks/法伯睡眠宝典/check-cache.json`
Expected: 全部块检查完,报告无异常或仅个别 `dangling`(正常,交人工复核)。记录:块数、耗时、`MANUAL_REVIEW` 条目,作为后续 8 本的耗时估算。

**检查点 B:** LLM 链路 + 缓存 + 修复已验证,进入批量阶段。

---

## Phase B — 批量执行(8 本)

### Task 7: 编排脚本 `ebook_run.sh` + 全量切分

**Files:**
- Create: `scripts/ebook_run.sh`

- [ ] **Step 1: 创建编排脚本**

```bash
#!/usr/bin/env bash
# 电子书批量入库编排:书单 + split/check/promote 子命令。
# 用法:
#   ./scripts/ebook_run.sh split [BOOK...]     # 转换+切分到 .tools staging
#   ./scripts/ebook_run.sh check [BOOK...]     # LLM 语义检查(逐本,断点续跑)
#   ./scripts/ebook_run.sh promote [BOOK...]   # 拷 chunks 到 raw/sources
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC_BASE="/mnt/c/Users/Lenovo/Downloads/电子书"
OUT_BASE="$ROOT/.tools/ebooks"
PROJECT="${LLM_WIKI_PROJECT:-$HOME/overseas-github/llm_wiki_projects/ParentingBooks}"

# 简化书名|epub相对路径|原书名(frontmatter source)
BOOKS=(
  "法伯睡眠宝典|1454-法伯睡眠宝典/法伯睡眠宝典.epub|法伯睡眠宝典"
  "崔玉涛自然养育法|11063-崔玉涛自然养育法/CuiYuTaoZiRanYangYuFa(J.epub|崔玉涛自然养育法"
  "好孕从卵子开始|11153-好孕，从卵子开始/HaoYunCongLuanZiKaiShi.epub|好孕，从卵子开始"
  "成就好爸爸|2548-成就好爸爸：男人一生最重要的工作/成就好爸爸：男人一生最重要的工作.epub|成就好爸爸：男人一生最重要的工作"
  "定本育儿百科|290-定本育儿百科/DingBenYuErBaiKe.epub|定本育儿百科"
  "西尔斯育儿经|291-西尔斯育儿经/XiErSiYuErJing.epub|西尔斯育儿经"
  "第一次当奶爸|738-第一次当奶爸/第一次当奶爸.epub|第一次当奶爸"
  "养育女孩|9028-养育女孩（成长版）/YangYuNuHai.epub|养育女孩（成长版）"
)

cmd="${1:-split}"
shift || true
selected=("$@")

for entry in "${BOOKS[@]}"; do
  IFS='|' read -r book rel source <<< "$entry"
  if [[ ${#selected[@]} -gt 0 ]]; then
    keep=0
    for s in "${selected[@]}"; do [[ "$s" == "$book" ]] && keep=1; done
    [[ $keep -eq 0 ]] && continue
  fi
  case "$cmd" in
    split)
      echo "==> split: $book"
      python3 "$ROOT/scripts/ebook_split.py" --epub "$SRC_BASE/$rel" \
        --book "$book" --source "$source" --out "$OUT_BASE/$book/chunks"
      ;;
    check)
      echo "==> check: $book"
      python3 "$ROOT/scripts/ebook_check.py" \
        --chunks "$OUT_BASE/$book/chunks" \
        --config "$ROOT/overlay/config/llm.judge.a.json" \
        --cache "$OUT_BASE/$book/check-cache.json" \
        --report "$OUT_BASE/$book/report.md"
      ;;
    promote)
      echo "==> promote: $book"
      mkdir -p "$PROJECT/raw/sources"
      cp "$OUT_BASE/$book/chunks/"*.md "$PROJECT/raw/sources/"
      echo "    copied to $PROJECT/raw/sources/"
      ;;
    *) echo "unknown cmd: $cmd (split|check|promote)" >&2; exit 1 ;;
  esac
done
```

- [ ] **Step 2: 逐本核对章节正则(抽查 2 本非《法伯》的 txt)**

每本先转换出 txt,`grep -n '^第[0-9]*章'` 看正文标题是否也是「第X章 + 全角空格」:

```bash
cd /home/ab/overseas-github/llm_wiki-server
# 好孕从卵子开始
ebook-convert "/mnt/c/Users/Lenovo/Downloads/电子书/11153-好孕，从卵子开始/HaoYunCongLuanZiKaiShi.epub" /tmp/ebook-test/hy.txt 2>&1 | tail -1
grep -n '^第[0-9]*章' /tmp/ebook-test/hy.txt | head -5
# 定本育儿百科(大部头)
ebook-convert "/mnt/c/Users/Lenovo/Downloads/电子书/290-定本育儿百科/DingBenYuErBaiKe.epub" /tmp/ebook-test/db.txt 2>&1 | tail -1
grep -n '^第[0-9]*章' /tmp/ebook-test/db.txt | head -5
```

Expected: 标题行都是 `第X章　<标题>`(全角空格)。若个别书用半角空格或别的形式,记下该书并在 `ebook_run.sh` 里给该书加 `--heading-re`(split 分支加参数)。确认后**没有**章节格式异常的书再往下。

- [ ] **Step 3: 全量切分 8 本**

Run: `./scripts/ebook_run.sh split`
Expected: 每本打印 `chapters=N` 与文件数;8 本全部跑完,无报错。`ls .tools/ebooks/*/chunks/ | wc -l` 给出总块数(预计 300–600)。

- [ ] **Step 4: 核对章节数与书名**

```bash
for d in .tools/ebooks/*/chunks; do echo "$d: $(ls "$d" | wc -l) files"; done
```
Expected: 定本育儿百科文件数明显最多;每本 ≥ 1 个文件;无空目录。

- [ ] **Step 5: 提交**

```bash
git add scripts/ebook_run.sh
git commit -m "feat(ebook): batch orchestration script (book list, split/check/promote)"
```

---

### Task 8: 全量 LLM 语义检查 + 截断修复 + 人工复核

**Files:** 无代码改动;数据产出在 `.tools/ebooks/`。

- [ ] **Step 1: 全量检查 8 本(逐本,缓存断点续跑)**

Run: `./scripts/ebook_run.sh check`
Expected: 每本打印逐块结果;结束后每本有 `check-cache.json` 与 `report.md`。耗时取决于总块数(参考 Task 6 单本耗时 × 8)。

> 若成本敏感:对已抽查过且块全部 OK 的少量书,可先只 `--sample` 几块复核,但默认**全量**,符合设计(超长块必查)。

- [ ] **Step 2: 汇总人工复核清单**

```bash
grep -l "MANUAL_REVIEW" .tools/ebooks/*/report.md 2>/dev/null
grep -h "^\*\*" .tools/ebooks/*/report.md 2>/dev/null | sort | uniq -c | sort -rn | head -20
```
Expected: 列出各书 MANUAL_REVIEW 条目(通常为 dangling/duplicate,量少)。逐个查看 chunk 内容,人工决定:能补就补、能并就并,不能就保留并在报告标注。

- [ ] **Step 3: 自动修复截断块(若有)**

Run: `for b in 崔玉涛自然养育法 好孕从卵子开始 法伯睡眠宝典 成就好爸爸 定本育儿百科 西尔斯育儿经 第一次当奶爸 养育女孩; do python3 scripts/ebook_check.py --chunks ".tools/ebooks/$b/chunks" --config overlay/config/llm.judge.a.json --cache ".tools/ebooks/$b/check-cache.json" --fix; done`
Expected: 打印 `fixed N truncated chunk(s)`;被改块缓存已失效,下一步重查。

- [ ] **Step 4: 修复后复检(仅被改块,缓存命中其余)**

Run: `./scripts/ebook_run.sh check`
Expected: 原 truncated 块消失或变 OK;剩余 MANUAL_REVIEW 记录在案。

**检查点 C:** 8 本 chunks 全部 LLM 复检通过、人工复核清单明确,进入落地。

---

### Task 9: 落地 raw/sources + 更新 purpose.md

**Files:**
- Modify: `/home/ab/overseas-github/llm_wiki_projects/ParentingBooks/purpose.md`(仓库外)

- [ ] **Step 1: promote 到 raw/sources**

Run: `./scripts/ebook_run.sh promote`
Expected: `raw/sources/` 新增 ≈ 总块数文件;旧 181 个不受影响。
Verify: `ls ~/overseas-github/llm_wiki_projects/ParentingBooks/raw/sources/ | wc -l` ≈ 181 + 总块数。

- [ ] **Step 2: 更新 purpose.md(追加新书定位)**

读 `purpose.md` 后,把「核心目标」从只提郑玉巧/崔玉涛改为覆盖 10 本书,并在「内容特点·来源」追加:

```markdown
- 新增(2026-08):《崔玉涛自然养育法》《好孕，从卵子开始》《法伯睡眠宝典》《成就好爸爸：男人一生最重要的工作》《定本育儿百科》《西尔斯育儿经》《第一次当奶爸》《养育女孩（成长版）》。扩展覆盖:备孕/孕前、婴儿睡眠训练、父职参与、女孩养育、综合百科。
```

- [ ] **Step 3: 提交(仅仓库内文件;purpose.md 在仓库外,本地保存即可)**

```bash
git add -A scripts/
git commit -m "chore(ebook): promote staged chunks to raw/sources (sources live outside repo)"
```
若此提交无仓库内改动,则跳过并说明。

---

## Phase C — ingest + eval

### Task 10: ingest 批量入库

**Files:** 无代码改动。

- [ ] **Step 1: 跑 ingest-batch(新文件全走 LLM 生成 wiki 页)**

```bash
cd /home/ab/overseas-github/llm_wiki-server
LLM_WIKI_PROJECT="$HOME/overseas-github/llm_wiki_projects/ParentingBooks" \
LLM_WIKI_CONFIG=overlay/config/server.local.json \
INGEST_LOG=/tmp/llm-wiki-ingest-ebooks.log \
./scripts/ingest-batch.sh
```
Expected: 逐文件打印进度;旧的 181 个显示 `SKIP (already ingested)`;新文件逐个 ingest。耗时较长(每块一次 LLM 调用)。

- [ ] **Step 2: 验证 wiki/sources 增加**

Run: `ls "$HOME/overseas-github/llm_wiki_projects/ParentingBooks/wiki/sources/" | wc -l`
Expected: 明显大于 181(≈ 181 + 新块数)。有失败的话看 `/tmp/llm-wiki-ingest-ebooks.log` 中 `FAILED` 行,针对失败文件单独 `llm-wiki ingest` 重试。

- [ ] **Step 3: 提交(无仓库内改动则跳过)**

---

### Task 11: 启动 server + 生成新书测试用例 + eval/fix

**Files:**
- Create: `overlay/eval/test_cases/parenting_books_ebooks.json`(生成产物)

- [ ] **Step 1: 启动 server(:8080)**

```bash
cd /home/ab/overseas-github/llm_wiki-server
export LLM_WIKI_PROJECT="$HOME/overseas-github/llm_wiki_projects/ParentingBooks"
export LLM_WIKI_API_TOKEN="$(python3 -c 'import json; print(json.load(open("overlay/config/server.local.json")).get("apiConfig",{}).get("token",""))')"
export LLM_WIKI_CONFIG=overlay/config/server.local.json
export LLM_WIKI_STATIC=upstream/dist
nohup ./overlay/server/target/release/llm-wiki-server > /tmp/llm-wiki-server.log 2>&1 &
sleep 2
curl -s http://127.0.0.1:8080/api/v1/health | head -c 200
```
Expected: `{"ok":true,...}`。token 从 `server.local.json` 的 `apiConfig.token` 取;若为空,用构建时的 `VITE_API_TOKEN`(见 CLAUDE.md 检查清单里的 token)。server 无 token 时会对 `/search` 401——务必先确认 token 非空。

- [ ] **Step 2: 为新书生成测试用例(v2 schema)**

```bash
python3 overlay/eval/generate_test_cases.py \
  --project "$HOME/overseas-github/llm_wiki_projects/ParentingBooks" \
  --config overlay/config/server.local.json --schema v2 --mode auto \
  --target-count 60 \
  --output overlay/eval/test_cases/parenting_books_ebooks.json
```
Expected: 生成含 60 条 v2 用例的 JSON(`expected_sources.{must,should}`),覆盖新书章节。

- [ ] **Step 3: 跑 ingest_check + auto_fix(结构合规)**

Run: `./overlay/eval/scripts/run_eval.sh ParentingBooks all --fix`
Expected: health → ingest_check 报告(新书 source 页 frontmatter/链接密度)→ auto_fix 修复缺失字段/wikilink。**注意**:`run_eval.sh` 内置 rag_eval 用的是旧 `parenting_books.json`;新书用例单独跑 Step 4。

- [ ] **Step 4: 用新书用例跑 rag_eval**

```bash
python3 overlay/eval/rag_eval.py \
  --project "$HOME/overseas-github/llm_wiki_projects/ParentingBooks" \
  --test-cases overlay/eval/test_cases/parenting_books_ebooks.json \
  --mode all --token "<server.local.json apiConfig.token>" \
  --output overlay/eval/results/ebooks-$(date +%Y%m%d)
```
Expected: Recall@K / MRR 结果(新书指标,单独看,不与旧指标混比)。

- [ ] **Step 5: 提交测试用例 + 评估产物**

```bash
git add overlay/eval/test_cases/parenting_books_ebooks.json
git commit -m "eval(ebook): add v2 test cases for 8 new books"
```

**检查点 D(收尾):** ingest 成功、新书用例评估完成、`purpose.md` 已更新。整体流程闭环。

---

## 自检

**Spec 覆盖:**
- 转换 EPUB→txt + 字符数比对 → Task 3 Step 3 / Task 7 Step 2 ✓
- 章节切分 + 超长再切(段落边界,不腰斩句子)→ Task 1,2 ✓
- LLM 语义检查(截断/自包含/重复)+ 超长必查 + 报告 + 自动重切 → Task 4,5,8 ✓
- 简化命名 + frontmatter(source 用原书名)→ Task 2 ✓
- 跳过 52 号重复书 → 书单不含它 ✓
- 落地 raw/sources + purpose.md → Task 9 ✓
- ingest-batch → Task 10 ✓
- 新书生成测试用例 + run_eval/fix → Task 11 ✓
- 成本控制(缓存/--sample/--only-long)→ Task 4,6,8 ✓

**类型一致性:** `write_chunks(front, chapters, book, source, out_dir, max_chars, dry_run)` 签名在 Task 2 定义、Task 3/7 调用一致;`check_chunk(path, config, cache, check_fn)` 与 `fix_truncated(paths, cache, config, check_fn)` 在 Task 4/5 一致;`ebook_run.sh` 调用的 `--epub/--book/--source/--out`、`--chunks/--cache/--report` 均与 CLI 一致。
