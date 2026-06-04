# Embedding 探索

本目录存放 **overlay 定制** 的 embedding 实验，不修改 `upstream/`。

## 文件

| 文件 | 说明 |
|------|------|
| `EXPLORATION.md` | Embedding 配置、LanceDB 路径、混合搜索机制 |
| `test_embedding.example.py` | DashScope 测试模板（用环境变量） |

本地曾有的 `test_embedding.py`（含明文 API Key）**未迁入本仓库**；请使用 `test_embedding.example.py` 并设置 `DASHSCOPE_API_KEY`。若旧密钥曾写入本地文件，建议在云平台轮换。

## 运行测试

```bash
export DASHSCOPE_API_KEY="your-key"
python overlay/embedding/test_embedding.example.py
```

## 向量库位置

仍在 Wiki 项目目录：`{project}/.llm-wiki/lancedb/`（upstream 约定）。
