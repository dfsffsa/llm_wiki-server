#!/usr/bin/env python3
"""
Ingest 自动修复管线

用法:
    python auto_fix.py --project <项目路径>                   # 检查 + 修复
    python auto_fix.py --project <项目路径> --dry-run          # 仅报告不修复
    python auto_fix.py --project <项目路径> --budget 5         # 最多修 5 个问题

流程:
    check -> 分类 fixable findings -> 对每个 finding 执行 fixer ->
    re-check -> 回归检查 -> 记录结果
"""
import argparse
import json
import os
import sys
from datetime import datetime
from typing import Dict

# 相对导入
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ingest_check import run_ingest_check, Finding
from fixers import FIXER_REGISTRY, get_fixer


def _to_finding(obj) -> Finding:
    """Convert a dict or Finding to a Finding instance."""
    if isinstance(obj, Finding):
        return obj
    return Finding(**obj)


def run_auto_fix(
    project_dir: str, dry_run: bool = False, budget: int = 3, verbose: bool = False
) -> Dict:
    # Phase 1: Check
    check_results = run_ingest_check(project_dir, verbose=verbose)
    findings_raw = check_results.get("findings", [])

    # run_ingest_check returns dicts (via to_dict()); convert to Finding objects
    findings = [_to_finding(f) for f in findings_raw]

    fixable = [f for f in findings if f.auto_fixable]
    unfixable = [f for f in findings if not f.auto_fixable]

    report = {
        "project": project_dir,
        "timestamp": str(datetime.now()),
        "total_findings": len(findings),
        "fixable_count": len(fixable),
        "unfixable_count": len(unfixable),
        "results": [],
        "score_before": check_results.get("overall_score", 0),
        "score_after": None,
        "errors": [],
    }

    if verbose:
        print(
            f"Findings: {len(findings)} total, {len(fixable)} fixable, "
            f"{len(unfixable)} unfixable"
        )
        for f in unfixable:
            print(f"  [MANUAL] {f.page}: {f.message}")

    if dry_run:
        for f in fixable:
            print(
                f"  [WOULD_FIX] {f.page}: {f.message} "
                f"(strategy={f.fix_strategy})"
            )
        report["dry_run"] = True
        return report

    # Phase 2: Fix (with budget)
    applied = 0
    for f in fixable:
        if applied >= budget:
            report["errors"].append(
                f"Budget reached ({budget}), remaining "
                f"{len(fixable) - applied} findings skipped"
            )
            break

        fixer = get_fixer(f.fix_strategy)
        if fixer is None:
            report["errors"].append(
                f"No fixer for strategy: {f.fix_strategy}"
            )
            continue

        try:
            result = fixer(project_dir, f)
            result["finding"] = {"page": f.page, "message": f.message}
            report["results"].append(result)
            if result.get("fixed"):
                applied += 1
                print(
                    f"  [FIXED] {f.page}: "
                    f"{result.get('summary', result.get('diff', 'ok'))}"
                )
            else:
                print(
                    f"  [SKIP] {f.page}: "
                    f"{result.get('error', 'no changes needed')}"
                )
        except Exception as e:
            report["errors"].append(f"Fix failed for {f.page}: {e}")
            print(f"  [ERROR] {f.page}: {e}")

    # Phase 3: Re-check
    if applied > 0:
        check_after = run_ingest_check(project_dir, verbose=False)
        report["score_after"] = check_after.get("overall_score", 0)
        remaining = [
            f
            for f in check_after.get("findings", [])
            if f.get("auto_fixable")
        ]
        report["remaining_after_fix"] = len(remaining)

        score_delta = report["score_after"] - report["score_before"]
        report["score_delta"] = round(score_delta, 1)
        if score_delta < 0:
            print(
                f"  [REGRESSION] Score dropped {score_delta} points! "
                "Check fix_backups/"
            )
    else:
        report["score_after"] = report["score_before"]
        report["score_delta"] = 0

    return report


def main():
    parser = argparse.ArgumentParser(description="Ingest 自动修复管线")
    parser.add_argument(
        "--project", "-p", required=True, help="项目路径"
    )
    parser.add_argument(
        "--dry-run", "-n", action="store_true", help="仅报告不修复"
    )
    parser.add_argument(
        "--budget",
        "-b",
        type=int,
        default=3,
        help="单次最大修复数",
    )
    parser.add_argument(
        "--verbose", "-v", action="store_true", help="详细输出"
    )
    parser.add_argument(
        "--output", "-o", help="结果输出 JSON 路径"
    )
    args = parser.parse_args()

    report = run_auto_fix(
        args.project, args.dry_run, args.budget, args.verbose
    )

    print(
        f"\nSummary: {report['fixable_count']} fixable, "
        f"{sum(1 for r in report['results'] if r.get('fixed'))} fixed, "
        f"{len(report['errors'])} errors"
    )
    if report.get("score_delta"):
        print(
            f"Score: {report['score_before']} -> "
            f"{report['score_after']} ({report['score_delta']:+})"
        )

    if args.output:
        out_dir = os.path.dirname(args.output)
        if out_dir:
            os.makedirs(out_dir, exist_ok=True)
        with open(args.output, "w", encoding="utf-8") as f:
            json.dump(report, f, ensure_ascii=False, indent=2)
        print(f"Report saved: {args.output}")


if __name__ == "__main__":
    main()
