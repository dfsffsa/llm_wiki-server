# Overlay patches

Patches apply to `upstream/` submodule at build time via `./scripts/apply-patches.sh`.

| Patch | Purpose |
|-------|---------|
| `0002-http-ui-bootstrap.patch` | HTTP mode (v0.4.20+): App.tsx auto-open server project, vite HTTP aliases, `bootstrapHttpProject` stub in `fs.ts` |

Do not commit changes inside `upstream/` — only bump the submodule pointer after upstream releases.
