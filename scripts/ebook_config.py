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
            for k in ("dir", "epub"):
                v = b.get(k, "")
                if ".." in v or "/" in v or "\\" in v:
                    errors.append(f"books[{i}] {k} must be a plain name (no path separators or ..): {v!r}")
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
    elif cmd == "load":
        print(f"ok: name={cfg['name']} books={len(cfg['books'])} "
              f"sourceDir={cfg['sourceDir']}")
    else:
        print(f"error: unknown command '{cmd}' (expected: load|props|books)", file=sys.stderr)
        sys.exit(2)


if __name__ == "__main__":
    main()
