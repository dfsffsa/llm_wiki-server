/**
 * HTTP backend: proxy streamChat through llm-wiki-server (avoids browser CORS).
 */
import type { LlmConfig } from "@/stores/wiki-store"
import type { ChatMessage, RequestOverrides } from "@/lib/llm-providers"
import { isFetchNetworkError } from "@/lib/tauri-fetch"
import { getBackendClient } from "../backend-client"
import { getActiveProject } from "../path-helpers"

export type { ChatMessage, RequestOverrides } from "@/lib/llm-providers"
export { isFetchNetworkError }

export interface StreamCallbacks {
  onToken: (token: string) => void
  onReasoningToken?: (token: string) => void
  onDone: () => void
  onError: (error: Error) => void
}

export async function streamChat(
  _config: LlmConfig,
  messages: ChatMessage[],
  callbacks: StreamCallbacks,
  signal?: AbortSignal,
  _requestOverrides?: RequestOverrides,
): Promise<void> {
  const { onToken, onReasoningToken, onDone, onError } = callbacks
  const { id } = await getActiveProject()
  let finished = false

  const finish = () => {
    if (finished) return
    finished = true
    onDone()
  }

  try {
    const res = await getBackendClient().chatStream(id, messages, signal)
    if (!res.ok || !res.body) {
      const errBody = await res.text().catch(() => "")
      onError(new Error(errBody || `Chat request failed (${res.status})`))
      return
    }

    const reader = res.body.getReader()
    const decoder = new TextDecoder()
    let buffer = ""

    while (true) {
      if (signal?.aborted) {
        reader.cancel().catch(() => {})
        onError(new Error("Aborted"))
        return
      }
      const { done, value } = await reader.read()
      if (done) break
      buffer += decoder.decode(value, { stream: true })

      let boundary = buffer.indexOf("\n\n")
      while (boundary !== -1) {
        const chunk = buffer.slice(0, boundary)
        buffer = buffer.slice(boundary + 2)
        if (parseSseChunk(chunk, onToken, onReasoningToken, finish, onError)) return
        boundary = buffer.indexOf("\n\n")
      }
    }

    if (buffer.trim()) {
      if (parseSseChunk(buffer, onToken, onReasoningToken, finish, onError)) return
    }
    finish()
  } catch (err) {
    onError(err instanceof Error ? err : new Error(String(err)))
  }
}

/** @returns true if stream should stop (error or done). */
function parseSseChunk(
  chunk: string,
  onToken: (t: string) => void,
  onReasoningToken: ((t: string) => void) | undefined,
  onDone: () => void,
  onError: (e: Error) => void,
): boolean {
  for (const line of chunk.split("\n")) {
    if (!line.startsWith("data: ")) continue
    const raw = line.slice(6).trim()
    if (!raw) continue
    try {
      const parsed = JSON.parse(raw) as { event?: string; data?: Record<string, unknown> }
      switch (parsed.event) {
        case "token":
          if (typeof parsed.data?.token === "string") onToken(parsed.data.token)
          break
        case "reasoning":
          if (typeof parsed.data?.token === "string") onReasoningToken?.(parsed.data.token)
          break
        case "done":
          onDone()
          return true
        case "error":
          onError(new Error(String(parsed.data?.message ?? "Chat stream error")))
          return true
        default:
          break
      }
    } catch {
      // ignore malformed SSE lines
    }
  }
  return false
}
