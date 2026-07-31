#!/usr/bin/env python3
"""
增量重跑：只重跑之前失败的 source_file，合并回原 JSON。

用法:
    python3 overlay/eval/rerun_failed.py \
        --project ~/overseas-github/llm_wiki_projects/ParentingBooks \
        --input docs/superpowers/ParentingBooks_llmjudge_results.json \
        --config-a overlay/config/llm.judge.a.json \
        --config-b overlay/config/llm.judge.b.json \
        --output /tmp/parenting-eval-merged.json \
        --verbose
"""

import argparse
import json
import os
import sys
from datetime import datetime

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from judge.llm_client import load_llm_config
from judge.extractor import extract_claims, parse_extracted_claims
from judge.evaluator import evaluate_wiki
from llm_judge import summarize, read_file


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--project", required=True)
    p.add_argument("--input", required=True, help="原评估结果 JSON")
    p.add_argument("--config-a", required=True)
    p.add_argument("--config-b", required=True)
    p.add_argument("--output", required=True)
    p.add_argument("--verbose", "-v", action="store_true")
    args = p.parse_args()

    with open(args.input, encoding="utf-8") as f:
        data = json.load(f)

    reports = data.get("reports", [])
    print(f"原结果: {len(reports)} 条", flush=True)

    failed_idx = [(i, r) for i, r in enumerate(reports)
                  if "error" in r.get("scores", {})]
    print(f"失败: {len(failed_idx)} 条，将重跑", flush=True)

    cfg_a = load_llm_config(args.config_a)
    cfg_b = load_llm_config(args.config_b)

    for n, (i, r) in enumerate(failed_idx, 1):
        src_rel = r["source_file"]
        wiki_rel = r["wiki_page"]
        src_path = os.path.join(args.project, src_rel)
        wiki_path = os.path.join(args.project, wiki_rel)

        src = read_file(src_path)
        wiki = read_file(wiki_path)
        if not src or not wiki:
            print(f"[{n}/{len(failed_idx)}] {src_rel} → 跳过（读不到文件）", flush=True)
            continue

        if args.verbose:
            print(f"[{n}/{len(failed_idx)}] {os.path.basename(src_rel)} ...",
                  end=" ", flush=True)

        try:
            claims_resp = extract_claims(src, os.path.basename(src_rel), cfg_a)
            claims = parse_extracted_claims(claims_resp)
            claims_text = json.dumps(claims, ensure_ascii=False, indent=2)
            report = evaluate_wiki(claims_text, wiki, cfg_b)
            report.source_file = src_rel
            report.wiki_page = wiki_rel
            reports[i] = report.to_dict()
            if args.verbose:
                cov = report.scores.get("coverage", "?")
                halls = len(report.hallucinations)
                print(f"coverage={cov} halls={halls}", flush=True)
        except Exception as e:
            if args.verbose:
                print(f"ERROR: {e}", flush=True)
            reports[i]["scores"] = {"error": str(e)}

        # checkpoint
        data["reports"] = reports
        data["summary"] = summarize(reports)
        data["rerun_timestamp"] = str(datetime.now())
        tmp = args.output + ".tmp"
        os.makedirs(os.path.dirname(os.path.abspath(args.output)), exist_ok=True)
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=2)
        os.replace(tmp, args.output)

    # 最终统计
    err = sum(1 for r in reports if "error" in r.get("scores", {}))
    print(f"\n合并完成: {len(reports)} 条，剩余错误 {err} 条", flush=True)
    print(json.dumps(summarize(reports), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
