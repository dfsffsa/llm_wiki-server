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
    t = re.sub(r"[\"'\"\'“”「」『』《》]", "", t).strip()
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
    注意:单段超长且无句尾标点时无法切分,该块可能超过 max_chars。
    """
    paras = split_paragraphs(text)
    chunks = []
    cur = ""
    for p in paras:
        if len(cur) + len(p) + 2 <= max_chars:
            cur = (cur + "\n\n" + p).strip()
            continue
        if cur:
            chunks.append(cur)
        if len(p) <= max_chars:
            cur = p
            continue
        # 单段超长:按句尾标点硬切
        sentences = re.split(rf"(?<=[{re.escape(SENTENCE_END)}])", p)
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
        # 从标题提取章节编号(如 "第5章　..." → 5),维持原始顺序
        m = re.match(r"第([0-9]+)章", heading)
        ch_num = int(m.group(1)) if m else idx
        parts = subsplit(text, max_chars)
        for pi, part in enumerate(parts, start=1):
            suffix = "" if pi == 1 else f"-{pi}"
            fname = f"{book}-{ch_num:02d}-{title}{suffix}.md"
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
