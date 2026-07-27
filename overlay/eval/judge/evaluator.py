import json
import re
from .llm_client import call_llm
from .models import CoverageClaim, Hallucination, JudgeReportItem

EVAL_SYSTEM_PROMPT = "你是一个知识库质量评估员。逐条检查 wiki 页面是否覆盖了素材中的关键信息。评分维度：1. 信息覆盖率（0-10） 2. 事实一致性（0-10）输出 JSON 格式。"

EVAL_USER_TEMPLATE = """素材关键陈述（不可修改）:
{claims_text}

Wiki 页面:
{wiki_content}

逐条检查上述关键陈述是否在 wiki 中有对应内容。同时检查 wiki 中是否有素材中无依据的多余信息。
输出 JSON: coverage_claims[], hallucinations[], scores"""


def _try_parse_json(text):
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass
    m = re.search(r"```(?:json)?\s*([\s\S]*?)```", text)
    if m:
        try:
            return json.loads(m.group(1))
        except json.JSONDecodeError:
            pass
    for suffix in ['"}', '"]', "}", '"}', '"]']:
        try:
            return json.loads(text + suffix)
        except json.JSONDecodeError:
            continue
    return None


def evaluate_wiki(claims_text, wiki_content, llm_config):
    prompt = EVAL_USER_TEMPLATE.format(claims_text=claims_text, wiki_content=wiki_content)
    resp = call_llm(prompt, llm_config, system=EVAL_SYSTEM_PROMPT)
    return parse_eval_response(resp, "", "")

def parse_eval_response(resp, source_file, wiki_page):
    import logging
    data = _try_parse_json(resp)
    if data is None:
        logging.warning(f"Failed to parse eval response:\n{resp[:300]}")
        return JudgeReportItem(source_file=source_file, wiki_page=wiki_page, scores={"error": "unparseable_response"})

    # CoverageClaim 合法字段
    CC_FIELDS = {"claim", "source_location", "wiki_coverage", "wiki_excerpt"}
    HALL_FIELDS = {"claim", "wiki_location", "severity", "judge_reasoning"}

    FIELD_ALIASES = {
        "location": "source_location", "status": "wiki_coverage",
        "coverage": "wiki_coverage", "covered": "wiki_coverage",
        "excerpt": "wiki_excerpt", "text": "wiki_excerpt",
        "wiki_text": "wiki_excerpt", "coverage_status": "wiki_coverage",
        "index": None, "claim_id": None, "note": None,
    }

    HALL_ALIASES = {
        "location": "wiki_location", "reason": "judge_reasoning",
        "explanation": "judge_reasoning", "reasoning": "judge_reasoning",
        "statement": "claim", "content": "claim", "hallucination": "claim",
        "description": "claim", "text": "claim",
    }

    def normalize_item(item, valid_fields, aliases):
        if not isinstance(item, dict):
            return {k: v for k, v in {"claim": str(item)}.items() if k in valid_fields}
        for alias, target in aliases.items():
            if alias in item:
                if target:
                    if target not in item:
                        item[target] = item.pop(alias)
                    else:
                        del item[alias]
        item.setdefault("source_location", "")
        item.setdefault("wiki_excerpt", "")
        item.setdefault("wiki_coverage", "partial")
        item.setdefault("claim", "untitled")
        return {k: v for k, v in item.items() if k in valid_fields}

    def normalize_hallucination(h):
        if not isinstance(h, dict):
            return {"claim": str(h), "severity": "minor", "wiki_location": "", "judge_reasoning": ""}
        for alias, target in HALL_ALIASES.items():
            if alias in h:
                if target:
                    if target not in h:
                        h[target] = h.pop(alias)
                    else:
                        del h[alias]
        h.setdefault("wiki_location", "")
        h.setdefault("judge_reasoning", "")
        h.setdefault("severity", "minor")
        h.setdefault("claim", "unknown")
        return {k: v for k, v in h.items() if k in HALL_FIELDS}

    raw_halls = data.get("hallucinations", [])
    if not isinstance(raw_halls, list):
        raw_halls = []
    raw_scores = data.get("scores", {})
    if not isinstance(raw_scores, dict):
        raw_scores = {}
    # 规范化 score 字段名
    SCORE_ALIASES = {"information_coverage": "coverage", "fact_consistency": "consistency",
                     "coverage_score": "coverage", "consistency_score": "consistency",
                     "info_coverage": "coverage"}
    for alias, target in SCORE_ALIASES.items():
        if alias in raw_scores and target not in raw_scores:
            raw_scores[target] = raw_scores.pop(alias)

    claims = [CoverageClaim(**normalize_item(c, CC_FIELDS, FIELD_ALIASES)) for c in data.get("coverage_claims", []) if isinstance(c, dict)]
    halls = [Hallucination(**normalize_hallucination(h)) for h in raw_halls]
    return JudgeReportItem(
        source_file=source_file, wiki_page=wiki_page,
        coverage_claims=claims, hallucinations=halls,
        scores=raw_scores)
