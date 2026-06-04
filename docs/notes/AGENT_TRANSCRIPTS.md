# Agent 会话记录说明

## 存放位置

Cursor 将 Agent 会话写在**项目外**的元数据目录，例如：

```text
~/.cursor/projects/<workspace-slug>/agent-transcripts/
```

本仓库对应 slug 曾为 `home-ab-overseas-github-llm-wiki`（打开 `llm_wiki` 工作区时）。  
改用 `llm_wiki-server` 后，新会话会出现在新的 slug 目录下。

## 本仓库策略

| 做法 | 说明 |
|------|------|
| ✅ 提交 | `docs/notes/agent-session-*.md` 等人读摘要 |
| ❌ 不提交 | 原始 `*.jsonl` 转录文件 |
| ❌ 不提交 | 整个 `.cursor/` 目录 |

## 备份原始转录（可选）

```bash
# 示例：归档到本机，不进入 git
mkdir -p ~/archive/llm-wiki-cursor-sessions
cp -a ~/.cursor/projects/home-ab-overseas-github-llm-wiki/agent-transcripts \
  ~/archive/llm-wiki-cursor-sessions/
```

## 已有摘要

- [agent-session-2026-06-04.md](./agent-session-2026-06-04.md) — 本次改造规划会话
