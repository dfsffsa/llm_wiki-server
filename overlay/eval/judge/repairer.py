"""
定向修复 wiki 页面中 LLM Judge 发现的问题 (Phase 3).

流程:
  1. should_repair() → 判定页面是否需要修复
  2. repair_page() → 调用 LLM 只修改问题区域
  3. write_repaired_page() → 备份原页面 → 写入修复后内容
  4. verify_repair() → re-evaluate 确认改进, 否则回退 + 标记人工

用户确认的设计决策:
  - threshold=6 (coverage < 6 触发修复)
  - 幻觉添加标注而非删除: "> ⚠️ 此信息在原始素材中未找到依据"
  - 修复失败直接标记人工 (不重试)
"""

import json
import os
import shutil
from typing import Optional

from .llm_client import call_llm
from .models import JudgeReportItem, CoverageClaim, Hallucination
from .evaluator import evaluate_wiki, parse_eval_response
from .extractor import extract_claims, parse_extracted_claims

# ── Prompt 模板 ──────────────────────────────────────────────

REPAIR_SYSTEM_PROMPT = """你是一个 wiki 页面修复专家。你的任务是：根据评估反馈，修复 wiki 页面中存在的问题。

规则：
1. 只改动评估反馈中指出的问题区域，其他内容保持完全不变
2. 如果反馈指出"某信息缺失"，在 wiki 页面中补充该信息（基于 source 原文）
3. 如果反馈指出"某信息是幻觉"，在对应内容后添加标注：
   > ⚠️ 此信息在原始素材中未找到依据
4. 保持页面的 frontmatter 不变
5. 保持原有的写作风格和格式
6. 输出：只输出修复后的完整 wiki 页面内容，不要包含任何额外的说明或代码块标记"""

REPAIR_USER_TEMPLATE = """## Source 原文
{source_content}

## 当前 Wiki 页面
{wiki_content}

## 评估反馈

### 缺失的关键信息（coverage < threshold）
{coverage_gaps}

### 幻觉清单
{hallucinations_text}

请根据评估反馈修复上述 wiki 页面。只改动有问题的区域。"""


# ── 判定逻辑 ──────────────────────────────────────────────


def should_repair(scores: dict, hallucinations: list, threshold: int = 6) -> bool:
    """判定页面是否需要修复: coverage < threshold 或存在幻觉"""
    coverage = scores.get("coverage", 10)
    return coverage < threshold or len(hallucinations) > 0


def format_content_gaps(claims: list) -> str:
    """格式化 coverage 不足的要点"""
    gaps = []
    for c in claims:
        if c.wiki_coverage in ("missing", "partial"):
            gaps.append(f"- {c.claim}  (覆盖状态: {c.wiki_coverage})")
    return "\n".join(gaps) if gaps else "(无)"


def format_hallucinations(halls: list) -> str:
    """格式化幻觉清单"""
    items = []
    for h in halls:
        items.append(f"- {h.claim}  (位置: {h.wiki_location}, 严重度: {h.severity})")
    return "\n".join(items) if items else "(无)"


def format_eval_report_for_prompt(report: JudgeReportItem) -> tuple:
    """从 JudgeReportItem 提取并格式化缺失要点和幻觉"""
    gaps = format_content_gaps(report.coverage_claims)
    halls = format_hallucinations(report.hallucinations)
    return gaps, halls


# ── 核心修复逻辑 ──────────────────────────────────────────


def repair_page(source_content: str, wiki_content: str,
                report: JudgeReportItem, llm_config: dict) -> Optional[str]:
    """调用 LLM 定向修复 wiki 页面, 返回修复后的内容 (或 None)"""
    gaps, halls = format_eval_report_for_prompt(report)

    prompt = REPAIR_USER_TEMPLATE.format(
        source_content=source_content,
        wiki_content=wiki_content,
        coverage_gaps=gaps,
        hallucinations_text=halls,
    )

    try:
        new_content = call_llm(prompt, llm_config, system=REPAIR_SYSTEM_PROMPT)
        # 清理 LLM 可能添加的 markdown 代码块包裹
        new_content = _strip_code_fence(new_content)
        return new_content.strip()
    except Exception as e:
        import logging
        logging.warning(f"repair_page LLM call failed: {e}")
        return None


def _strip_code_fence(text: str) -> str:
    """去掉 LLM 可能输出的 ```markdown ... ``` 等代码块包裹"""
    text = text.strip()
    for prefix in ("```markdown", "```md", "```"):
        if text.startswith(prefix):
            # 去掉开头 ``` 行
            first_newline = text.find("\n")
            if first_newline != -1:
                text = text[first_newline + 1:]
            # 去掉结尾的 ```
            if text.endswith("```"):
                text = text[:-3].rstrip()
            break
    return text.strip()


# ── 文件操作 ──────────────────────────────────────────────


def write_repaired_page(project_dir: str, report: JudgeReportItem,
                        new_content: str) -> dict:
    """
    写入修复后的页面。
    返回 dict: {page, backup, success, error?}
    """
    wiki_path = os.path.normpath(os.path.join(project_dir, report.wiki_page))
    if not wiki_path.startswith(os.path.normpath(project_dir)):
        return {"success": False, "error": "path traversal detected"}

    # 确认原文件存在
    if not os.path.isfile(wiki_path):
        return {"success": False, "error": f"wiki page not found: {wiki_path}"}

    # 备份
    backup_dir = os.path.join(project_dir, "fix_backups")
    os.makedirs(backup_dir, exist_ok=True)
    backup_key = report.wiki_page.replace("/", "_")
    backup_path = os.path.join(backup_dir, f"{backup_key}.bak")

    if not os.path.exists(backup_path):
        shutil.copy2(wiki_path, backup_path)

    # 写入新内容
    try:
        with open(wiki_path, "w", encoding="utf-8") as f:
            f.write(new_content)
    except OSError as e:
        return {"success": False, "error": str(e)}

    return {
        "page": report.wiki_page,
        "backup": backup_path,
        "success": True,
    }


# ── 修复后验证 ──────────────────────────────────────────


def _reevaluate_page(source_content: str, wiki_content: str,
                     llm_config: dict) -> Optional[JudgeReportItem]:
    """re-evaluate: 从 source 重新提取并评估 wiki 页面"""
    try:
        claims_resp = extract_claims(source_content, "", llm_config)
        claims = parse_extracted_claims(claims_resp)
        claims_text = json.dumps(claims, ensure_ascii=False, indent=2)
        return evaluate_wiki(claims_text, wiki_content, llm_config)
    except Exception as e:
        import logging
        logging.warning(f"re-evaluate failed: {e}")
        return None


def verify_repair(project_dir: str, src_path: str, report: JudgeReportItem,
                  new_content: str, llm_config: dict,
                  threshold: int = 6) -> dict:
    """
    修复后验证：re-evaluate → 确认 coverage ↑, hallucinations ↓
    如果验证失败，自动回退并标记人工。

    返回 dict: {success, verdict, before_scores, after_scores, rollback?}
    """
    src_content = _read_file(src_path)
    if not src_content:
        return {"success": False, "error": "cannot read source for re-evaluate"}

    # 原页面内容已备份，从 report 可知修复前的 scores
    before = {
        "coverage": report.scores.get("coverage", 0),
        "consistency": report.scores.get("consistency", 0),
        "hallucinations": len(report.hallucinations),
    }

    # re-evaluate 修复后的页面
    new_report = _reevaluate_page(src_content, new_content, llm_config)
    if new_report is None:
        return {"success": False, "error": "re-evaluate failed"}

    after = {
        "coverage": new_report.scores.get("coverage", 0),
        "consistency": new_report.scores.get("consistency", 0),
        "hallucinations": len(new_report.hallucinations),
    }

    # 判定：coverage 未下降 且 hallucinations 未增加
    coverage_ok = after["coverage"] >= before["coverage"]
    halls_ok = after["hallucinations"] <= before["hallucinations"]
    passed = coverage_ok and halls_ok

    result = {
        "success": passed,
        "verdict": "passed" if passed else "failed",
        "before": before,
        "after": after,
    }

    if not passed:
        # 回退到备份
        wiki_path = os.path.normpath(os.path.join(project_dir, report.wiki_page))
        backup_dir = os.path.join(project_dir, "fix_backups")
        backup_key = report.wiki_page.replace("/", "_")
        backup_path = os.path.join(backup_dir, f"{backup_key}.bak")
        if os.path.exists(backup_path):
            shutil.copy2(backup_path, wiki_path)
            result["rollback"] = True
            result["note"] = "repair rejected by verification, rolled back to backup; mark for manual review"

    return result


def _read_file(path: str) -> Optional[str]:
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except Exception:
        return None
