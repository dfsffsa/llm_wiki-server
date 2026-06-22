# Overlay 与 Upstream 融合机制

> 最后更新：2026-06-08

本文说明 `overlay/` 如何与 `upstream/` submodule融合，以及升级同步的完整机制。

---

## 1. 核心概念

### 1.1 Upstream 是什么

`upstream/` 是一个 **Git Submodule**，指向官方仓库：

```ini
# .gitmodules
[submodule "upstream"]
    path = upstream
    url = https://github.com/nashsu/llm_wiki.git
```

| 概念 | 说明 |
|------|------|
| **官方仓库** | `nashsu/llm_wiki`，由上游维护者发布 |
| **submodule** | 嵌套的独立 Git 仓库，remote 指向官方 |
| **父仓库记录的内容** | 仅一个 **commit SHA**（如 `9712d43` = tag `v0.4.20`） |

父仓库通过 SHA 指针记录 upstream 版本，clone 时 Git 会检出对应版本的 `upstream/` 目录。

### 1.2 Overlay是什么

`overlay/` 是自定义实现的集合：

```
overlay/
├── server/              # HTTP 服务（Rust）
├── cli/                  # CLI 工具
├── web/                  # HTTP 前端适配层
├── crates/llm-wiki-common/  # 共享 Rust 库
├── patches/              # 对 upstream 的 patch 文件
└── config/               # 配置样例
```

---

## 2. 融合机制

### 2.1 三层架构

```
┌─────────────────────────────────────────────────────────────────┐
│                    融合架构                                      │
├─────────────────────────────────────────────────────────────────┤
│ │
│  Git 仓库层 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ llm_wiki-server (main)                                  │   │
│  │   ├── upstream/ (submodule) → nashsu/llm_wiki           │   │
│  │   └── overlay/ (本地目录)                                 │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  代码融合层（在构建时）                                           │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 1. apply-patches.sh → 修改 upstream源码                 │   │
│  │ 2. Vite alias → 将 upstream 模块重定向到 overlay/web/*   │   │
│  │ 3. npm run build → 生成 upstream/dist                    │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  运行层 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ upstream/dist (静态文件) + overlay/server (HTTP 服务)   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 Patch 机制

`overlay/patches/` 存放对 upstream 的修改，通过 `apply-patches.sh` 在构建时应用：

```bash
./scripts/apply-patches.sh
# 效果：在 upstream/ 目录应用 patch（修改本地 upstream 工作区）
```

**当前 patch：** `0002-http-ui-bootstrap.patch`

| 修改文件 | 改动内容 |
|----------|----------|
| `App.tsx` | HTTP模式下从 server加载项目 |
| `fs.ts` | 添加 `bootstrapHttpProject()` stub |
| `vite.config.ts` | 添加 HTTP alias 配置 |
| `vite-env.d.ts` | 添加 VITE_* 类型定义 |

**设计原则：** 不修改官方仓库，而是用 patch 文件记录差异。

### 2.3 Vite Alias 重定向

当 `VITE_BACKEND=http` 时，Vite 配置将 upstream 模块重定向到 overlay：

```typescript
// upstream/vite.config.ts
alias: {
  "@": path.resolve(__dirname, "./src"),
  // 当 VITE_BACKEND=http 时，以下 alias生效
  "@/commands/fs": path.join(overlayWeb, "commands/fs.ts"),
  "@/lib/search": path.join(overlayWeb, "lib/search.ts"),
  "@/lib/llm-client": path.join(overlayWeb, "lib/llm-client.ts"),
  // ...
}
```

**关键点：** overlay alias 必须写在 `@` **之前**，否则会被通用 `@` 捕获。

### 2.4 调用链对比

| 操作 | 桌面 Tauri 模式 | HTTP 只读模式 |
|------|----------------|---------------|
| 读取文件 | `fs.ts` → Tauri invoke | `overlay/web/commands/fs.ts` → `GET /files` |
| 搜索 | `search.ts` → Tauri invoke | `overlay/web/lib/search.ts` → `POST /search` |
| Chat |浏览器直连 LLM | `overlay/web/lib/llm-client.ts` → `POST /chat` (server代理) |

---

## 3. 同步更新流程

### 3.1 完整流程

```
┌─────────────────────────────────────────────────────────────────┐
│                    同步更新流程 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Step 1: 准备干净的 upstream │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ cd upstream && git reset --hard                        │    │
│  │ # 丢弃本地 patch 脏改动                                  │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
│  Step 2: 升级到新版本                                           │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ ./scripts/sync-upstream.sh v0.4.21 │    │
│  │ # 内部：git fetch + checkout tag/origin/branch          │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
│  Step 3: 重新应用 patch │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ ./scripts/apply-patches.sh                              │    │
│  │ # 检查 patch 是否能应用；若冲突需手动处理                │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
│  Step 4: 验证构建                                               │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ ./scripts/build-all.sh                                  │    │
│  │ npm run test:mocks --prefix upstream │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
│  Step 5: 提交更改 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ git add upstream overlay/patches                        │    │
│  │ git commit -m "chore: bump upstream to v0.4.21"        │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 Patch 冲突处理

当 upstream 升级后 patch 无法 apply 时：

```bash
# Step 1: 查看冲突
./scripts/apply-patches.sh
# 输出: warning: patch does not apply cleanly

# Step 2: 手动处理
cd upstream
git reset --hard v0.4.21  # 回到干净状态

# 对照 overlay/web/ 中的改动，手动修改 upstream 文件
# 编辑相关文件...

# Step 3: 生成新 patch
cd upstream
git diff > ../overlay/patches/0002-http-ui-bootstrap.patch

# Step 4: 验证 patch
cd ..
./scripts/apply-patches.sh  # 应显示 already applied
```

### 3.3 团队成员跟进

```bash
# 拉取更新
git pull origin main
git submodule update --init --recursive  # 更新 submodule指针

# 重新应用 patch
./scripts/apply-patches.sh

# 重新构建
./scripts/build-all.sh
```

---

## 4. 设计原则

### 4.1 核心原则

| 原则 | 说明 |
|------|------|
| **Upstream 零 commit** | 不在 `upstream/` 目录内提交业务代码 |
| **Overlay 全定制** | 100% 的自定义代码在 `overlay/` |
| **Patch at build time** | patch 在构建时应用，不影响官方仓库 |
| **按 Tag 升级** | 不追每个 commit，按 Release tag 升级 |

### 4.2 为什么这样设计

1. **保持上游同步能力**
   - 可以随时获取上游最新功能
   - 安全更新（上游 bugfix 可以快速合并）

2. **隔离定制代码**
   - 业务逻辑与官方代码分离
   - 便于维护和升级

3. **最小化 patch**
   - 仅修改必要的文件
   - 便于追踪和管理

---

## 5. 目录职责

| 路径 | 谁维护 | 是否进 Git |
|------|--------|------------|
| `upstream/` 官方 commit指针 | 维护者 bump | ✅（仅 SHA 变化） |
| `overlay/` | 本项目 | ✅ |
| `overlay/patches/*.patch` | 本项目 | ✅ |
| 构建时 apply 到 `upstream/` 的改动 | 构建脚本 | ❌ |
| 在 `upstream/` 内手改并 commit | **禁止** | ❌ |

---

## 6. 相关文档

| 文档 | 内容 |
|------|------|
| [上游同步.md](./上游同步.md) | submodule 同步原理与日常操作 |
| [README-OVERLAY.md](../README-OVERLAY.md) | Overlay 开发与构建速查 |
| [代码结构总览.md](./代码结构总览.md) | 架构与模块关系 |
| [overlay/patches/README.md](../overlay/patches/README.md) | Patch 说明 |
