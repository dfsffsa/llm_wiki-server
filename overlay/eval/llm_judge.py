#!/usr/bin/env python3
"""
LLM-as-Judge Ingest 质量评估

对 ingest 产出的 wiki 页面做内容级质量评估：
- 角色 A (Extractor): 从 source 中提取关键陈述清单
- 角色 B (Evaluator): 逐条比对 wiki 页面的覆盖度

用法:
    python llm_judge.py --project <path> --config <config.json>
    python llm_judge.py --project <path> --config <config.json> --sample 20
"""

import argparse
import json
import os
import random
import sys
import glob
from datetime import datetime

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from judge.llm_client import load_llm_config
from judge.extractor import extract_claims, parse_extracted_claims
from judge.evaluator import evaluate_wiki
from judge.models import JudgeReportItem
from judge.repairer import should_repair, repair_page, write_repaired_page, verify_repair


def find_source_wiki_pairs(project_dir: str, sample: int = None) -> list:
    """找到 raw/sources/*.md 和 wiki/sources/*.md 的配对"""
    raw_dir = os.path.join(project_dir, "raw", "sources")
    wiki_dir = os.path.join(project_dir, "wiki", "sources")
    pairs = []
    if not os.path.isdir(raw_dir):
        return pairs
    for rf in glob.glob(os.path.join(raw_dir, "*.md")):
        base = os.path.basename(rf)
        wf = os.path.join(wiki_dir, base)
        if os.path.exists(wf):
            pairs.append((rf, wf))
    if sample and sample < len(pairs):
        pairs = random.sample(pairs, sample)
    return sorted(pairs)


def read_file(path: str) -> str:
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except Exception:
        return None


def summarize(reports: list) -> dict:
    if not reports:
        return {
            "sources_evaluated": 0,
            "avg_coverage": 0,
            "avg_consistency": 0,
            "total_hallucinations": 0,
            "low_quality": []
        }
    coverages = [r["scores"].get("coverage", 0) for r in reports]
    consistencies = [r["scores"].get("consistency", 0) for r in reports]
    total_hall = sum(len(r.get("hallucinations", [])) for r in reports)
    return {
        "sources_evaluated": len(reports),
        "avg_coverage": round(sum(coverages) / len(coverages), 1) if coverages else 0,
        "avg_consistency": round(sum(consistencies) / len(consistencies), 1) if consistencies else 0,
        "total_hallucinations": total_hall,
        "low_quality": [
            r["source_file"] for r in reports
            if r["scores"].get("coverage", 10) < 5
        ]
    }


def run_repairs(project_dir: str, config_path: str, eval_result: dict,
                threshold: int = 6, dry_run: bool = False,
                verbose: bool = False) -> dict:
    """对评估结果中低质量的页面执行 auto-fix"""
    llm_config = load_llm_config(config_path)
    repair_log = []

    for report_dict in eval_result.get("reports", []):
        scores = report_dict.get("scores", {})
        if "error" in scores:
            continue

        source_file = report_dict.get("source_file", "")
        wiki_page = report_dict.get("wiki_page", "")
        halls = report_dict.get("hallucinations", [])

        if not should_repair(scores, halls, threshold):
            continue

        if verbose:
            cov = scores.get("coverage", "?")
            print(f"  [{source_file}] coverage={cov} halls={len(halls)} → 需要修复", flush=True)

        src_path = os.path.join(project_dir, source_file)
        wiki_path = os.path.join(project_dir, wiki_page)
        src_content = read_file(src_path)
        wiki_content = read_file(wiki_path)
        if not src_content or not wiki_content:
            if verbose:
                print(f"    ⚠ 跳过: 无法读取 source 或 wiki")
            continue

        report = JudgeReportItem.from_dict(report_dict)

        if dry_run:
            repair_log.append({
                "page": wiki_page,
                "reason": {
                    "coverage": scores.get("coverage"),
                    "hallucinations": len(halls),
                },
                "dry_run": True,
            })
            continue

        # 修复
        new_content = repair_page(src_content, wiki_content, report, llm_config)
        if new_content is None:
            if verbose:
                print(f"    ✗ 修复失败 (LLM error)")
            repair_log.append({
                "page": wiki_page,
                "status": "failed",
                "error": "LLM call failed",
            })
            continue

        # 写入
        write_result = write_repaired_page(project_dir, report, new_content)
        if not write_result["success"]:
            if verbose:
                print(f"    ✗ 写入失败: {write_result.get('error')}")
            repair_log.append({
                "page": wiki_page,
                "status": "failed",
                "error": write_result.get("error"),
            })
            continue

        # 验证
        verify_result = verify_repair(
            project_dir, src_path, report, new_content, llm_config, threshold
        )

        entry = {
            "page": wiki_page,
            "status": "repaired" if verify_result["success"] else "rejected",
            "before": verify_result.get("before"),
            "after": verify_result.get("after"),
            "rollback": verify_result.get("rollback", False),
        }
        repair_log.append(entry)

        if verbose:
            if verify_result["success"]:
                b = verify_result["before"]
                a = verify_result["after"]
                print(f"    ✓ coverage {b['coverage']}→{a['coverage']} "
                      f"halls {b['hallucinations']}→{a['hallucinations']}")
            else:
                print(f"    ✗ 验证失败, 已回退: {verify_result.get('note', '')}")

    repaired = [r for r in repair_log if r.get("status") == "repaired"]
    failed = [r for r in repair_log if r.get("status") == "failed"]
    rejected = [r for r in repair_log if r.get("status") == "rejected"]

    return {
        "summary": {
            "total_flagged": len(repair_log),
            "repaired": len(repaired),
            "failed": len(failed),
            "rejected": len(rejected),
            "dry_run": dry_run,
        },
        "repairs": repair_log,
    }


def _save_checkpoint(output_path: str, result: dict):
    """写 checkpoint，确保目录存在"""
    if not output_path:
        return
    os.makedirs(os.path.dirname(os.path.abspath(output_path)), exist_ok=True)
    tmp = output_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(result, f, ensure_ascii=False, indent=2)
    os.replace(tmp, output_path)


def run_llm_judge(project_dir: str, config_path: str, sample: int = None,
                  verbose: bool = False,
                  output_path: str = None) -> dict:
    """运行 LLM-as-Judge 评估管线
    如果指定 output_path, 每完成一个文件写 checkpoint。
    """
    llm_config = load_llm_config(config_path)
    pairs = find_source_wiki_pairs(project_dir, sample)
    reports = []

    for i, (src, wiki) in enumerate(pairs):
        src_content = read_file(src)
        wiki_content = read_file(wiki)
        if not src_content or not wiki_content:
            continue

        source_name = os.path.basename(src)
        wiki_rel = os.path.relpath(wiki, project_dir)

        if verbose:
            print(f"[{i+1}/{len(pairs)}] {source_name} ...", end=" ", flush=True)

        try:
            # 角色 A: 提取关键陈述
            claims_resp = extract_claims(src_content, source_name, llm_config)
            claims = parse_extracted_claims(claims_resp)
            claims_text = json.dumps(claims, ensure_ascii=False, indent=2)

            # 角色 B: 评估 wiki 覆盖度
            report = evaluate_wiki(claims_text, wiki_content, llm_config)
            report.source_file = os.path.relpath(src, project_dir)
            report.wiki_page = wiki_rel
            reports.append(report.to_dict())

            if verbose:
                cov = report.scores.get("coverage", "?")
                hall = len(report.hallucinations)
                print(f"coverage={cov} hallucinations={hall}")
        except Exception as e:
            if verbose:
                print(f"ERROR: {e}")
            reports.append(JudgeReportItem(
                source_file=os.path.relpath(src, project_dir),
                wiki_page=wiki_rel,
                scores={"error": str(e)}
            ).to_dict())

        # 每完成一个文件写 checkpoint
        if output_path:
            partial = {
                "project": os.path.basename(project_dir.rstrip("/")),
                "timestamp": str(datetime.now()),
                "config": {"model": llm_config.get("model"), "sample": sample},
                "progress": f"{len(reports)}/{len(pairs)}",
                "reports": reports,
                "summary": summarize(reports),
            }
            _save_checkpoint(output_path, partial)

    return {
        "project": os.path.basename(project_dir.rstrip("/")),
        "timestamp": str(datetime.now()),
        "config": {"model": llm_config.get("model"), "sample": sample},
        "reports": reports,
        "summary": summarize(reports)
    }


def main():
    parser = argparse.ArgumentParser(description="LLM-as-Judge Ingest 质量评估")
    parser.add_argument("--project", "-p", required=True, help="项目路径")
    parser.add_argument("--config", "-c", required=True, help="LLM 配置 JSON 路径")
    parser.add_argument("--sample", "-s", type=int, default=None,
                        help="随机抽样数量（不指定则评估全部）")
    parser.add_argument("--verbose", "-v", action="store_true", help="详细输出")
    parser.add_argument("--output", "-o", help="结果输出 JSON 路径")
    parser.add_argument("--auto-fix", action="store_true",
                        help="自动修复低质量页面（coverage < threshold 或含幻觉）")
    parser.add_argument("--threshold", type=int, default=6,
                        help="覆盖率阈值（默认 6）")
    parser.add_argument("--dry-run", action="store_true",
                        help="预览模式：显示会修复哪些页面，但不实际修改文件")
    args = parser.parse_args()

    result = run_llm_judge(args.project, args.config, args.sample, args.verbose,
                           output_path=args.output)

    # Phase 3: auto-fix
    if args.auto_fix:
        print("\n=== Auto-Fix Phase ===")
        fix_result = run_repairs(
            args.project, args.config, result,
            threshold=args.threshold, dry_run=args.dry_run,
            verbose=args.verbose,
        )
        print(json.dumps(fix_result["summary"], ensure_ascii=False, indent=2))
        result["auto_fix"] = fix_result

    if args.output:
        os.makedirs(os.path.dirname(os.path.abspath(args.output)), exist_ok=True)
        with open(args.output, "w", encoding="utf-8") as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        print(f"Results saved to {args.output}")


if __name__ == "__main__":
    main()
