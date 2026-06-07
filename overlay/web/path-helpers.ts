import { getRelativePath, isAbsolutePath, joinPath, normalizePath } from "@/lib/path-utils"
import { getBackendClient, resolveHttpProject } from "./backend-client"
import { readOnlyMode } from "./env"

export function readOnlyError(action: string): Error {
  return new Error(`Read-only HTTP mode: ${action} is not available in the browser UI.`)
}

export async function getActiveProject(): Promise<{ id: string; path: string }> {
  const project = await resolveHttpProject()
  if (!project) throw new Error("No wiki project configured on the server (LLM_WIKI_PROJECT)")
  return { id: project.id, path: normalizePath(project.path) }
}

export function toRelativeProjectPath(projectPath: string, filePath: string): string {
  const root = normalizePath(projectPath).replace(/\/$/, "")
  const target = normalizePath(filePath)
  if (isAbsolutePath(target)) {
    return getRelativePath(target, root)
  }
  return target.replace(/^\/+/, "")
}

export function toAbsoluteProjectPath(projectPath: string, relPath: string): string {
  const root = normalizePath(projectPath).replace(/\/$/, "")
  const rel = normalizePath(relPath).replace(/^\/+/, "")
  return joinPath(root, rel)
}

export async function readViaApi(projectPath: string, filePath: string): Promise<string> {
  const { id } = await getActiveProject()
  const rel = toRelativeProjectPath(projectPath, filePath)
  return getBackendClient().readFileContent(id, rel)
}

export function assertWritable(action: string): void {
  if (readOnlyMode) throw readOnlyError(action)
}
