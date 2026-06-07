import type { FileNode, WikiProject } from "@/types/wiki"
import { normalizePath } from "@/lib/path-utils"
import {
  apiNodeToFileNode,
  getBackendClient,
  resolveHttpProject,
  toWikiProject,
} from "../backend-client"
import {
  assertWritable,
  getActiveProject,
  readViaApi,
  toAbsoluteProjectPath,
  toRelativeProjectPath,
} from "../path-helpers"

interface RawProject {
  name: string
  path: string
}

export interface FileBase64 {
  base64: string
  mimeType: string
}

export async function readFile(path: string): Promise<string> {
  const { path: projectPath } = await getActiveProject()
  return readViaApi(projectPath, path)
}

export async function writeFile(_path: string, _contents: string): Promise<void> {
  assertWritable("write file")
}

export async function writeFileAtomic(_path: string, _contents: string): Promise<void> {
  assertWritable("write file")
}

export async function listDirectory(path: string): Promise<FileNode[]> {
  const projectPath = normalizePath(path)
  const { id } = await getActiveProject()
  const data = await getBackendClient().listFiles(id, {
    root: "all",
    recursive: true,
    maxFiles: 2000,
  })
  return data.files.map((node) => apiNodeToFileNode(node, projectPath))
}

export async function copyFile(_source: string, _destination: string): Promise<void> {
  assertWritable("copy file")
}

export async function copyDirectory(_source: string, _destination: string): Promise<string[]> {
  assertWritable("copy directory")
  return []
}

export async function preprocessFile(_path: string): Promise<string> {
  assertWritable("preprocess file")
  return ""
}

export async function deleteFile(_path: string): Promise<void> {
  assertWritable("delete file")
}

export async function findRelatedWikiPages(
  _projectPath: string,
  _sourceName: string,
): Promise<string[]> {
  return []
}

export async function createDirectory(_path: string): Promise<void> {
  assertWritable("create directory")
}

export async function fileExists(path: string): Promise<boolean> {
  try {
    await readFile(path)
    return true
  } catch {
    return false
  }
}

export async function getFileModifiedTime(_path: string): Promise<number> {
  return 0
}

export async function getFileSize(_path: string): Promise<number> {
  return 0
}

export async function getFileMd5(_path: string): Promise<string> {
  return ""
}

export async function readFileAsBase64(_path: string): Promise<FileBase64> {
  throw new Error("Binary file reads are not supported in HTTP read-only mode")
}

export async function createProject(_name: string, _path: string): Promise<WikiProject> {
  assertWritable("create project")
  throw new Error("unreachable")
}

export async function openProject(path: string): Promise<WikiProject> {
  const data = await getBackendClient().getProjects()
  const normalized = normalizePath(path)
  const match =
    data.projects.find((p) => normalizePath(p.path) === normalized) ??
    data.currentProject ??
    data.projects[0]
  if (!match) throw new Error(`Unknown project: ${path}`)
  return toWikiProject(match)
}

export async function openProjectFolder(_path: string): Promise<void> {
  assertWritable("open folder in file manager")
}

export async function clipServerStatus(): Promise<string> {
  return "disabled (HTTP mode)"
}

export async function apiServerStatus(): Promise<string> {
  try {
    const health = await getBackendClient().health()
    return String(health.status ?? "running")
  } catch {
    return "error"
  }
}

export async function apiServerReloadConfig(): Promise<string> {
  return "ok"
}

export async function mcpServerEntryPath(): Promise<string> {
  throw new Error("MCP server is not available in HTTP read-only mode")
}

/** Bootstrap helper used by App.tsx patch in HTTP mode. */
export async function bootstrapHttpProject(): Promise<WikiProject | null> {
  const project = await resolveHttpProject()
  if (!project) return null

  try {
    const { useWikiStore } = await import("@/stores/wiki-store")
    const runtime = await getBackendClient().getRuntimeConfig()
    if (runtime.chatEnabled && runtime.llmConfig) {
      useWikiStore.getState().setLlmConfig(runtime.llmConfig as import("@/stores/wiki-store").LlmConfig)
    }
  } catch (err) {
    console.warn("Failed to load runtime LLM config:", err)
  }

  return project
}

export type { RawProject }
