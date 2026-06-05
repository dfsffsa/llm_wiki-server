import type { FileNode, WikiProject } from "@/types/wiki"
import { apiBaseUrl, apiToken } from "../env"

export interface ApiFileNode {
  name: string
  path: string
  isDir: boolean
  size?: number
  children?: ApiFileNode[]
}

export interface ProjectsResponse {
  ok: boolean
  projects: Array<{
    id: string
    name: string
    path: string
    current: boolean
  }>
  currentProject: ProjectsResponse["projects"][number] | null
}

export interface FileContentResponse {
  ok: boolean
  projectId: string
  path: string
  content: string
}

export interface FilesResponse {
  ok: boolean
  projectId: string
  root: string
  files: ApiFileNode[]
  truncated: boolean
}

export interface SearchApiResponse {
  ok: boolean
  projectId: string
  mode: string
  note?: string
  tokenHits: number
  vectorHits: number
  results: Array<{
    path: string
    title: string
    snippet: string
    titleMatch: boolean
    score: number
    vectorScore?: number
    images: Array<{ url: string; alt: string }>
    content?: string
  }>
}

export class HttpBackendError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message)
    this.name = "HttpBackendError"
  }
}

export class HttpBackendClient {
  constructor(
    private readonly baseUrl = apiBaseUrl,
    private readonly token = apiToken,
  ) {}

  private url(path: string, query?: Record<string, string | number | boolean | undefined>): string {
    const q = new URLSearchParams()
    if (this.token) q.set("token", this.token)
    if (query) {
      for (const [k, v] of Object.entries(query)) {
        if (v !== undefined) q.set(k, String(v))
      }
    }
    const qs = q.toString()
    return `${this.baseUrl}${path}${qs ? `?${qs}` : ""}`
  }

  private headers(): HeadersInit {
    const h: Record<string, string> = {
      Accept: "application/json",
    }
    if (this.token) {
      h.Authorization = `Bearer ${this.token}`
    }
    return h
  }

  private async parse<T>(res: Response): Promise<T> {
    const body = (await res.json().catch(() => ({}))) as { error?: string; ok?: boolean }
    if (!res.ok || body.ok === false) {
      throw new HttpBackendError(body.error ?? res.statusText, res.status)
    }
    return body as T
  }

  async health(): Promise<Record<string, unknown>> {
    const res = await fetch(this.url("/api/v1/health"), { headers: this.headers() })
    return this.parse(res)
  }

  async getProjects(): Promise<ProjectsResponse> {
    const res = await fetch(this.url("/api/v1/projects"), { headers: this.headers() })
    return this.parse(res)
  }

  async listFiles(
    projectId: string,
    opts: { root?: string; recursive?: boolean; maxFiles?: number } = {},
  ): Promise<FilesResponse> {
    const res = await fetch(
      this.url(`/api/v1/projects/${encodeURIComponent(projectId)}/files`, {
        root: opts.root ?? "all",
        recursive: opts.recursive ?? true,
        maxFiles: opts.maxFiles ?? 2000,
      }),
      { headers: this.headers() },
    )
    return this.parse(res)
  }

  async readFileContent(projectId: string, relPath: string): Promise<string> {
    const res = await fetch(
      this.url(`/api/v1/projects/${encodeURIComponent(projectId)}/files/content`, {
        path: relPath,
      }),
      { headers: this.headers() },
    )
    const data = await this.parse<FileContentResponse>(res)
    return data.content
  }

  async search(
    projectId: string,
    query: string,
    opts: { topK?: number; includeContent?: boolean } = {},
  ): Promise<SearchApiResponse> {
    const res = await fetch(
      this.url(`/api/v1/projects/${encodeURIComponent(projectId)}/search`),
      {
        method: "POST",
        headers: { ...this.headers(), "Content-Type": "application/json" },
        body: JSON.stringify({
          query,
          topK: opts.topK ?? 20,
          includeContent: opts.includeContent ?? false,
        }),
      },
    )
    return this.parse(res)
  }
}

let client: HttpBackendClient | null = null

export function getBackendClient(): HttpBackendClient {
  if (!client) client = new HttpBackendClient()
  return client
}

export function toWikiProject(entry: ProjectsResponse["projects"][number]): WikiProject {
  return { id: entry.id, name: entry.name, path: entry.path }
}

export function apiNodeToFileNode(node: ApiFileNode, projectPath: string): FileNode {
  const prefix = projectPath.replace(/\/$/, "")
  const rel = node.path.replace(/^\/+/, "")
  return {
    name: node.name,
    path: rel.includes("/") || rel.includes("\\") ? `${prefix}/${rel}` : `${prefix}/${rel}`,
    is_dir: node.isDir,
    children: node.children?.map((c) => apiNodeToFileNode(c, projectPath)),
  }
}

export async function resolveHttpProject(): Promise<WikiProject | null> {
  const data = await getBackendClient().getProjects()
  if (data.currentProject) return toWikiProject(data.currentProject)
  return data.projects[0] ? toWikiProject(data.projects[0]) : null
}
