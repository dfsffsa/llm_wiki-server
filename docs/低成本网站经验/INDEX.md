# 低成本网站经验归档

> 归档时间：2026-04-15 ~ 2026-04-17  
> 来源：本地分析目录 `~/hn_low_cost_site_analysis_2026-04-15/`  
> 关联：HN 上 [Steve Hanov](https://github.com/smhanov) 一系列低成本独立站的工程实践

本目录是分析 Steve Hanov（smhanov）几个网站的低成本搭建经验、做认证内核选型和前端分层策略时的工作笔记。  
与 `llm_wiki-server` 项目本身**无直接代码依赖**，纯笔记归档，方便在多台开发机之间共享。

---

## 阅读顺序建议

| 起点 | 接下来 |
|------|--------|
| 想快速了解作者怎么用极少代码做网站 | [README.md](./README.md)（HN 帖子分析） → [PROJECTS_SUMMARY_CN.md](./PROJECTS_SUMMARY_CN.md)（laconic / llmhub / auth 项目总结） |
| 关心**认证**怎么选型 | [AUTH_ANALYSIS.md](./AUTH_ANALYSIS.md) → [SMHANOV_AUTH_CODE_ANALYSIS.md](./SMHANOV_AUTH_CODE_ANALYSIS.md) → [AUTH_DECISION.md](./AUTH_DECISION.md) |
| 要让 LLM/Codex 帮忙实现认证 | [AUTH_SYSTEM_DESIGN_FOR_LLM.md](./AUTH_SYSTEM_DESIGN_FOR_LLM.md) → [AUTH_IMPLEMENTATION_PROMPT.md](./AUTH_IMPLEMENTATION_PROMPT.md) |
| 关心**前端分层**策略 | [FRONTEND_ANALYSIS.md](./FRONTEND_ANALYSIS.md) → [FRONTEND_LAYERING_STRATEGY_FOR_CODEX.md](./FRONTEND_LAYERING_STRATEGY_FOR_CODEX.md) → [FRONTEND_SYSTEM_PROMPT_SHORT.md](./FRONTEND_SYSTEM_PROMPT_SHORT.md) |
| 查原始资料链接 | [SOURCES.md](./SOURCES.md) |

---

## 文件清单

| 文件 | 主题 |
|------|------|
| [README.md](./README.md) | HN 帖子分析：如何用低成本方式搭建网站（入口总览） |
| [SOURCES.md](./SOURCES.md) | 信息源链接清单（HN、smhanov 各仓库地址） |
| [PROJECTS_SUMMARY_CN.md](./PROJECTS_SUMMARY_CN.md) | Steve Hanov 相关项目（laconic / llmhub / auth）综合总结 |
| [AUTH_ANALYSIS.md](./AUTH_ANALYSIS.md) | 作者网站登录实现调研：30 行 Go 背后真正的内核 |
| [SMHANOV_AUTH_CODE_ANALYSIS.md](./SMHANOV_AUTH_CODE_ANALYSIS.md) | `smhanov/auth` 库源码逐层分析 |
| [AUTH_DECISION.md](./AUTH_DECISION.md) | 决策记录：采用 `smhanov/auth` 作为认证内核，停止自研 `boringauth` |
| [AUTH_SYSTEM_DESIGN_FOR_LLM.md](./AUTH_SYSTEM_DESIGN_FOR_LLM.md) | 可复用认证内核设计方案（喂给 LLM 的完整设计文档） |
| [AUTH_IMPLEMENTATION_PROMPT.md](./AUTH_IMPLEMENTATION_PROMPT.md) | Auth 内核实现 Prompt（给 Codex 的实现指令） |
| [FRONTEND_ANALYSIS.md](./FRONTEND_ANALYSIS.md) | 作者前端实现方案分析 |
| [FRONTEND_LAYERING_STRATEGY_FOR_CODEX.md](./FRONTEND_LAYERING_STRATEGY_FOR_CODEX.md) | 前端分层策略（独立开发者视角） |
| [FRONTEND_SYSTEM_PROMPT_SHORT.md](./FRONTEND_SYSTEM_PROMPT_SHORT.md) | 前端 System Prompt 精简版 |
| [SMHANOV_AUTH_README.md](./SMHANOV_AUTH_README.md) | 上游 [smhanov/auth](https://github.com/smhanov/auth) 库 README 归档副本（MIT） |
| [SMHANOV_AUTH_DEMO_README.md](./SMHANOV_AUTH_DEMO_README.md) | 本地 demo README（演示如何把 `smhanov/auth` 包一层 `/auth/*` JSON adapter） |

---

## 与本项目（llm_wiki-server）的关系

**当前无直接代码依赖。** 这些笔记单独存档在 `docs/低成本网站经验/`，因为：

1. 内容是低成本独立站的工程经验，将来可能被本项目复用（例如给 `llm-wiki-server` 加用户系统时直接参考 `AUTH_DECISION.md` 的结论）
2. 多台开发机之间共享这套思考记录，比放在散落的本地目录更稳妥
3. 不引入开源代码副本（避免重复分发 `smhanov/auth` 源码），所有引用都指向上游 GitHub

如未来真的把 `smhanov/auth` 接入本项目，再单独建 `overlay/auth/` 目录，并在 [代码结构总览](../代码结构总览.md) 中登记。

---

## 历史路径说明

原本地工作目录为 `~/hn_low_cost_site_analysis_2026-04-15/`，包含：

- 11 个分析笔记 + 2 个第三方 README（已迁入本目录，共 13 个 .md）
- `smhanov-auth/`：开源库 [smhanov/auth](https://github.com/smhanov/auth) 的本地副本（**源码未归档**，需要时直接 `git clone` 即可；其 README 已作为 `SMHANOV_AUTH_README.md` 归档）
- `smhanov-auth-demo/`：本地 demo（含编译产物与 SQLite 数据，**源码未归档**；demo 的 README 已作为 `SMHANOV_AUTH_DEMO_README.md` 归档作为接入示例参考）

文档中曾出现的 `/root/hn_low_cost_site_analysis_2026-04-15/...` 绝对路径已在归档时改写为相对表述或上游链接。
