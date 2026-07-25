import json
from .llm_client import call_llm

EXTRACT_SYSTEM_PROMPT = """
你是一个文档分析专家。你的任务是从给定的原始材料中提取所有关键陈述。

要求：
1. 提取材料中所有重要的**事实性信息**（数值、定义、因果关系、操作步骤、警示）
2. 每条陈述必须能直接引用原文中的具体位置
3. 不遗漏关键信息，也不过度拆分（同一自然段内相关的信息归为一条）
4. 用简洁的陈述句表达
5. 输出 JSON 数组格式
"""

EXTRACT_USER_TEMPLATE = """
请从以下源文件中提取所有关键陈述：

文件: {file_name}

--- 原文 ---
{source_content}
--- 原文结束 ---

输出 JSON 数组，每项包含：
  - claim: 关键陈述
  - location: 在原文中的位置（段落号或描述）
"""


def extract_claims(source_content: str, file_name: str, llm_config: dict) -> str:
    """调用 LLM 提取关键陈述，返回原始 LLM 响应文本。"""
    prompt = EXTRACT_USER_TEMPLATE.format(file_name=file_name, source_content=source_content)
    return call_llm(prompt, llm_config, system=EXTRACT_SYSTEM_PROMPT)


def parse_extracted_claims(llm_response: str) -> list:
    """从 LLM 响应中解析出 {'claim': ..., 'location': ...} 列表。"""
    try:
        return json.loads(llm_response)
    except json.JSONDecodeError:
        # 尝试从代码块中提取
        import re
        m = re.search(r"```(?:json)?\s*([\s\S]*?)```", llm_response)
        if m:
            return json.loads(m.group(1))
        raise
