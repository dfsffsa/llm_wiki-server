import fs from "node:fs"

export function loadConfigFile(path: string): Record<string, unknown> {
  const raw = fs.readFileSync(path, "utf8")
  const parsed = JSON.parse(raw) as Record<string, unknown>
  expandEnv(parsed)
  return parsed
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
