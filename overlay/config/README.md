# Overlay 配置样例

| 文件 | Git | 用途 |
|------|-----|------|
| `server.example.json` | ✅ 跟踪 | HTTP server：`projects[]`、`llmConfig`、`embeddingConfig` 样例 |
| `llm.example.json` | ✅ 跟踪 | CLI ingest / reindex 用 LLM 配置样例 |
| `*.local.json` | ❌ gitignore | 本地密钥副本（如 `server.minimax.local.json`） |

## 本地使用

```bash
# Server（HTTP Chat + 多项目）
cp overlay/config/server.example.json overlay/config/server.minimax.local.json
# 编辑 model、customEndpoint、apiKey 或 ${LLM_API_KEY} 占位符
export LLM_WIKI_CONFIG=overlay/config/server.minimax.local.json

# 或直接使用样例 + 环境变量
export LLM_API_KEY="..."
export LLM_WIKI_CONFIG=overlay/config/server.example.json

# CLI ingest
cp overlay/config/llm.example.json overlay/config/llm.json
export LLM_WIKI_CONFIG=overlay/config/llm.json
```

`server.example.json` 中的 `projects[].path` 请改为你本机的 wiki 项目绝对路径。
