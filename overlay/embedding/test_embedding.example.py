"""
测试 embedding API（模板 — 勿在此文件写入真实 API Key）

用法:
  export DASHSCOPE_API_KEY="sk-..."
  python overlay/embedding/test_embedding.example.py
"""

import os
import sys

from openai import OpenAI

api_key = os.environ.get("DASHSCOPE_API_KEY", "").strip()
if not api_key:
    print("error: set DASHSCOPE_API_KEY environment variable", file=sys.stderr)
    sys.exit(1)

client = OpenAI(
    api_key=api_key,
    base_url=os.environ.get(
        "DASHSCOPE_BASE_URL",
        "https://dashscope.aliyuncs.com/compatible-mode/v1",
    ),
)

texts = ["Hello, world!", "你好，世界！", "LLM Wiki 是一个好用的工具"]
model = os.environ.get("EMBEDDING_MODEL", "text-embedding-v4")
dimensions = int(os.environ.get("EMBEDDING_DIMENSIONS", "2048"))

try:
    response = client.embeddings.create(
        model=model,
        input=texts,
        dimensions=dimensions,
    )
    print("API 调用成功!")
    print(f"model={model} dimensions={dimensions} count={len(response.data)}")
    for i, item in enumerate(response.data):
        print(f"\n文本 {i + 1}: {texts[i][:30]}...")
        print(f"  Embedding 长度: {len(item.embedding)}")
        print(f"  前5个维度: {item.embedding[:5]}")
except Exception as e:
    print(f"API 调用失败: {e}", file=sys.stderr)
    sys.exit(1)
