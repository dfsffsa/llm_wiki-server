# 电子书批量复用实现计划(配置化驱动 + runbook)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把「电子书 → 切分 → 检查 → 落地」固化为配置驱动的可复用流程(batch JSON + `detect` 自动正则探测 + 测试用例跨书覆盖修复 + 新批次 runbook),让下次新增一批电子书 10 分钟内进入切分。

**Architecture:** `ebook_run.sh` 去掉三处硬编码(SRC_BASE/PROJECT/BOOKS),改为读一个 batch JSON 配置(经 `ebook_config.py` 校验并输出 TSV 供 bash 消费);新增 `ebook_detect.py` 启发式探测每本书的章节标题正则;`generate_test_cases.py` 遍历源文件前固定种子洗牌实现跨书覆盖;`docs/新批次电子书入库.md` 记录 10 步流程与全部踩坑。

**Tech Stack:** bash + Python 3.12(stdlib,无新依赖);复用现有 `ebook_split.py`/`ebook_check.py`/`ingest-parallel.sh`;测试用 unittest。

**设计 spec:** [2026-08-06-ebook-batch-reuse-design.md](../specs/2026-08-06-ebook-batch-reuse-design.md)

---

## 关键事实(执行前必读)

- 仓库根:`/home/ab/overseas-github/llm-wiki-server`(branch `main`,已 push 到 origin)。
- 现有工具(通用,勿改其接口):`scripts/ebook_split.py`(`--epub/--book/--source/--out/--heading-re/--max-chars/--dry-run`)、`scripts/ebook_check.py`(`--chunks/--config/--cache/--report/--fix`,增量缓存)、`scripts/ingest-parallel.sh`(env 驱动,空格安全)。
- `scripts/ebook_run.sh` 当前是硬编码版本(`SRC_BASE`/`PROJECT`/`BOOKS` 数组),本次改造为配置驱动。
- 配置字段含 `|`(正则 `^(CHAPTER [0-9]+|Part [IVX]+)　`)和空格(`CHAPTER 17` 等文件名),bash 一律用 TSV + `IFS=$'\t' read -r` 消费,勿裸拆词。
- `generate_test_cases.py` 的 `import random` 只在 `generate_human_review_list` 局部(第 576 行),模块顶层没有,需补顶层 import。
- 测试运行方式:
  - `cd /home/ab/overseas-github/llm-wiki-server && python3 -m unittest discover -s scripts/tests -v`
  - `cd /home/ab/overseas-github/llm-wiki-server && python3 -m unittest discover -s overlay/eval/tests -v`
- 环境:`TENCENT_TOKEN` 已设;`ebook-convert`(calibre)在 `/usr/bin/ebook-convert`;真实电子书在 `/mnt/c/Users/Lenovo/Downloads/电子书/`(WSL 挂载)。

---

## 文件结构

| 文件 | 职责 | 类型 |
|------|------|------|
| `scripts/ebook_config.py` | 读/校验 batch JSON,输出 props/books 的 TSV | 新建 |
| `scripts/ebook_run.sh` | 配置驱动编排(split/detect/check/fix/promote/pipeline) | 重写 |
| `scripts/ebooks/batches/example.json` | 批次配置格式模板(仓库内) | 新建 |
| `scripts/ebook_detect.py` | 探测章节标题正则候选(启发式,无 LLM) | 新建 |
| `overlay/eval/generate_test_cases.py` | 源文件遍历前固定种子洗牌(跨书覆盖) | 修改 |
| `docs/新批次电子书入库.md` | 新批次 runbook(10 步 + 踩坑) | 新建 |
| `scripts/tests/test_ebook_config.py` | config 校验/TSV 测试 | 新建 |
| `scripts/tests/test_ebook_detect.py` | detect 测试 | 新建 |
| `overlay/eval/tests/test_generate_test_cases.py` | shuffle 覆盖测试 | 新建 |

---

## Task 1: `ebook_config.py`(配置加载/校验/TSV 输出)

**Files:**
- Create: `scripts/ebook_config.py`
- Create: `scripts/tests/test_ebook_config.py`

- [ ] **Step 1: 写失败测试**

Create `scripts/tests/test_ebook_config.py`:

```python
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -p 'test_ebook_config.py' -v`
Expected: FAIL —— `ModuleNotFoundError: No module named 'ebook_config'`

- [ ] **Step 3: 实现**

Create `scripts/ebook_config.py`:

```python
#!/usr/bin/env python3
"""Load + validate an ebook batch JSON config; emit props / per-book rows as TSV for bash.

用法:
  python3 scripts/ebook_config.py load <config.json>    # 校验 + 摘要
  python3 scripts/ebook_config.py props <config.json>   # sourceDir\toutBase\tmaxChars\tproject
  python3 scripts/ebook_config.py books <config.json>   # 每书一行 book\tdir\tepub\tsource\theadingRe
"""
import json
import sys

REQUIRED_TOP = ("name", "sourceDir", "books")
REQUIRED_BOOK = ("dir", "epub", "book", "source")
DEFAULT_OUT_BASE = ".tools/ebooks"
DEFAULT_MAX_CHARS = 2500


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def validate(cfg):
    errors = []
    for k in REQUIRED_TOP:
        if k not in cfg:
            errors.append(f"missing top-level field: {k}")
    books = cfg.get("books")
    if not isinstance(books, list) or len(books) == 0:
        errors.append("books must be a non-empty list")
    else:
        for i, b in enumerate(books):
            if not isinstance(b, dict):
                errors.append(f"books[{i}] must be an object")
                continue
            for k in REQUIRED_BOOK:
                if k not in b:
                    errors.append(f"books[{i}] missing field: {k}")
    if errors:
        raise ValueError("; ".join(errors))
    return cfg


def props_tsv(cfg):
    return "\t".join([
        cfg["sourceDir"],
        cfg.get("outBase", DEFAULT_OUT_BASE),
        str(cfg.get("maxChars", DEFAULT_MAX_CHARS)),
        cfg.get("project", ""),
    ])


def books_tsv(cfg):
    rows = []
    for b in cfg["books"]:
        rows.append("\t".join([
            b["book"], b["dir"], b["epub"], b["source"], b.get("headingRe", ""),
        ]))
    return "\n".join(rows)


def main():
    if len(sys.argv) < 3:
        print("usage: ebook_config.py (load|props|books) <config.json>", file=sys.stderr)
        sys.exit(2)
    cmd, path = sys.argv[1], sys.argv[2]
    try:
        cfg = validate(load(path))
    except (OSError, json.JSONDecodeError, ValueError) as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(1)
    if cmd == "books":
        print(books_tsv(cfg))
    elif cmd == "props":
        print(props_tsv(cfg))
    else:
        print(f"ok: name={cfg['name']} books={len(cfg['books'])} "
              f"sourceDir={cfg['sourceDir']}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: 运行确认通过**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -v`
Expected: `Ran 29 tests ... OK`(现有 27 + 本任务新增部分)

- [ ] **Step 5: CLI 冒烟**

Run: `python3 scripts/ebook_config.py load <(echo '{"name":"t","sourceDir":"/d","books":[{"dir":"d","epub":"a","book":"b","source":"s"}]}')`
Expected: 输出 `ok: name=t books=1 sourceDir=/d`(若 `<(...)` 进程替换不可用,改为写临时文件再 load)
再跑一次 `books` 子命令,确认输出 `b\td\ta\ts\t`(5 个 tab 字段)。

- [ ] **Step 6: 提交**

```bash
git add scripts/ebook_config.py scripts/tests/test_ebook_config.py
git commit -m "feat(ebook): batch JSON config loader — validate + props/books TSV"
```

---

## Task 2: 重写 `ebook_run.sh`(配置驱动)+ `example.json`

**Files:**
- Rewrite: `scripts/ebook_run.sh`
- Create: `scripts/ebooks/batches/example.json`

- [ ] **Step 1: 创建 example.json 模板**

Create `scripts/ebooks/batches/example.json`:

```json
{
  "name": "example-batch",
  "sourceDir": "/mnt/c/Users/Lenovo/Downloads/电子书",
  "project": "ParentingBooks",
  "outBase": ".tools/ebooks",
  "maxChars": 2500,
  "books": [
    {
      "dir": "1454-法伯睡眠宝典",
      "epub": "法伯睡眠宝典.epub",
      "book": "法伯睡眠宝典",
      "source": "法伯睡眠宝典",
      "headingRe": "^第[0-9]+章　"
    }
  ]
}
```

- [ ] **Step 2: 重写 `scripts/ebook_run.sh`**

Replace the entire file with:

```bash
#!/usr/bin/env bash
# 电子书批量入库编排(配置驱动)。
# 用法:
#   ./scripts/ebook_run.sh -c <batch.json> split|detect|check|fix|promote|pipeline [BOOK...]
# 配置模板: scripts/ebooks/batches/example.json
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -c|--config) CONFIG="$2"; shift 2 ;;
    -*) echo "unknown option: $1" >&2; exit 1 ;;
    *) break ;;
  esac
done
cmd="${1:-split}"
shift || true
selected=("$@")

if [[ -z "$CONFIG" ]]; then
  echo "error: 需要 -c <batch.json>(模板见 scripts/ebooks/batches/example.json)" >&2
  exit 1
fi

# 配置路径解析:绝对直接用;相对依次试 scripts/ebooks/batches → .tools/ebooks/batches → cwd
if [[ "$CONFIG" != /* ]]; then
  for base in "$ROOT/scripts/ebooks/batches" "$ROOT/.tools/ebooks/batches" "$PWD"; do
    if [[ -f "$base/$CONFIG" ]]; then CONFIG="$base/$CONFIG"; break; fi
  done
fi
[[ -f "$CONFIG" ]] || { echo "error: config not found: $CONFIG" >&2; exit 1; }

# 读取配置(props: sourceDir outBase maxChars project)
IFS=$'\t' read -r SOURCE_DIR OUT_BASE MAX_CHARS PROJECT <<< "$(python3 "$ROOT/scripts/ebook_config.py" props "$CONFIG")"
PROJECT="${LLM_WIKI_PROJECT:-$PROJECT}"
if [[ -z "$PROJECT" ]]; then
  echo "error: 未指定 project(配置里或 LLM_WIKI_PROJECT env)" >&2
  exit 1
fi
PROJECT_DIR="$HOME/overseas-github/llm_wiki_projects/$PROJECT"

BOOK_ROWS="$(python3 "$ROOT/scripts/ebook_config.py" books "$CONFIG")"
[ -n "$BOOK_ROWS" ] || { echo "error: 配置里没有书" >&2; exit 1; }

is_selected() {
  [[ ${#selected[@]} -eq 0 ]] && return 0
  for s in "${selected[@]}"; do [[ "$s" == "$1" ]] && return 0; done
  return 1
}

do_split() {
  local book="$1" dir="$2" epub="$3" source="$4" hre="$5"
  local heading_args=()
  [[ -n "$hre" ]] && heading_args=(--heading-re "$hre")
  echo "==> split: $book"
  python3 "$ROOT/scripts/ebook_split.py" --epub "$SOURCE_DIR/$dir/$epub" \
    --book "$book" --source "$source" --out "$OUT_BASE/$book/chunks" \
    --max-chars "$MAX_CHARS" "${heading_args[@]}"
}

do_detect() {
  local book="$1" dir="$2" epub="$3"
  echo "==> detect: $book"
  python3 "$ROOT/scripts/ebook_detect.py" --epub "$SOURCE_DIR/$dir/$epub"
}

do_check() {
  local book="$1" fix="${2:-}"
  local fix_args=()
  [[ "$fix" == "--fix" ]] && fix_args=(--fix)
  echo "==> ${fix:+fix }check: $book"
  python3 "$ROOT/scripts/ebook_check.py" \
    --chunks "$OUT_BASE/$book/chunks" \
    --config "$ROOT/overlay/config/llm.judge.a.json" \
    --cache "$OUT_BASE/$book/check-cache.json" "${fix_args[@]}"
}

do_promote() {
  local book="$1"
  local chunks_dir="$OUT_BASE/$book/chunks"
  shopt -s nullglob
  local files=("$chunks_dir"/*.md)
  shopt -u nullglob
  if [[ ${#files[@]} -eq 0 ]]; then
    echo "    WARNING: no chunks for $book, skip promote" >&2
    return 0
  fi
  echo "==> promote: $book"
  mkdir -p "$PROJECT_DIR/raw/sources"
  cp "${files[@]}" "$PROJECT_DIR/raw/sources/"
  echo "    copied ${#files[@]} files to $PROJECT_DIR/raw/sources/"
}

while IFS=$'\t' read -r book dir epub source hre; do
  is_selected "$book" || continue
  case "$cmd" in
    split)   do_split "$book" "$dir" "$epub" "$source" "$hre" ;;
    detect)  do_detect "$book" "$dir" "$epub" ;;
    check)   do_check "$book" ;;
    fix)     do_check "$book" --fix ;;
    promote) do_promote "$book" ;;
    pipeline)
      do_split "$book" "$dir" "$epub" "$source" "$hre"
      do_check "$book"
      do_check "$book" --fix
      do_promote "$book"
      ;;
    *) echo "unknown cmd: $cmd (split|detect|check|fix|promote|pipeline)" >&2; exit 1 ;;
  esac
done <<< "$BOOK_ROWS"
```

- [ ] **Step 3: 校验脚本语法**

Run: `bash -n scripts/ebook_run.sh && chmod +x scripts/ebook_run.sh && echo "syntax OK"`
Expected: `syntax OK`

- [ ] **Step 4: 配置解析冒烟(不触发真实切分)**

Run: `./scripts/ebook_run.sh -c example.json detect 法伯睡眠宝典` 2>&1 | head -5
Expected: 走通配置解析(SOURCE_DIR/PROJECT/BOOK_ROWS 读取无错),进入 detect(因 ebook_detect.py 尚未创建,此步预期报 `No such file or directory` —— **在 Task 4 完成前这是预期失败**,仅验证配置解析部分;若报的是配置错误则说明解析有问题)。

> 若想单独验证配置解析,可临时把 `do_detect` 改成 `echo "==> detect: $book (config parsed)"` 跑一次再改回。

- [ ] **Step 5: 提交**

```bash
git add scripts/ebook_run.sh scripts/ebooks/batches/example.json
git commit -m "feat(ebook): config-driven ebook_run.sh (batch JSON) + example template"
```

---

## Task 3: `ebook_detect.py`(自动正则探测)

**Files:**
- Create: `scripts/ebook_detect.py`
- Create: `scripts/tests/test_ebook_detect.py`

- [ ] **Step 1: 写失败测试**

Create `scripts/tests/test_ebook_detect.py`:

```python
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -p 'test_ebook_detect.py' -v`
Expected: FAIL —— `ModuleNotFoundError: No module named 'ebook_detect'`

- [ ] **Step 3: 实现**

Create `scripts/ebook_detect.py`:

```python
#!/usr/bin/env python3
"""探测电子书 txt 的章节标题正则候选。

用法:
  python3 scripts/ebook_detect.py --txt book.txt
  python3 scripts/ebook_detect.py --epub book.epub [--txt out.txt]
输出候选正则 + 命中样本,供人工确认后填入批次配置 headingRe。
"""
import argparse
import os
import re
import subprocess

# (正则, 描述) —— 覆盖常见中英文书章节格式
CANDIDATES = [
    (r"^第[0-9]+章[　\s]", "第N章(全角/半角空格)"),
    (r"^第[一二三四五六七八九十百千]+章[　\s]?", "第X章(中文数字)"),
    (r"^CHAPTER [0-9]+[　\s]", "CHAPTER N"),
    (r"^Part [IVX]+[　\s]", "Part N(罗马数字)"),
    (r"^[0-9]{2} ", "两位数字+半角空格"),
    (r"^[0-9]+[\.．]", "数字+点/句点"),
]
SAMPLE_N = 5


def convert_epub(epub_path, txt_path):
    subprocess.run(["ebook-convert", epub_path, txt_path], check=True)


def load_lines(path):
    with open(path, encoding="utf-8") as f:
        return f.read().splitlines()


def analyze(lines, candidates=CANDIDATES, sample_n=SAMPLE_N):
    """对每个候选正则统计匹配数并抽样,返回 [{desc, regex, count, samples}]。"""
    results = []
    for regex, desc in candidates:
        pat = re.compile(regex)
        matched = [ln for ln in lines if pat.match(ln)]
        if matched:
            results.append({
                "desc": desc,
                "regex": regex,
                "count": len(matched),
                "samples": matched[:sample_n],
            })
    results.sort(key=lambda r: r["count"], reverse=True)
    return results


def render(results):
    if not results:
        return "未找到任何候选章节标题模式。\n"
    lines = ["候选章节标题正则(按匹配数排序):\n"]
    for r in results:
        lines.append(f"- {r['desc']}: `{r['regex']}`  (命中 {r['count']} 行)")
        for s in r["samples"]:
            lines.append(f"    {s[:60]}")
        lines.append("")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description="探测章节标题正则")
    ap.add_argument("--epub", help="EPUB 路径(先转换)")
    ap.add_argument("--txt", help="已转换的 txt 路径")
    args = ap.parse_args()
    if args.epub:
        txt = args.txt or "/tmp/ebook-detect.txt"
        convert_epub(args.epub, txt)
    elif args.txt:
        txt = args.txt
    else:
        ap.error("需要 --epub 或 --txt")
    lines = load_lines(txt)
    print(render(analyze(lines)))


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: 运行确认通过**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s scripts/tests -v`
Expected: `Ran 34 tests ... OK`(29 + 5 新增)

- [ ] **Step 5: 提交**

```bash
git add scripts/ebook_detect.py scripts/tests/test_ebook_detect.py
git commit -m "feat(ebook): detect chapter heading regex candidates (heuristic, no LLM)"
```

---

## Task 4: 真实书 `detect` 冒烟(验证 8 本里的 2 本格式)

**Files:** 无代码改动。

- [ ] **Step 1: 对《法伯睡眠宝典》detect**

Run: `./scripts/ebook_run.sh -c example.json detect 法伯睡眠宝典`
Expected: 输出候选列表,其中含 `第N章(全角/半角空格): \`^第[0-9]+章[　\s]\`` 且命中样本为 `第1章　...` 等正文标题。

- [ ] **Step 2: 对《西尔斯育儿经》detect(需先有它的配置项)**

临时把 `scripts/ebooks/batches/example.json` 的 books 数组追加西尔斯条目:
```json
    {
      "dir": "291-西尔斯育儿经",
      "epub": "XiErSiYuErJing.epub",
      "book": "西尔斯育儿经",
      "source": "西尔斯育儿经",
      "headingRe": ""
    }
```
然后 Run: `./scripts/ebook_run.sh -c example.json detect 西尔斯育儿经`
Expected: 输出含 `CHAPTER N: \`^CHAPTER [0-9]+[　\s]\`` 候选,命中样本 `CHAPTER 1　...` 等。验证后**把西尔斯条目从 example.json 移除**(example 只保留法伯作演示,勿留 8 本)。

- [ ] **Step 3: 提交(若有脚本改动)**

若 Task 2 的 detect 子命令在 Task 3 之前跑过并留了调试改动,先回退;无改动则跳过此步并说明。

---

## Task 5: `generate_test_cases.py` 跨书覆盖修复

**Files:**
- Modify: `overlay/eval/generate_test_cases.py`
- Create: `overlay/eval/tests/test_generate_test_cases.py`

- [ ] **Step 1: 写失败测试**

Create `overlay/eval/tests/test_generate_test_cases.py`:

```python
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s overlay/eval/tests -p 'test_generate_test_cases.py' -v`
Expected: FAIL —— `ImportError: cannot import name 'shuffled_sources'`

- [ ] **Step 3: 实现**

Modify `overlay/eval/generate_test_cases.py`:

1. 在模块顶层 import 区(第 16–21 行附近)加 `import random`:
```python
import argparse
import json
import os
import re
import sys
import glob
import random
```

2. 在 `generate_v2_batch` 定义之前加一个 helper(放在文件靠前的函数区,例如 `scan_derived_pages` 附近):
```python
def shuffled_sources(raw_dir: str, seed: int = 42) -> List[str]:
    """按固定种子洗牌源文件列表,避免首本书独占用例(跨书覆盖)。"""
    files = sorted(glob.glob(f"{raw_dir}/*.md"))
    random.Random(seed).shuffle(files)
    return files
```

3. 把 `generate_v2_batch` 内的:
```python
    source_files = sorted(glob.glob(f"{raw_dir}/*.md"))
```
改为:
```python
    source_files = shuffled_sources(raw_dir)
```

- [ ] **Step 4: 运行确认通过**

Run: `cd /home/ab/overseas-github/llm_wiki-server && python3 -m unittest discover -s overlay/eval/tests -v`
Expected: `Ran 112 tests ... OK`(111 + 1 新增,或 112+ 视已加测试数)

- [ ] **Step 5: 提交**

```bash
git add overlay/eval/generate_test_cases.py overlay/eval/tests/test_generate_test_cases.py
git commit -m "fix(eval): shuffle sources with fixed seed for cross-book test-case coverage"
```

---

## Task 6: Runbook `docs/新批次电子书入库.md`

**Files:**
- Create: `docs/新批次电子书入库.md`

- [ ] **Step 1: 创建 runbook**

创建 `docs/新批次电子书入库.md`,内容为固定 10 步流程 + 踩坑速查。**每步必须给出实际命令**,结构如下(正文用中文,命令用代码块):

```markdown
# 新批次电子书入库 — 复用流程

> 目标:新一批电子书 → raw/sources 落地(切分+检查+修复) → ingest → eval。
> 前置:已跑通首次入库(工具在 `scripts/`、配置驱动见 `ebook_run.sh -c`)。
> 全流程 10 步,大多数步骤幂等可续跑。

## 0. 环境前置
- `TENCENT_TOKEN` 已设(检查/生成用例用 Tencent tokenhub)
- `LLM_WIKI_PROJECT` 或批次配置里的 `project`(ingest 用)
- `LLM_WIKI_CONFIG=overlay/config/server.local.json`(ingest/eval 用)

## 1. 建批次配置
复制模板: `cp scripts/ebooks/batches/example.json .tools/ebooks/batches/<批次名>.json`
填 `name/sourceDir/project/books[]`(dir=源子目录, epub, book=简化书名, source=原书名, headingRe 暂留空)。
校验: `python3 scripts/ebook_config.py load .tools/ebooks/batches/<批次名>.json`

## 2. 逐本探测章节正则
`./scripts/ebook_run.sh -c <批次名>.json detect [BOOK...]`
看输出的候选正则 + 命中样本,把确认的填进配置的 `headingRe`。
> 常见格式:`第1章　`→`^第[0-9]+章　`;`CHAPTER 1　`→`^CHAPTER [0-9]+[　\s]`;`1.标题`→`^[0-9]+[\.．]`;`第一章 标题`→`^第[一二三四五六七八九十百千]+章[　\s]?`

## 3. 测 LLM 配额
单文件试跑 ingest(不同步骤可能用不同端点,各自测):
`LLM_WIKI_PROJECT=... LLM_WIKI_CONFIG=... ./scripts/llm-wiki ingest <一个源文件> --project ... --config ...`
> 若 429「已达到 Token Plan 用量上限」→ 该端点配额耗尽,换端点或充值。

## 4. 切分
`./scripts/ebook_run.sh -c <批次名>.json split [BOOK...]`
> 幂等;可重跑。`.tools/ebooks/<book>/chunks/` 为 staging。

## 5. LLM 语义检查
`./scripts/ebook_run.sh -c <批次名>.json check [BOOK...]`
> 增量缓存(`check-cache.json`),断点续跑;超长块必查。

## 6. 修复截断
`./scripts/ebook_run.sh -c <批次名>.json fix [BOOK...]`
> 只处理"未完句尾"块;悬空/重复/拒绝 进 MANUAL_REVIEW(`.tools/ebooks/<book>/report.md`)。

## 7. 落地 raw/sources
`./scripts/ebook_run.sh -c <批次名>.json promote [BOOK...]`

## 8. 并行入库
`INGEST_WORKERS=4 LLM_WIKI_PROJECT="$HOME/overseas-github/llm_wiki_projects/<project>" LLM_WIKI_CONFIG=overlay/config/server.local.json ./scripts/ingest-parallel.sh`
长任务**脱离会话**: `setsid nohup env ... bash scripts/ingest-parallel.sh > /tmp/ingest.log 2>&1 &`
进度: `tail /tmp/ingest.log`;ok/FAILED 标记在**主日志**。

## 9. 生成测试用例
`python3 overlay/eval/generate_test_cases.py --project <路径> --config overlay/config/server.local.json --schema v2 --mode auto --target-count 60 --output overlay/eval/test_cases/<批次>.json`
> 已修复跨书覆盖(固定种子洗牌)。

## 10. eval
`./overlay/eval/scripts/run_eval.sh <项目名> all --fix`(ingest_check + auto_fix)
新书用例: `python3 overlay/eval/rag_eval.py --project <项目名> --test-cases overlay/eval/test_cases/<批次>.json --mode all --token <token>`

---

## 踩坑速查
| 坑 | 现象 | 解法 |
|---|---|---|
| 文件名含半角空格(`CHAPTER 17`) | 并行 ingest 静默漏文件 | 路径一律 `read -r`,勿裸拆词(已内置) |
| `rag_eval --project` 按项目名 | 传路径→回退第一个项目,0 命中 | 传**项目名** `ParentingBooks`,非路径 |
| `run_eval.sh` 的 rag_eval 不带 token | 401 | 直接 `rag_eval.py --token` |
| `pkill -f "tsx"` 匹配自身 | shell 被杀(exit 144) | 用 `[t]sx` 方括号 |
| 杀 ingest 留孤儿 node/tsx | 双跑 | 杀完 `pgrep -fc 'cmd-ingest'` 验证 0 |
| 长任务会话绑定 | 会话关闭即死 | `setsid nohup` 脱离 |
| LLM 拒绝/非 JSON | 崩整本 | `check` 已逐块容错(记 error) |
| 单本 10h 级任务 | 中途崩丢进度 | 增量缓存/`wiki/sources` 存在性续跑 |
```

- [ ] **Step 2: 校验内容无占位符**

通读一遍,确认 10 步都有具体命令、无 TBD。

- [ ] **Step 3: 提交**

```bash
git add docs/新批次电子书入库.md
git commit -m "docs(ebook): new-batch runbook — 10-step reuse flow + pitfalls"
```

---

## Task 7: 集成冒烟(配置驱动全链路,法伯单书)

**Files:** 无代码改动(若冒烟发现 bug 则回到对应任务修)。

- [ ] **Step 1: 配置驱动 split(法伯)**

Run: `./scripts/ebook_run.sh -c example.json split 法伯睡眠宝典 2>&1 | tail -6`
Expected: 打印 `==> split: 法伯睡眠宝典` + 切分统计(前端 N 行/chapters/full files),chunks 重新生成(幂等覆盖)。

- [ ] **Step 2: 配置驱动 check(法伯,走缓存)**

Run: `./scripts/ebook_run.sh -c example.json check 法伯睡眠宝典 2>&1 | tail -3`
Expected: 71 块全部 `[C]` 缓存命中,快速完成,无新 LLM 调用。

- [ ] **Step 3: 配置驱动 promote(法伯,幂等)**

Run: `./scripts/ebook_run.sh -c example.json promote 法伯睡眠宝典 2>&1 | tail -3`
Expected: 打印 `copied 71 files to .../raw/sources/`,raw/sources 已有法伯文件被覆盖(无副作用)。

- [ ] **Step 4: 全量测试回归**

Run: `python3 -m unittest discover -s scripts/tests -v && python3 -m unittest discover -s overlay/eval/tests -v`
Expected: 两套全过(scripts ~34 +,eval ~112 +)。

- [ ] **Step 5: 提交(若有修复)**

若冒烟发现问题并修复,提交;否则说明"无代码改动,冒烟通过"。

---

## 自检

**Spec 覆盖:**
- §3 配置格式(JSON + 路径解析)→ Task 1,2 ✓
- §4 ebook_run.sh 配置驱动 + 子命令(split/detect/check/fix/promote/pipeline)→ Task 2 ✓
- §5 自动正则探测 → Task 3,4 ✓
- §6 测试用例覆盖修复(固定种子洗牌)→ Task 5 ✓
- §7 Runbook(10 步 + 踩坑)→ Task 6 ✓
- §8 测试与验收(配置解析单测、detect 冒烟、用例覆盖验证、集成冒烟)→ Task 1,3,4,5,7 ✓

**类型一致性:** `ebook_config.props_tsv` 输出 4 字段,`ebook_run.sh` 用 `IFS=$'\t' read -r SOURCE_DIR OUT_BASE MAX_CHARS PROJECT` 读 —— 字段顺序一致;`books_tsv` 输出 5 字段 `book dir epub source headingRe`,bash 同样序读。`ebook_detect.analyze` 返回 `{desc, regex, count, samples}`,`render` 消费 —— 一致。
