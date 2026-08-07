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
