# LLM Wiki CLI (Phase 3)

## 规划命令

| 命令 | 实现层 | 说明 |
|------|--------|------|
| `llm-wiki search` | Rust | 调用 `search_project` / HTTP API |
| `llm-wiki preprocess` | Rust | PDF/Office → 文本 |
| `llm-wiki reindex` | Rust + TS | 重建 LanceDB 向量 |
| `llm-wiki rescan` | Rust | 源目录重扫 |
| `llm-wiki ingest` | Node/TS | 包装 `upstream/src/lib/ingest.ts` |

## 目录

- `rust/` — clap CLI（待实现）
- `node/` — ingest orchestration（待实现）
