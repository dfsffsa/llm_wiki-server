# llm-wiki-server (Phase 1)

Headless HTTP 服务：托管 `upstream/dist` 静态 UI，扩展官方 `api_server` 能力。

## 规划能力

- 绑定 `0.0.0.0`（可配置）
- 从 `LLM_WIKI_CONFIG` 读取配置（替代 Tauri `app_data_dir`）
- 复用 upstream `src-tauri` 的 `search`、`fs`、`vectorstore` 模块

## 实现状态

🚧 骨架阶段 — `src/main.rs` 为占位入口，待 Phase 1 实现。

## 构建

```bash
cargo build --release --manifest-path overlay/server/Cargo.toml
```
