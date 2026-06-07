import fs from "node:fs"
import path from "node:path"

export function loadConfigFile(configPath: string): Record<string, unknown> {
  const resolved = resolveConfigPath(configPath)
  const raw = fs.readFileSync(resolved, "utf8")
  const parsed = JSON.parse(raw) as Record<string, unknown>
  expandEnv(parsed)
  return parsed
}

/** Resolve relative paths against LLM_WIKI_REPO (Node cwd is overlay/cli/node). */
function resolveConfigPath(configPath: string): string {
  if (path.isAbsolute(configPath)) return configPath
  const repo = process.env.LLM_WIKI_REPO
  if (repo) {
    const fromRepo = path.join(repo, configPath)
    if (fs.existsSync(fromRepo)) return fromRepo
  }
  return path.resolve(process.cwd(), configPath)
}

function expandEnv(value: unknown): void {
  if (typeof value === "string") {
    return
  }
  if (Array.isArray(value)) {
    for (const item of value) expandEnv(item)
    return
  }
  if (value && typeof value === "object") {
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      if (typeof v === "string" && v.startsWith("${") && v.endsWith("}")) {
        const key = v.slice(2, -1)
        const env = process.env[key]
        if (env) (value as Record<string, unknown>)[k] = env
      } else {
        expandEnv(v)
      }
    }
  }
}

export function parseFlag(name: string): string | undefined {
  const idx = process.argv.indexOf(name)
  if (idx === -1 || idx + 1 >= process.argv.length) return undefined
  return process.argv[idx + 1]
}
