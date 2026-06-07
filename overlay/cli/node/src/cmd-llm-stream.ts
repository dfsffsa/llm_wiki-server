#!/usr/bin/env tsx
/**
 * Headless LLM streaming for HTTP Chat proxy.
 * Reads JSON from stdin, writes SSE events to stdout.
 *
 * Usage:
 *   echo '{"messages":[...]}' | npx tsx cmd-llm-stream.ts --config /path/llm.json
 */
import { streamChat, type ChatMessage } from "@/lib/llm-client"
import type { LlmConfig } from "@/stores/wiki-store"
import { useWikiStore } from "@/stores/wiki-store"
import { loadConfigFile, parseFlag } from "./load-config.js"

interface StreamRequest {
  messages: ChatMessage[]
}

function writeSse(event: string, data: unknown): void {
  const payload = JSON.stringify({ event, data })
  process.stdout.write(`data: ${payload}\n\n`)
}

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = []
  for await (const chunk of process.stdin) {
    chunks.push(chunk as Buffer)
  }
  return Buffer.concat(chunks).toString("utf8")
}

async function main() {
  const configPath = parseFlag("--config")
  if (!configPath) {
    writeSse("error", { message: "Missing --config" })
    process.exit(1)
  }

  const raw = await readStdin()
  let body: StreamRequest
  try {
    body = JSON.parse(raw) as StreamRequest
  } catch {
    writeSse("error", { message: "Invalid JSON on stdin" })
    process.exit(1)
  }

  if (!Array.isArray(body.messages) || body.messages.length === 0) {
    writeSse("error", { message: "messages array is required" })
    process.exit(1)
  }

  const config = loadConfigFile(configPath)
  const llmConfig = config.llmConfig as LlmConfig
  if (!llmConfig?.model) {
    writeSse("error", { message: "config.llmConfig.model is required" })
    process.exit(1)
  }
  useWikiStore.getState().setLlmConfig(llmConfig)

  await streamChat(
    llmConfig,
    body.messages,
    {
      onToken: (token) => writeSse("token", { token }),
      onReasoningToken: (token) => writeSse("reasoning", { token }),
      onDone: () => writeSse("done", {}),
      onError: (err) => {
        writeSse("error", { message: err.message })
        process.exit(1)
      },
    },
  )
}

main().catch((err) => {
  writeSse("error", { message: err instanceof Error ? err.message : String(err) })
  process.exit(1)
})
