# LLM Wiki Server

在 [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki) 之上叠加 **HTTP 服务 + CLI + Web 部署** 的集成仓库。

- **upstream/** — 官方源码（git submodule，只读）
- **overlay/** — 我们的定制实现
- **docs/** — 架构与改造方案

## 快速开始

```bash
git clone --recurse-submodules git@github.com:dfsffsa/llm_wiki-server.git
cd llm_wiki-server

# 安装 upstream 前端依赖（构建 UI 时需要）
npm install --prefix upstream

# 升级 upstream（按 tag）
./scripts/sync-upstream.sh v0.4.20
```

## 文档

| 文档 | 说明 |
|------|------|
| [docs/README.md](docs/README.md) | 文档索引 |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 完整架构、分阶段计划、耦合分析 |
| [docs/feasibility-assessment.md](docs/feasibility-assessment.md) | 服务化可行性评估 |
| [docs/upstream-architecture-zh.md](docs/upstream-architecture-zh.md) | 上游桌面版架构说明（中文） |
| [README-OVERLAY.md](README-OVERLAY.md) | Overlay 开发指南、Git 工作流 |

原本地 `llm_wiki/` 目录中的分析已全部迁入本仓库，**可删除**；请勿使用或推送 `dfsffsa/llm_wiki`。

## 环境变量（规划中的 headless 服务）

| 变量 | 说明 |
|------|------|
| `LLM_WIKI_PROJECT` | Wiki 项目根目录（含 `wiki/`、`raw/`） |
| `LLM_WIKI_API_TOKEN` | API 鉴权 token |
| `LLM_WIKI_BIND` | 监听地址，默认 `127.0.0.1:8080` |
| `LLM_WIKI_CONFIG` | 服务端配置文件路径（embedding、LLM 等） |

## 许可证

基于上游 **GPL v3.0**。见 [upstream/LICENSE](upstream/LICENSE)。
