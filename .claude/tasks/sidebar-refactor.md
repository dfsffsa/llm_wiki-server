# Task: 侧边栏整合项目列表 + 空状态改造

## 目标

把项目列表从独立主页移到侧边栏上层，去掉 #view-home，chat 空状态改为类似 Onyx 的"How can I help?" + 推荐问题。

## 当前布局

```
Topbar (user+quota+theme+logout)
┌──────────┬────────────────────┐
│ Sidebar  │ Chat Main          │
│ (220px)  │ Messages / input   │
│ Conv list│                    │
└──────────┴────────────────────┘
```

## 目标布局

```
┌── Sidebar (240px) ────┬── Main Content ──────────┐
│ 📁 项目1 [active]      │                          │
│ 📁 项目2              │  How can I help?         │
│ ─────────────────────  │                          │
│ 🔍 Search conv        │  [推荐问题1]             │
│ ＋ New Chat           │  [推荐问题2]             │
│ ─────────────────────  │  [推荐问题3]             │
│ Recent conversations  │                          │
│ (empty: guide text)   │   ┌─ 输入… ─┐ [发送]    │
│ ─────────────────────  │   └─────────┘           │
│ 👤 user@email         │                          │
│ 🔴 今日剩余 N/M       │                          │
└───────────────────────┴──────────────────────────┘
```

## 改动

### index.html

1. 去掉 `<header class="topbar" id="topbar">`（用户信息移到侧边栏底部）
2. 去掉 `<section id="view-home">`（项目卡片选择页）
3. 侧边栏 `<aside class="history-sidebar">` 改为以下结构：

```html
<aside class="sidebar" id="sidebar">
  <!-- 项目列表 -->
  <div class="sidebar-section" id="sidebar-projects">
    <div class="section-header">
      <span>知识库</span>
    </div>
    <div id="project-list" class="project-list"></div>
  </div>
  <div class="sidebar-divider"></div>
  <!-- 对话功能区 -->
  <div class="sidebar-section">
    <div class="section-header">
      <button id="btn-search-sidebar" class="sidebar-action">🔍 搜索对话</button>
      <button id="btn-new-chat" class="sidebar-action">＋ 新建</button>
    </div>
  </div>
  <!-- 对话列表 -->
  <ul id="history-list" class="history-list"></ul>
  <!-- 底部：用户信息 -->
  <div class="sidebar-footer">
    <span id="sidebar-user" class="sidebar-user"></span>
    <span id="sidebar-usage" class="sidebar-usage"></span>
    <button id="btn-logout-sidebar" class="sidebar-logout">登出</button>
  </div>
</aside>
```

4. `#view-chat` 不需要大改，只移除 `#btn-back`（不需要回主页了）

### app.css

1. 添加 `.sidebar` 样式（取代旧的 `.history-sidebar`）：width 240px，flex column
2. `.project-list`：项目按钮列表，当前项目高亮
3. `.sidebar-section` + `.section-header`：区域标题样式
4. `.sidebar-divider`：分隔线
5. `.sidebar-footer`：底部固定，flex row，显示用户信息+登出
6. 空状态样式：`.empty-state` 居中显示 welcome 文字 + 推荐问题 buttons
7. 移除旧的 `.topbar` 相关样式
8. 保留 `.history-list` / `.history-item` / `.msg-reasoning` 等现有样式

### app.js

1. 删除（或注释掉）`#view-home` 相关的逻辑：`renderProjectGrid`、`#btn-back` listener、`showView("home")` 调用
2. 初始化时直接进入聊天视图，不再显示 home
3. 在 `init()` 中，获取项目列表后调用 `renderProjectList()` 渲染侧边栏项目
4. 点击项目→ `openProject()`（与现有行为一致）
5. `openProject()` 改为直接设置当前项目、渲染推荐问题、不切换 view
6. 添加 `renderEmptyState()`：显示 "How can I help?" + 基于当前项目的推荐问题按钮。点击推荐问题触发 `sendMessage()`
7. 用户信息渲染：在 `renderTopbar()` 改为 `renderSidebarUser()`，显示 email + 额度 + 登出按钮
8. `showView()` 只在 `"chat"` 模式（去掉 `"home"`）
9. 移除 `showView("home")` 调用（`#btn-back` listener）
10. 搜索对话按钮 `#btn-search-sidebar` 绑定到 `showSearch()`

具体函数实现参考：

```js
function renderProjectList() {
  const el = $("#project-list");
  if (!el) return;
  el.innerHTML = "";
  for (const p of state.projects) {
    const btn = document.createElement("button");
    btn.className = "sidebar-project" + (p.id === state.activeProject?.id ? " active" : "");
    btn.textContent = (p.emoji || "📁") + " " + (p.title || p.name);
    btn.addEventListener("click", () => openProject(p));
    el.appendChild(btn);
  }
}

function renderEmptyState() {
  const msgs = $("#messages");
  if (!msgs) return;
  msgs.innerHTML = "";
  const welcome = document.createElement("div");
  welcome.className = "empty-state";
  welcome.innerHTML = `<h2 class="empty-title">How can I help?</h2>` +
    (state.activeProject?.starters?.length > 0
      ? `<div class="suggestion-list">` +
        state.activeProject.starters.map(s =>
          `<button class="suggestion-chip" data-text="${escapeHtml(s)}">${escapeHtml(s)}</button>`
        ).join("") +
        `</div>`
      : "");
  msgs.appendChild(welcome);
  welcome.querySelectorAll(".suggestion-chip").forEach(btn => {
    btn.addEventListener("click", () => sendMessage(btn.dataset.text));
  });
}

function renderSidebarUser() {
  const uel = $("#sidebar-user");
  const usageEl = $("#sidebar-usage");
  if (!state.user) { /* hide footer */ return; }
  if (uel) uel.textContent = state.user.email || "";
  if (usageEl && state.usage) {
    const r = Math.max(0, state.usage.limit - state.usage.used);
    usageEl.textContent = `剩余 ${r}/${state.usage.limit}`;
  }
}
```

### app.js 中的 showView() 简化

```js
function showView(name) {
  // home view removed — only chat and search exist
  $("#view-search").classList.toggle("active", name === "search");
  // chat view always visible as default
  $("#view-chat").classList.toggle("active", name !== "search");
}
```

### app.js init() 改造

```js
async function init() {
  if (!(await ensureLogin())) return;
  try {
    const [projectsRes, metaRes, runtimeRes] = await Promise.all([
      apiGet("/api/v1/projects"),
      fetch("/lite/projects.meta.json").then((r) => r.json()),
      apiGet("/api/v1/runtime-config").catch(() => ({ chatEnabled: false })),
    ]);
    state.meta = metaRes;
    state.projects = (projectsRes.projects || []).map(mergeProject);
    state.chatEnabled = runtimeRes.chatEnabled !== false;
    if (!state.chatEnabled) showBanner("问答功能暂不可用，请检查服务端 LLM 配置。");
    if (!state.projects.length) showBanner("暂无可用知识库，请在服务端配置 projects。");
    renderProjectList();
    if (state.projects.length > 0) {
      openProject(state.projects[0]); // 默认选中第一个项目
    }
    renderEmptyState();
    renderSidebarUser();
  } catch (err) { showBanner(`无法连接服务：${err instanceof Error ? err.message : err}`); }
  // remove: #btn-back listener (no home to go back to)
  $("#btn-new-chat").addEventListener("click", () => newConversation());
  $("#btn-search-sidebar")?.addEventListener("click", () => showSearch());
  // ... theme toggle, input handlers (unchanged)
  initCitationCard();
  // ... search handlers
  // ... input wiring
}
```

### 用户信息变更

当前 `renderTopbar()` 函数改为 `renderSidebarUser()`（见上）。移除旧 topbar 相关代码。logout 按钮 listener 绑定到 `#btn-logout-sidebar`。

### CSS 新增样式

```css
.sidebar { width: 240px; display: flex; flex-direction: column; background: var(--surface); border-right: 1px solid var(--border); flex-shrink: 0; }
.project-list { display: flex; flex-direction: column; gap: 2px; padding: 4px 8px; }
.sidebar-project { display: block; width: 100%; text-align: left; padding: 8px 10px; border: none; background: transparent; border-radius: 8px; cursor: pointer; font-size: 0.9rem; color: var(--text); }
.sidebar-project:hover, .sidebar-project.active { background: var(--accent-soft); }
.sidebar-divider { height: 1px; background: var(--border); margin: 4px 8px; }
.sidebar-section { padding: 6px 8px; }
.section-header { display: flex; justify-content: space-between; align-items: center; font-size: 0.8rem; color: var(--text-muted); padding: 0 4px; margin-bottom: 4px; }
.sidebar-action { background: none; border: none; color: var(--accent); cursor: pointer; font-size: 0.8rem; padding: 4px; }
.sidebar-footer { margin-top: auto; padding: 8px 12px; border-top: 1px solid var(--border); font-size: 0.8rem; display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
.sidebar-user { color: var(--text); font-weight: 500; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sidebar-usage { color: var(--text-muted); font-size: 0.75rem; }
.sidebar-logout { background: none; border: none; color: var(--accent); cursor: pointer; font-size: 0.75rem; padding: 2px 4px; }

/* Empty state */
.empty-state { display: flex; flex-direction: column; align-items: center; justify-content: center; flex: 1; text-align: center; padding: 2rem; gap: 1.5rem; }
.empty-title { font-size: 1.5rem; font-weight: 600; color: var(--text); margin: 0; }
.suggestion-list { display: flex; flex-direction: column; gap: 8px; max-width: 400px; width: 100%; }
.suggestion-chip { padding: 10px 16px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); cursor: pointer; font-size: 0.9rem; color: var(--text); transition: background 0.12s; text-align: left; }
.suggestion-chip:hover { background: var(--accent-soft); }
```

## 约束

- 必须保留现有功能：来源卡片、引用角标、深色模式、搜索页、conversations API 调用
- 不修改任何 Rust 文件
- 修改后 `cp` 到 `upstream/dist/lite/` 同步
- app.js 语法必须通过 `node --check`
- Commit message: `feat(lite): sidebar project list, empty state, user in sidebar footer`

## 执行步骤

1. 读当前 `index.html` / `app.js` / `app.css` 理解完整结构
2. 按上述设计修改三个文件
3. 同步 dist：`cp overlay/static/lite/{app.js,app.css,index.html} upstream/dist/lite/`
4. 语法检查：`node --check overlay/static/lite/app.js`
5. 构建确认：`cargo build --release --manifest-path overlay/server/Cargo.toml`
6. 浏览器或 curl 测试
7. Commit

## 参考

工作目录：`/home/li/overseas-github/llm_wiki-server`，分支 `feat/public-deploy-auth`。当前 HEAD: `fc83fb5`（待确认——先 `git log -1` 查看）
