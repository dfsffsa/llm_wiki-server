# Overlay 开发指南

## 目录说明

```
overlay/
├── server/       # Headless HTTP 服务（Rust）— Phase 1
├── cli/          # 命令行工具 — Phase 3
│   ├── rust/     # search, preprocess, reindex
│   └── node/     # ingest（包装 upstream ingest.ts）
├── web/          # HttpBackend、Vite 环境 — Phase 2
├── embedding/    # Embedding 探索与实验（不含密钥）
└── config/       # 配置样例
```

## Git 工作流

### 日常开发（只改 overlay）

```bash
git checkout -b overlay/server/my-feature
# 编辑 overlay/...
git add overlay/
git commit -m "feat(server): ..."
```

### 升级 upstream

```bash
./scripts/sync-upstream.sh v0.4.20   # 或 origin/main
git add upstream
git commit -m "chore: bump upstream to v0.4.20"
```

**禁止**在 `upstream/` 目录内提交定制代码。若必须修改上游文件：

1. 在 `overlay/patches/` 新增 patch
2. 更新 `scripts/apply-patches.sh`
3. 在 PR/文档中记录原因，便于 upstream 合并后删除 patch

### 克隆带子模块

```bash
git clone --recurse-submodules git@github.com:dfsffsa/llm_wiki-server.git
# 已克隆但未初始化子模块：
git submodule update --init --recursive
```

## 构建（规划）

```bash
# 构建官方 UI
npm run build --prefix upstream

# 构建 overlay server（Phase 1 完成后）
cargo build --release --manifest-path overlay/server/Cargo.toml
```

## Embedding 实验

见 `overlay/embedding/README.md`。勿提交含 API Key 的脚本；使用 `test_embedding.example.py` 与环境变量。
