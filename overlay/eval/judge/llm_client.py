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


def _extract_balanced_json(text: str) -> Optional[str]:
    """Find the first '{' and return the balanced {...} object string.

    Tracks string literals and escapes so a '}' inside a string value
    (e.g. {"a":"}"}) does not end the object early.
    Returns None if no balanced object is found.
    """
    start = text.find("{")
    if start < 0:
        return None
    depth = 0
    in_string = False
    escape = False
    for i in range(start, len(text)):
        c = text[i]
        if escape:
            escape = False
            continue
        if c == "\\":
            escape = True
            continue
        if c == '"' and not in_string:
            in_string = True
            continue
        if c == '"' and in_string:
            in_string = False
            continue
        if in_string:
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return text[start : i + 1]
    return None


def _cleanup_if_else(text: str) -> str:
    """Replace Python-style inline if/else in JSON string values.

    Pattern: "valueA" if <cond> else "valueB" -> "valueB" (the else branch).
    Applied repeatedly until stable.
    """
    pattern = r'"[^"]*"\s+if\s+.+?\s+else\s+("(?:[^"\\]|\\.)*")'
    while True:
        cleaned = re.sub(pattern, r"\1", text)
        if cleaned == text:
            break
        text = cleaned
    return text


def parse_json_response(text: str) -> dict:
    """Parse JSON from LLM response, handling fences, trailing garbage, and Python if/else expressions."""
    text = text.strip()

    # 1. Direct parse
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass

    # 2. Balanced extraction (handles trailing quotes/garbage, prose wrapping, ``` fences)
    balanced = _extract_balanced_json(text)
    if balanced is not None:
        try:
            return json.loads(balanced)
        except json.JSONDecodeError:
            pass

        # 3. If/else cleanup on the balanced JSON text
        cleaned = _cleanup_if_else(balanced)
        if cleaned != balanced:
            try:
                return json.loads(cleaned)
            except json.JSONDecodeError:
                pass

    # 3b. If/else cleanup on the original text (fallback when balanced extraction failed)
    cleaned_raw = _cleanup_if_else(text)
    if cleaned_raw != text:
        try:
            return json.loads(cleaned_raw)
        except json.JSONDecodeError:
            pass

    # 4. Truncation recovery: append closing brackets
    for suffix in ['"}', '"]', '}', '"]}', '}']:
        try:
            return json.loads(text + suffix)
        except json.JSONDecodeError:
            continue

    # 5. Unrecoverable
    raise ValueError(f"Could not parse JSON from response: {text[:200]}")
