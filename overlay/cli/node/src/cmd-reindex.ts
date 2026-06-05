#!/usr/bin/env tsx
import { embedAllPages } from "@/lib/embedding"
import type { EmbeddingConfig } from "@/stores/wiki-store"
import { normalizePath } from "@/lib/path-utils"
import { loadConfigFile, parseFlag } from "./load-config.js"
import { hydrateStoresFromConfig } from "./setup-stores.js"

async function main() {
  const project = parseFlag("--project")
  const configPath = parseFlag("--config")

  if (!project || !configPath) {
    console.error("Usage: cmd-reindex.ts --project PATH --config CONFIG.json")
    process.exit(1)
  }

  const config = loadConfigFile(configPath)
  hydrateStoresFromConfig(config)

  const emb = config.embeddingConfig as EmbeddingConfig | undefined
  if (!emb?.enabled) {
    throw new Error("embeddingConfig.enabled must be true in config for vector reindex")
  }

  console.log(`[reindex] project=${project}`)
  const total = await embedAllPages(normalizePath(project), emb, (done, all) => {
    if (done % 5 === 0 || done === all) {
      console.log(`[reindex] ${done}/${all}`)
    }
  })
  console.log(`[reindex] embedded ${total} page(s)`)
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err)
  process.exit(1)
})
