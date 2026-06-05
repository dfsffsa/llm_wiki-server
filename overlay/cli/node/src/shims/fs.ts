import fs from "node:fs/promises"
import path from "node:path"
import type { FileNode, WikiProject } from "@/types/wiki"

function normalize(p: string): string {
  return p.replace(/\\/g, "/")
}

async function readText(pathStr: string): Promise<string> {
  return fs.readFile(pathStr, "utf8")
}

export async function readFile(pathStr: string): Promise<string> {
  return readText(pathStr)
}

export async function writeFile(pathStr: string, contents: string): Promise<void> {
  await fs.mkdir(path.dirname(pathStr), { recursive: true })
  await fs.writeFile(pathStr, contents, "utf8")
}

export async function writeFileAtomic(pathStr: string, contents: string): Promise<void> {
  await writeFile(pathStr, contents)
}

async function buildTree(dir: string, root: string): Promise<FileNode[]> {
  const entries = await fs.readdir(dir, { withFileTypes: true })
  const nodes: FileNode[] = []
  for (const entry of entries) {
    if (entry.name.startsWith(".")) continue
    const full = path.join(dir, entry.name)
    const rel = normalize(path.relative(root, full))
    if (entry.isDirectory()) {
      const children = await buildTree(full, root)
      nodes.push({
        name: entry.name,
        path: normalize(full),
        is_dir: true,
        children: children.length > 0 ? children : undefined,
      })
    } else if (entry.isFile()) {
      nodes.push({ name: entry.name, path: normalize(full), is_dir: false })
    }
  }
  nodes.sort((a, b) => Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name))
  return nodes
}

export async function listDirectory(pathStr: string): Promise<FileNode[]> {
  const root = normalize(pathStr)
  return buildTree(root, root)
}

export async function copyFile(source: string, destination: string): Promise<void> {
  await fs.mkdir(path.dirname(destination), { recursive: true })
  await fs.copyFile(source, destination)
}

export async function copyDirectory(_source: string, _destination: string): Promise<string[]> {
  throw new Error("copyDirectory not implemented in CLI fs shim")
}

export async function preprocessFile(pathStr: string): Promise<string> {
  return readFile(pathStr)
}

export async function deleteFile(pathStr: string): Promise<void> {
  await fs.unlink(pathStr)
}

export async function findRelatedWikiPages(_projectPath: string, _sourceName: string): Promise<string[]> {
  return []
}

export async function createDirectory(pathStr: string): Promise<void> {
  await fs.mkdir(pathStr, { recursive: true })
}

export async function fileExists(pathStr: string): Promise<boolean> {
  try {
    await fs.access(pathStr)
    return true
  } catch {
    return false
  }
}

export async function getFileModifiedTime(pathStr: string): Promise<number> {
  const stat = await fs.stat(pathStr)
  return stat.mtimeMs
}

export async function getFileSize(pathStr: string): Promise<number> {
  const stat = await fs.stat(pathStr)
  return stat.size
}

export async function getFileMd5(_pathStr: string): Promise<string> {
  return ""
}

export async function readFileAsBase64(pathStr: string): Promise<{ base64: string; mimeType: string }> {
  const buf = await fs.readFile(pathStr)
  const ext = path.extname(pathStr).toLowerCase()
  const mimeType =
    ext === ".png" ? "image/png" : ext === ".jpg" || ext === ".jpeg" ? "image/jpeg" : "application/octet-stream"
  return { base64: buf.toString("base64"), mimeType }
}

export async function createProject(name: string, projectPath: string): Promise<WikiProject> {
  return { id: projectPath, name, path: projectPath }
}

export async function openProject(projectPath: string): Promise<WikiProject> {
  return { id: projectPath, name: path.basename(projectPath), path: projectPath }
}

export async function openProjectFolder(_pathStr: string): Promise<void> {}

export async function clipServerStatus(): Promise<string> {
  return "disabled (CLI)"
}

export async function apiServerStatus(): Promise<string> {
  return "disabled (CLI)"
}

export async function apiServerReloadConfig(): Promise<string> {
  return "ok"
}

export async function bootstrapHttpProject(): Promise<WikiProject | null> {
  return null
}
