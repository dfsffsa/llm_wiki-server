# Agent 会话摘要（2026-06-04）

> 从 Cursor 会话 `0face610-00b7-4140-ad35-a0ea56ef56aa` 提炼。  
> 原始记录位于本机（**不纳入 git**）：  
> `~/.cursor/projects/home-ab-overseas-github-llm-wiki/agent-transcripts/`

## 会话主题

1. 评估 llm_wiki 项目概况  
2. 可行性：桌面 → HTTP 服务 + CLI + overlay 叠上游  
3. 环境无法读本地代码时的排查与恢复  
4. 基于代码的详细评估（后通过 GitHub / 本地读取完成）  
5. 创建集成仓库 `llm_wiki-server`（submodule + overlay）  
6. 将 `llm_wiki/` 本地分析迁入 `llm_wiki-server`，可删除旧目录  
7. 明确 **不要** 使用或推送 `dfsffsa/llm_wiki`

## 已确认决策

| 决策 | 说明 |
|------|------|
| 集成仓库 | `dfsffsa/llm_wiki-server`，upstream = `nashsu/llm_wiki` submodule |
| upstream 版本 | 初始 pin `v0.4.16`（与本地基线一致） |
| Git 策略 | upstream/ 零定制 commit；overlay/ 全定制 |
| 职责划分 | Web 浏览；CLI 入库/解析/索引；HTTP 扩展 19828 |
| ingest | 主要在 TS `ingest.ts`，CLI 需 headless 包装 |
| embedding 定制 | 放 `overlay/embedding/`，不 fork upstream |
| 密钥 | `test_embedding.py` 不提交；用 example + 环境变量 |

## 关键结论（代码审计后）

- 已有 API：`19828`，含 search/graph/files/rescan；`chat` 为 501  
- `api_server` 绑 `AppHandle` + `app_data_dir`，headless 需在 overlay 注入配置  
- 前端：`src/commands/` 2 文件 + 40+ 处 import；11 处直接 `invoke`  
- 分阶段：Phase 0 ✅ → Phase 1 server → Phase 2 Web → Phase 3 CLI  

详见 [feasibility-assessment.md](../feasibility-assessment.md)、[ARCHITECTURE.md](../ARCHITECTURE.md)。

## 产出物（已入库）

| 产出 | 路径 |
|------|------|
| 集成仓库 | https://github.com/dfsffsa/llm_wiki-server |
| 架构方案 | `docs/ARCHITECTURE.md` |
| 可行性评估 | `docs/feasibility-assessment.md` |
| 上游架构中文说明 | `docs/upstream-architecture-zh.md` |
| Embedding 探索 | `overlay/embedding/EXPLORATION.md` |
| overlay 骨架 | `overlay/server`, `cli`, `web`, `docker/` |

## 未决问题 / 后续

- [ ] Web 是否严格只读？（影响 Phase 2 范围）  
- [ ] 是否 bump upstream 至 v0.4.20 并跑 `sync-upstream.sh`  
- [ ] Phase 1：`overlay/server` 实现 headless HTTP  
- [ ] DashScope embedding 与 LanceDB 维度对齐验证  

## 为何不把原始 JSONL 放进 git

| 原因 | 说明 |
|------|------|
| 体积与噪声 | 主会话 ~110KB，大量 tool_use 重复 |
| 路径绑定 | 记录的是 `llm_wiki` 工作区，非 `llm_wiki-server` |
| 隐私 | 可能含密钥、绝对路径、环境细节 |
| 可维护性 | 摘要文档更适合团队与后续 Agent |

若需保留完整记录，请在本机备份 `agent-transcripts/` 目录，或定期导出为本地归档（不 push）。
