#!/usr/bin/env tsx
import { autoIngest } from "@/lib/ingest"
import { normalizePath } from "@/lib/path-utils"
import { loadConfigFile, parseFlag } from "./load-config.js"
import { hydrateStoresFromConfig } from "./setup-stores.js"

async function main() {
  const project = parseFlag("--project")
  const source = parseFlag("--source")
  const configPath = parseFlag("--config")

  if (!project || !source || !configPath) {
    console.error("Usage: cmd-ingest.ts --project PATH --source FILE --config CONFIG.json")
    process.exit(1)
  }

  const config = loadConfigFile(configPath)
  const llmConfig = hydrateStoresFromConfig(config)

  console.log(`[ingest] project=${project}`)
  console.log(`[ingest] source=${source}`)
  console.log(`[ingest] model=${llmConfig.model}`)

  const written = await autoIngest(normalizePath(project), normalizePath(source), llmConfig)
  console.log(`[ingest] done — ${written.length} wiki file(s) written`)
  for (const file of written) {
    console.log(`  ${file}`)
  }
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err)
  process.exit(1)
})
