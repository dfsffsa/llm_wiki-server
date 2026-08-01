#!/usr/bin/env python3
"""
加载已合并的评估结果 JSON，跑 auto-fix。

用法:
    python3 overlay/eval/run_auto_fix.py \
        --project ~/overseas-github/llm_wiki_projects/ParentingBooks \
        --config-b overlay/config/llm.judge.b.json \
        --input /tmp/parenting-eval-merged.json \
        --threshold 6 --dry-run
"""

import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from llm_judge import run_repairs


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--project", required=True)
    p.add_argument("--config-b", required=True)
    p.add_argument("--input", required=True, help="合并后的评估 JSON")
    p.add_argument("--threshold", type=int, default=6)
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--verbose", "-v", action="store_true")
    args = p.parse_args()

    with open(args.input, encoding="utf-8") as f:
        result = json.load(f)

    # 预过滤：只修 coverage<threshold 的页面，忽略幻觉数量
    filtered = []
    for r in result.get("reports", []):
        s = r.get("scores", {})
        if "error" in s:
            continue
        if s.get("coverage", 10) < args.threshold:
            filtered.append(r)
    result["reports"] = filtered
    print(f"将修复 coverage<{args.threshold} 的页面: {len(filtered)} 个", flush=True)
    if not args.dry_run:
        print("非 dry-run，将实际修改 wiki 文件", flush=True)

    fix = run_repairs(
        args.project, args.config_b, result,
        threshold=args.threshold, dry_run=args.dry_run,
        verbose=args.verbose,
    )
    print(json.dumps(fix["summary"], ensure_ascii=False, indent=2))

    out_path = args.input.replace(".json", "-autofix.json")
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(fix, f, ensure_ascii=False, indent=2)
    print(f"详情写入 {out_path}", flush=True)


if __name__ == "__main__":
    main()
