import json
import os
import re
import time
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


def call_llm(prompt: str, llm_config: dict, system: str = "",
             max_retries: int = 3) -> str:
    """调用 OpenAI 兼容接口。返回 response 文本。

    遇 429 Rate Limit 自动重试，指数退避：20s, 40s, 80s。
    """
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

    for attempt in range(max_retries):
        try:
            resp = requests.post(url, headers=headers, json=payload, timeout=300)
            resp.raise_for_status()
            return resp.json()["choices"][0]["message"]["content"]
        except requests.exceptions.HTTPError as e:
            if e.response.status_code == 429 and attempt < max_retries - 1:
                wait = 2 ** (attempt + 1) * 10  # 20s, 40s, 80s
                print(f"  [429 限流] 等待 {wait}s 后重试 ({attempt+1}/{max_retries})",
                      flush=True)
                time.sleep(wait)
                continue
            raise


def parse_json_response(text: str) -> dict:
    """Parse JSON from LLM response, handling markdown code block fences and truncation."""
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
    # Try truncation recovery: append closing brackets
    for suffix in ['"}', '"]', '}', '"]}', '}']:
        try:
            return json.loads(text + suffix)
        except json.JSONDecodeError:
            continue
    raise ValueError(f"Could not parse JSON from response: {text[:200]}")
