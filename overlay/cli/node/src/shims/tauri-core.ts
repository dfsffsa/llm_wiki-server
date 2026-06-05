import { spawnSync } from "node:child_process"
import fs from "node:fs"
import os from "node:os"
import path from "node:path"

const LLM_WIKI_BIN = process.env.LLM_WIKI_BIN ?? "llm-wiki"

function runVector(args: string[], stdin?: string): unknown {
  const result = spawnSync(LLM_WIKI_BIN, ["vector", ...args], {
    input: stdin,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  })
  if (result.error) {
    throw result.error
  }
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || `llm-wiki vector failed (${result.status})`)
  }
  const out = (result.stdout ?? "").trim()
  if (!out) return null
  try {
    return JSON.parse(out)
  } catch {
    return out
  }
}

export async function invoke<T = unknown>(cmd: string, args: Record<string, unknown>): Promise<T> {
  switch (cmd) {
    case "vector_upsert_chunks": {
      const payload = JSON.stringify({ chunks: args.chunks ?? [] })
      runVector(
        [
          "upsert-chunks",
          "--project",
          String(args.projectPath),
          "--page-id",
          String(args.pageId),
        ],
        payload,
      )
      return undefined as T
    }
    case "vector_delete_page":
      runVector([
        "delete-page",
        "--project",
        String(args.projectPath),
        "--page-id",
        String(args.pageId),
      ])
      return undefined as T
    case "vector_count_chunks": {
      const out = runVector(["count-chunks", "--project", String(args.projectPath)]) as string
      return Number(out) as T
    }
    case "vector_legacy_row_count":
      return 0 as T
    case "vector_drop_legacy":
      return undefined as T
    case "vector_search_chunks":
      return [] as T
    default:
      throw new Error(`Unsupported Tauri invoke in CLI: ${cmd}`)
  }
}

export function convertFileSrc(filePath: string, _protocol: string): string {
  return pathToFileUrl(filePath)
}

function pathToFileUrl(p: string): string {
  const resolved = path.resolve(p)
  const prefix = os.platform() === "win32" ? "file:///" : "file://"
  return prefix + resolved.split(path.sep).join("/")
}
