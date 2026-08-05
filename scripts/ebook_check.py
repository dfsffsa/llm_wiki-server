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
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO_ROOT, "overlay", "eval"))
from judge.llm_client import load_llm_config, call_llm, parse_json_response  # noqa: E402

SENTENCE_END = "。！？!?…"  # Task 5: used by fix_truncated
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
        d = os.path.dirname(path) or "."
        os.makedirs(d, exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            json.dump(cache, f, ensure_ascii=False, indent=2)


def check_chunk(path, config, cache, check_fn=call_llm):
    """检查单块;按内容 hash 命中缓存直接返回 (verdict, was_cached)。

    对 LLM 拒绝/输出异常容错:捕获后记录 error verdict 并缓存,不抛出。
    """
    with open(path, encoding="utf-8") as f:
        content = f.read()
    fname = os.path.basename(path)
    h = hashlib.sha256(content.encode("utf-8")).hexdigest()
    cached = cache.get(fname)
    if cached and cached.get("hash") == h:
        return cached["verdict"], True
    try:
        resp = check_fn(PROMPT_TEMPLATE.format(chunk=content[:8000]), config)
        verdict = parse_json_response(resp)
    except Exception as e:  # noqa: BLE001 - LLM 拒绝/超时/解析失败不阻塞跑批
        verdict = {"ok": False, "severity": "error",
                   "issue": f"LLM 未返回有效判定: {str(e)[:120]}"}
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
              if v["verdict"].get("severity") in ("dangling", "duplicate", "error")]
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
        if not tail:
            continue
        if tail.endswith(tuple(SENTENCE_END)):
            continue  # 尾段其实是完整句子 → 人工复核,不自动搬
        nxt = paths[i + 1]
        with open(nxt, encoding="utf-8") as f:
            nxt_text = f.read()
        # 下一块正文:frontmatter+标题(块0)/来源行(块1),插到 index 2 → 来源行之后、正文之前
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


def run_check(paths, config, cache, cache_path, check_fn=call_llm, save_every=1):
    """检查所有块，每 save_every 个块自动保存缓存（崩溃安全）。

    Returns:
        results: list of (path, verdict, was_cached)
    """
    results = []
    for i, p in enumerate(paths):
        verdict, cached = check_chunk(p, config, cache, check_fn)
        results.append((p, verdict, cached))
        print(f"  {os.path.basename(p)}: {verdict.get('severity')} {verdict.get('issue', '')}")
        if (i + 1) % save_every == 0:
            save_cache(cache, cache_path)
    return results


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
    results = run_check(paths, config, cache, args.cache)
    if args.fix:
        n = fix_truncated(paths, cache, config)
        print(f"fixed {n} truncated chunk(s)")
        # 重新检查被修改的块(缓存已失效),更新报告结果
        for i, (p, _, _) in enumerate(results):
            if os.path.basename(p) not in cache:
                verdict, _ = check_chunk(p, config, cache)
                results[i] = (p, verdict, False)
        save_cache(cache, args.cache)
    write_report(report, results, cache)
    print(f"report -> {report}")


if __name__ == "__main__":
    main()
