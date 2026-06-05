# upstream 与官方仓库同步原理

> 最后更新：2026-06-05  
> 官方仓库：[nashsu/llm_wiki](https://github.com/nashsu/llm_wiki)  
> 集成仓库：`llm_wiki-server`（overlay + submodule）

本文说明 `upstream/` 目录如何与官网源码保持对齐、overlay 如何在不 fork 官方 repo 的前提下扩展行为，以及日常维护操作。

---

## 1. 核心原理

### 1.1 `upstream/` 是什么

`upstream/` **不是**复制粘贴的一份源码，而是 Git **submodule（子模块）**：

```ini
# .gitmodules
[submodule "upstream"]
    path = upstream
    url = https://github.com/nashsu/llm_wiki.git
```

| 概念 | 说明 |
|------|------|
| **官方仓库** | `https://github.com/nashsu/llm_wiki.git`，由 nashsu 维护发布 |
| **submodule** | 在 `llm_wiki-server` 里嵌套的独立 Git 仓库，remote 指向官方 |
| **父仓库记录的内容** | 仅一个 **commit SHA**（例如 `9712d43` = tag `v0.4.20`），不是 upstream 里每一行文件的 diff |

clone `llm_wiki-server` 时，Git 根据这个 SHA 从官方仓库检出对应版本的 `upstream/` 目录。

### 1.2 overlay 与 upstream 的分工

```text
github.com/nashsu/llm_wiki          github.com/dfsffsa/llm_wiki-server
        │                                      │
        │  fetch / checkout tag                │  记录 submodule 指针
        └──────────────► upstream/ ◄───────────┘
                         (submodule)
                              │
              apply-patches.sh（构建时，不 commit 进 submodule）
                              │
              overlay/server、overlay/web、overlay/cli …
```

| 目录 | 谁维护 | 是否提交到 llm_wiki-server |
|------|--------|---------------------------|
| `upstream/` 官方 commit 指针 | 维护者 bump | ✅（仅 SHA 变化） |
| `overlay/` | 本集成项目 | ✅ |
| `overlay/patches/*.patch` | 本集成项目 | ✅ |
| 构建时 apply 到 `upstream/` 的改动 | 构建脚本 | ❌ |
| 在 `upstream/` 内手改并 commit | **禁止** | ❌ |

**原则：** 官方代码零 fork；定制全部在 `overlay/` + patch 文件；HTTP 等对 upstream 文件的改动通过 **`overlay/patches/` + `scripts/apply-patches.sh`** 在构建时应用。

### 1.3 不会「自动」与官网同步

- 官方发新版本 **不会** 自动更新你们的 `llm_wiki-server`。
- 需要维护者主动运行 `./scripts/sync-upstream.sh <tag>`，验证通过后 **commit submodule 指针** 并 push。
- 团队其他人 `git pull` + `git submodule update` 后才会对齐到同一官方版本。

推荐策略（见 [ARCHITECTURE.md](./ARCHITECTURE.md)）：**按 Release tag 升级**（如 `v0.4.20`），不追 upstream 每个 commit。

---

## 2. 首次获取 upstream

### 2.1 克隆集成仓库（推荐）

```bash
git clone --recurse-submodules git@github.com:dfsffsa/llm_wiki-server.git
cd llm_wiki-server
```

`--recurse-submodules` 会同时初始化 `upstream/` 并检出指针指向的 commit。

### 2.2 已 clone 但未拉 submodule

```bash
cd llm_wiki-server
git submodule update --init --recursive
```

### 2.3 确认状态

```bash
git submodule status
# 示例： 9712d43... upstream (v0.4.20)

cd upstream && git remote -v && git describe --tags --always
# origin → https://github.com/nashsu/llm_wiki.git
# v0.4.20
```

---

## 3. 日常开发（只改 overlay）

**不要**在 `upstream/` 里做业务改动或 commit。

```bash
git checkout -b overlay/my-feature
# 编辑 overlay/、scripts/、docs/ …
git add overlay/ scripts/ docs/
git commit -m "feat(server): ..."
git push origin overlay/my-feature
```

构建 HTTP UI 前在本地应用 patch（若尚未 apply）：

```bash
./scripts/apply-patches.sh
VITE_BACKEND=http ./scripts/build-web.sh
```

`upstream/` 显示 `modified`（patch 已 apply）是**正常的本地构建状态**；提交父仓库时 **不要** 把这类 dirty submodule 一并 commit。

---

## 4. 升级到官方新版本

### 4.1 标准流程

```bash
cd llm_wiki-server

# 1. 确保 upstream 工作区干净（丢弃本地 patch 脏改动）
cd upstream && git reset --hard && cd ..

# 2. 同步到指定 tag 或分支
./scripts/sync-upstream.sh v0.4.20
# 或跟踪 main：./scripts/sync-upstream.sh main

# 3. 重新应用 overlay patch，检查是否冲突
./scripts/apply-patches.sh

# 4. 验证构建与测试
./scripts/build-all.sh
npm run test:mocks --prefix upstream
./scripts/e2e-full.sh   # 可选

# 5. 在父仓库只提交 submodule 指针 + 如有 patch 更新
git add upstream overlay/patches/
git commit -m "chore: bump upstream to v0.4.20"
git push origin main
```

`sync-upstream.sh` 内部逻辑：

1. 在 `upstream/` 内 `git fetch origin --tags`
2. checkout 指定 tag，或 `origin/<branch>`
3. （可选）运行 `npm run test:mocks`
4. 提示你在父仓库 `git add upstream`

### 4.2 patch 冲突时

若 `apply-patches.sh` 报 patch 无法 apply：

```bash
cd upstream
git reset --hard v0.4.20          # 干净官方树
# 手动合并 overlay 所需改动（对照 overlay/patches/ 与 overlay/web/）
git diff > ../overlay/patches/0002-http-ui-bootstrap.patch
cd ..
./scripts/apply-patches.sh        # 确认 idempotent
```

更新 patch 文件后，与 submodule 指针 **一起 commit** 到父仓库。

### 4.3 团队成员如何跟进 bump

```bash
git pull origin main
git submodule update --init --recursive
./scripts/apply-patches.sh
./scripts/build-all.sh   # 按需
```

---

## 5. 构建时 patch 的作用

当前 patch：`overlay/patches/0002-http-ui-bootstrap.patch`

| 作用 | 说明 |
|------|------|
| HTTP UI 启动 | `App.tsx` 在 `VITE_BACKEND=http` 时从 server 拉 project |
| Vite 别名 | `vite.config.ts` 将 `@/commands/fs` 等指向 `overlay/web/` |
| 类型 stub | `bootstrapHttpProject()` 等，供桌面构建 typecheck |

调用点：

- `./scripts/apply-patches.sh`（`build-all.sh`、`build-web.sh`、Docker 构建前）
- 脚本用 `git apply --check` / `--reverse --check` 保证**幂等**（已 apply 则跳过）

**重要：** patch 修改的是**本地 checkout 的 upstream 工作区**；官方 GitHub 上的 `nashsu/llm_wiki` **不受影响**。

---

## 6. 常见误操作与纠正

| 误操作 | 问题 | 纠正 |
|--------|------|------|
| 在 `upstream/` 内 commit 定制代码 | 污染 submodule，难以升级 | 改动挪到 `overlay/` 或 patch；`git reset --hard <tag>` |
| `git add upstream` 时 submodule 为 `-dirty` | 父仓库记录了 patch 脏状态 | `cd upstream && git reset --hard v0.4.20` 后再 add |
| 忘记 `--recurse-submodules` | `upstream/` 空目录 | `git submodule update --init --recursive` |
| 只 pull 父仓库不 update submodule | overlay 新代码配旧 upstream | `git submodule update` |
| 每个 upstream commit 都 bump | 集成测试成本高 | 按 **Release tag** 升级 |

### 恢复 upstream 到干净官方版本

```bash
cd upstream
git fetch origin --tags
git checkout v0.4.20    # 或当前集成线使用的 tag
git reset --hard
cd ..
./scripts/apply-patches.sh
```

---

## 7. 与 wiki 数据目录的关系

| 路径 | 是否在 llm_wiki-server 仓库内 |
|------|-------------------------------|
| `upstream/` | ✅ submodule（官方**应用源码**） |
| `overlay/` | ✅ 定制实现 |
| `~/.../llm_wiki_projects/CivilCareer/` 等 | ❌ 运行时 `LLM_WIKI_PROJECT` 挂载，不进 Git |

官方 repo 提供**程序**；你的 wiki 项目（`wiki/`、`raw/`、`.llm-wiki/`）是**数据**，单独存放。

---

## 8. 相关命令速查

| 命令 | 用途 |
|------|------|
| `git submodule status` | 查看当前 upstream SHA / 是否 dirty |
| `git submodule update --init --recursive` | 按父仓库指针检出 upstream |
| `./scripts/sync-upstream.sh vX.Y.Z` | bump 到官方 tag |
| `./scripts/apply-patches.sh` | 构建前应用 overlay patch |
| `./scripts/build-all.sh` | patch + HTTP UI + server + CLI |

---

## 9. 相关文档

| 文档 | 内容 |
|------|------|
| [ARCHITECTURE.md](./ARCHITECTURE.md) | 分阶段计划、Git 原则、Phase 4 升级 |
| [README-OVERLAY.md](../README-OVERLAY.md) | Overlay 开发与构建速查 |
| [overlay/patches/README.md](../overlay/patches/README.md) | Patch 列表 |
| [DEVELOPMENT_AND_TESTING.md](./DEVELOPMENT_AND_TESTING.md) | 测试脚本与 FAQ |
