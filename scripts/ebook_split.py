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
