/**
 * HTTP backend mode — enabled when building with VITE_BACKEND=http.
 */
declare const __HTTP_BACKEND__: boolean

export const isHttpBackend =
  import.meta.env.VITE_BACKEND === "http" || __HTTP_BACKEND__ === true

/** API base URL. Empty string = same origin (static UI served by llm-wiki-server). */
export const apiBaseUrl: string = (
  import.meta.env.VITE_API_BASE ?? ""
).replace(/\/$/, "")

/** Bearer token for API auth (must match LLM_WIKI_API_TOKEN on server). */
export const apiToken: string = import.meta.env.VITE_API_TOKEN ?? ""

/** Optional fixed project path hint (server also exposes via GET /projects). */
export const wikiProjectPath: string | undefined =
  import.meta.env.VITE_WIKI_PROJECT || undefined

export const readOnlyMode = isHttpBackend
