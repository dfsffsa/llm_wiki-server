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

// stdout is the SSE wire — every byte the client receives is parsed as an SSE
// event by overlay/web/lib/llm-client.ts. Any stray write (an undici banner, a
// deprecation warning, a forgotten console.log, a stack trace) would corrupt
// the stream. So we capture the real stdout for our own use, then redirect
// everything else that would hit stdout to stderr instead.
const rawStdoutWrite: typeof process.stdout.write =
  process.stdout.write.bind(process.stdout)
process.stdout.write = ((chunk: any, ...rest: any[]) =>
  process.stderr.write(chunk, ...rest)) as typeof process.stdout.write
// console.* and any code logging through stdout now land on stderr (server log),
// never on the SSE wire.
console.log = (...args: unknown[]) => process.stderr.write(`${args.join(" ")}\n`)
console.warn = (...args: unknown[]) => process.stderr.write(`${args.join(" ")}\n`)
console.error = (...args: unknown[]) => process.stderr.write(`${args.join(" ")}\n`)
console.info = (...args: unknown[]) => process.stderr.write(`${args.join(" ")}\n`)
console.debug = (...args: unknown[]) => process.stderr.write(`${args.join(" ")}\n`)

function writeSse(event: string, data: unknown): void {
  const payload = JSON.stringify({ event, data })
  rawStdoutWrite(`data: ${payload}\n\n`)
}

// Safety net for failures outside the main() promise chain: an uncaught
// synchronous exception or an unhandled rejection would otherwise kill the
// process silently — the client would see the stream truncate with no `done`
// and no way to tell a clean end from a crash. Emit an `error` frame first.
function emitFatalAndExit(message: string): void {
  writeSse("error", { message })
  process.exit(1)
}
process.on("uncaughtException", (err) => {
  emitFatalAndExit(err instanceof Error ? err.message : String(err))
})
process.on("unhandledRejection", (reason) => {
  emitFatalAndExit(reason instanceof Error ? reason.message : String(reason))
})

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

  // Force exit: tsx's loader and undici (fetch) keep the event loop alive after
  // streaming finishes, so stdout never reaches EOF and the HTTP server's
  // response copy hangs. Exit explicitly to flush the SSE `done` event.
  process.exit(0)
}

main().catch((err) => {
  writeSse("error", { message: err instanceof Error ? err.message : String(err) })
  process.exit(1)
})
