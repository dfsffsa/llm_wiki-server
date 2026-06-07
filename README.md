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

**全局入口：[文档指引.md](文档指引.md)**（按任务导航 · 四大模块）

| 模块 | 入口 |
|------|------|
| 项目结构与开发 | [代码结构总览](docs/代码结构总览.md) · [开发与测试](docs/开发与测试.md) |
| 日常运维 | [日常运维](docs/日常运维.md) · [新项目指引](docs/新项目指引.md) |
| 部署 | [部署指引](docs/部署指引.md) · [ECS 与 Tunnel](docs/部署-ECS与Tunnel.md) |
| 完整目录 | [docs/文档索引.md](docs/文档索引.md) |

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
