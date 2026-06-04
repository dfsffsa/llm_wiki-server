# Web platform adapter (Phase 2)

将 `upstream` 前端从 Tauri `invoke()` 切换为 HTTP `BackendClient`。

## 规划文件

- `backend-client.ts` — `HttpBackend` 实现
- `env.ts` — `VITE_BACKEND`, `VITE_API_TOKEN`
- `vite-plugin-llm-wiki.ts` — 构建时 alias `@/commands/fs` → overlay 实现

## 构建（规划）

```bash
cd upstream
VITE_BACKEND=http://127.0.0.1:8080 \
VITE_API_TOKEN=your-token \
npm run build
```
