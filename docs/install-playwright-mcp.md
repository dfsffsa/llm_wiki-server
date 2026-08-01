# 安装 Playwright MCP 服务

## 目标

安装 Playwright MCP(浏览器自动化工具),让 Claude Code 可以操控浏览器访问网页、截图、交互,用于前端设计参考和 UI 验证。

## 步骤

### 1. 安装 Playwright MCP 服务

运行以下命令:

```bash
claude mcp add playwright npx @playwright/mcp@latest
```

这会把 Playwright MCP 加到 `~/.claude.json` 的 `mcpServers` 配置中。

### 2. 安装 Playwright 浏览器

运行:

```bash
npx playwright install chromium
```

这一步下载 Chromium 浏览器(约 300MB),Playwright MCP 需要它来操控浏览器。

### 3. 验证安装

重启 Claude Code 后,应该可以列出 MCP 工具并看到 playwright 相关的工具(如 `browser_navigate`、`browser_snapshot`、`browser_click` 等)。

可以通过以下命令测试:

```bash
# 测试 playwright 是否能启动
npx playwright install --list 2>&1 | head -5

# 或者直接通过 MCP 协议测试
# (需要重启 claude 会话后,尝试调用 playwright 工具浏览网页)
```

### 4. 验证后

安装成功后,你可以让 AI 访问 https://onyx.app/ 查看 web 应用的 UI 布局和交互设计,作为你自托管知识库问答系统前端设计的参考。

## 已知信息

- 当前配置文件: `/home/li/.claude/settings.opencode.json`
- MCP 配置会写到 `/home/li/.claude.json`
- 项目目录: `/home/li/overseas-github/llm_wiki-server`
- Playwright MCP 文档: https://playwright.dev/docs/getting-started-mcp

## 依赖

- Node.js 18+ (已满足,当前版本 v25.2.1)
- npx (已满足)
- 需要网络连接下载 Chromium 浏览器
