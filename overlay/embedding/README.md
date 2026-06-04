# Embedding 探索

本目录存放 **overlay 定制** 的 embedding 实验，不修改 `upstream/`。

## 文件

| 文件 | 说明 |
|------|------|
| `EXPLORATION.md` | 上游 embedding 架构笔记（从 llm_wiki 迁移） |
| `test_embedding.example.py` | DashScope 测试模板（用环境变量，勿提交密钥） |

## 运行测试

```bash
export DASHSCOPE_API_KEY="your-key"
python overlay/embedding/test_embedding.example.py
```

## 向量库位置

仍在 Wiki 项目目录：`{project}/.llm-wiki/lancedb/`（upstream 约定）。
