import json
import os
import re
from typing import Optional

import requests


def load_llm_config(config_path: str) -> dict:
    with open(config_path) as f:
        cfg = json.load(f)
    llm = cfg.get("llmConfig", {})
    # 展开 ${ENV_VAR}
    for k, v in llm.items():
        if isinstance(v, str) and v.startswith("${") and v.endswith("}"):
            env_key = v[2:-1]
            llm[k] = os.environ.get(env_key, v)
    return llm


def call_llm(prompt: str, llm_config: dict, system: str = "") -> str:
    """调用 OpenAI 兼容接口。返回 response 文本。"""
    headers = {"Content-Type": "application/json"}
    if llm_config.get("apiKey"):
        headers["Authorization"] = f"Bearer {llm_config['apiKey']}"

    messages = []
    if system:
        messages.append({"role": "system", "content": system})
    messages.append({"role": "user", "content": prompt})

    payload = {
        "model": llm_config.get("model", "gpt-4"),
        "messages": messages,
        "temperature": 0.1,
        "max_tokens": llm_config.get("max_tokens", 8192),
    }

    endpoint = llm_config.get("endpoint", "").rstrip("/")
    if not endpoint:
        endpoint = "https://api.openai.com/v1"
    if endpoint.endswith("/chat/completions"):
        url = endpoint
    else:
        url = f"{endpoint}/chat/completions"

    resp = requests.post(url, headers=headers, json=payload, timeout=300)
    resp.raise_for_status()
    return resp.json()["choices"][0]["message"]["content"]


def parse_json_response(text: str) -> dict:
    """Parse JSON from LLM response, handling markdown code block fences."""
    text = text.strip()
    # Try direct parsing first
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass
    # Try extracting from ```json ... ``` block
    m = re.search(r"```(?:json)?\s*\n?(.*?)```", text, re.DOTALL)
    if m:
        try:
            return json.loads(m.group(1).strip())
        except json.JSONDecodeError:
            pass
    raise ValueError(f"Could not parse JSON from response: {text[:200]}")
