# Lite 问答页（静态）

面向非专业人士的极简 UI：选知识库主题 → 对话 → 历史记录。

- **URL：** `http://127.0.0.1:8080/lite/`
- **完整版：** `http://127.0.0.1:8080/`

## 功能概览（2026-06-06）

| 能力 | 说明 |
|------|------|
| 多项目选主题 | 首页卡片（育儿 / 职场等），数据来自 `GET /api/v1/projects` + `projects.meta.json` |
| RAG 对话 | 先 `search` 检索 wiki，再 `POST .../chat` SSE 流式回答 |
| 历史侧栏 | 左侧会话列表，按项目隔离，存 `localStorage` |
| 流式状态 | 「正在检索资料…」→「正在生成回答…」，回复结束后自动消失 |
| Markdown 渲染 | 助手回复用 **marked + DOMPurify**（标题、列表、代码块、表格、链接） |
| 超时保护 | 单次问答 120 秒无响应则提示「回复超时」并恢复输入 |

用户消息仍为纯文本；错误信息也为纯文本。

## 构建

随 HTTP UI 一起复制到 `upstream/dist/lite/`（含 Markdown 依赖 vendor）：

```bash
VITE_BACKEND=http VITE_API_TOKEN=e2e-test-token ./scripts/build-web.sh
```

`build-web.sh` 会从 `upstream/node_modules` 复制 `marked` / `DOMPurify` 到 `overlay/static/lite/vendor/`，再整体复制到 `dist/lite/`。

或仅复制 lite（不重建 React，需已存在 `vendor/`）：

```bash
mkdir -p upstream/dist/lite
cp -r overlay/static/lite/. upstream/dist/lite/
cat > upstream/dist/lite/config.js <<'EOF'
window.LLM_WIKI_LITE_CONFIG = { apiBase: "", apiToken: "e2e-test-token" };
EOF
```

修改 `app.js` / `markdown.js` / CSS 后至少执行上述复制，浏览器 **Ctrl+Shift+R** 强刷。

## Server 多项目

在 `LLM_WIKI_CONFIG` JSON 中配置 `projects`，或使用环境变量：

```bash
export LLM_WIKI_PROJECTS=~/overseas-github/llm_wiki_projects/ParentingBooks,~/overseas-github/llm_wiki_projects/CivilCareer
```

`GET /api/v1/projects` 应返回多个项目；Lite 页用 `projects.meta.json` 显示中文名、emoji 与主题色。

## 文件

| 文件 | 说明 |
|------|------|
| `index.html` | 单页：选主题 + 对话布局 |
| `app.css` | 方案 C 双主题 + 消息气泡 + Markdown 样式 |
| `app.js` | API、RAG、SSE、localStorage、流式 UI（ES module） |
| `markdown.js` | Markdown 解析与 XSS 过滤 |
| `vendor/marked.esm.js` | [marked](https://marked.js.org/) 解析器 |
| `vendor/purify.es.js` | [DOMPurify](https://github.com/cure53/DOMPurify)（须为 `.js` 扩展名，见排错） |
| `projects.meta.json` | 卡片文案、建议问题、theme |
| `config.js` | 构建生成，含 `apiToken`（勿提交真实密钥到 Git） |
| `config.example.js` | 本地开发示例 |

## 界面说明

### 首页

- 展示已注册 wiki 项目卡片；点击 Enter 对话视图。
- 若 API 失败，顶部 banner 显示「无法连接服务：…」。
- 若 `projects` 为空，banner 提示配置服务端。

### 对话页

- **左侧：** 历史会话（新建、切换）。
- **中间：** 用户气泡右对齐，助手气泡左对齐。
- **流式期间：** 助手气泡下方显示跳动圆点 + 状态文案；输入框禁用，placeholder 为「正在回复中，请稍候…」。
- **底部：** 建议问题芯片 + 输入框。

### Markdown 支持范围

Lite 版 intentionally 轻量，**不包含**完整 React UI 的以下能力：

- LaTeX / KaTeX 公式
- Mermaid 图表
- `[[wikilink]]` 点击跳转 wiki 页

若需上述能力，请使用完整版 `http://127.0.0.1:8080/`。

## 排错

| 现象 | 原因 / 处理 |
|------|-------------|
| **首页项目卡片空白** | 多为 JS 模块加载失败。打开 F12 → Console 查看报错。常见：`purify.es.mjs` MIME 错误 — 应使用 `purify.es.js`（已修复）；或重建 server 使 `.mjs` 返回 `application/javascript` |
| 顶部「无法连接服务」 | `config.js` 中 `apiToken` 与 `LLM_WIKI_API_TOKEN` 不一致，或 server 未启动 |
| 「暂无可用知识库」 | server 未配置 `projects[]` / `LLM_WIKI_PROJECTS` |
| 一直「正在检索资料…」 | RAG `search` 较慢，属正常；超过 120 秒会超时 |
| 一直「正在生成回答…」 | LLM 流未结束；检查 `LLM_WIKI_CONFIG`、Node/`npx tsx`、网络 |
| 回复显示原始 `**粗体**` | 未加载 `markdown.js` / `vendor/`；重新 `cp -r overlay/static/lite/. upstream/dist/lite/` 并强刷 |
| `BodyStreamBuffer was aborted` | 切换会话/返回首页时取消流式请求；已过滤，不应再显示在气泡内 |
| `/lite/` 打开却是完整版欢迎页 | 旧 server 未 serve 子目录 `index.html`；更新 `overlay/server` 并重启 |

验证 API：

```bash
curl "http://127.0.0.1:8080/api/v1/projects?token=e2e-test-token"
curl "http://127.0.0.1:8080/api/v1/runtime-config?token=e2e-test-token"
```

验证静态资源 MIME：

```bash
curl -sI http://127.0.0.1:8080/lite/vendor/purify.es.js | grep -i content-type
# 应为 application/javascript
```

## 相关文档

- 设计说明：[docs/superpowers/specs/2026-06-06-lite-chat-ui-design.md](../../../docs/superpowers/specs/2026-06-06-lite-chat-ui-design.md)
- 开发与 FAQ：[docs/开发与测试.md](../../../docs/开发与测试.md) §Q5、§Q8
- 新项目与对话：[docs/新项目指引.md](../../../docs/新项目指引.md) §6.1
