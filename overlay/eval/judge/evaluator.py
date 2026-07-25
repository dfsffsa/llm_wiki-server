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

def evaluate_wiki(claims_text, wiki_content, llm_config):
    prompt = EVAL_USER_TEMPLATE.format(claims_text=claims_text, wiki_content=wiki_content)
    resp = call_llm(prompt, llm_config, system=EVAL_SYSTEM_PROMPT)
    return parse_eval_response(resp, "", "")

def parse_eval_response(resp, source_file, wiki_page):
    try:
        data = json.loads(resp)
    except json.JSONDecodeError:
        m = re.search(r"```(?:json)?\s*([\s\S]*?)```", resp)
        if m:
            data = json.loads(m.group(1))
        else:
            raise
    claims = [CoverageClaim(**c) for c in data.get("coverage_claims", [])]
    halls = [Hallucination(**h) for h in data.get("hallucinations", [])]
    return JudgeReportItem(
        source_file=source_file, wiki_page=wiki_page,
        coverage_claims=claims, hallucinations=halls,
        scores=data.get("scores", {}))
