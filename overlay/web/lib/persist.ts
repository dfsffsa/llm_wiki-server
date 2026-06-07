/**
 * HTTP mode persistence: chat history in localStorage.
 * Review/lint are read-only — no server write path.
 */
import type { Conversation, DisplayMessage } from "@/stores/chat-store"
import type { LintItem } from "@/stores/lint-store"
import type { ReviewItem } from "@/stores/review-store"
import { normalizePath } from "@/lib/path-utils"

interface PersistedChatData {
  conversations: Conversation[]
  messages: DisplayMessage[]
}

function chatStorageKey(projectPath: string): string {
  return `llm-wiki-chat:${normalizePath(projectPath)}`
}

export async function saveChatHistory(
  projectPath: string,
  conversations: Conversation[],
  messages: DisplayMessage[],
): Promise<void> {
  const key = chatStorageKey(projectPath)
  const byConversation = new Map<string, DisplayMessage[]>()
  for (const msg of messages) {
    const list = byConversation.get(msg.conversationId) ?? []
    list.push(msg)
    byConversation.set(msg.conversationId, list)
  }
  for (const [convId, msgs] of byConversation) {
    byConversation.set(convId, msgs.slice(-100))
  }
  const trimmedMessages = [...byConversation.values()].flat()
  const payload: PersistedChatData = { conversations, messages: trimmedMessages }
  localStorage.setItem(key, JSON.stringify(payload))
}

export async function loadChatHistory(projectPath: string): Promise<PersistedChatData> {
  const key = chatStorageKey(projectPath)
  try {
    const raw = localStorage.getItem(key)
    if (!raw) return { conversations: [], messages: [] }
    const parsed = JSON.parse(raw) as PersistedChatData
    return {
      conversations: parsed.conversations ?? [],
      messages: parsed.messages ?? [],
    }
  } catch {
    return { conversations: [], messages: [] }
  }
}

export async function saveReviewItems(_projectPath: string, _items: ReviewItem[]): Promise<void> {
  // HTTP read-only mode
}

export async function loadReviewItems(_projectPath: string): Promise<ReviewItem[]> {
  return []
}

export async function saveLintItems(_projectPath: string, _items: LintItem[]): Promise<void> {
  // HTTP read-only mode
}

export async function loadLintItems(_projectPath: string): Promise<LintItem[]> {
  return []
}
