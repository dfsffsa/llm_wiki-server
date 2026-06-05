import type { EmbeddingConfig, LlmConfig, MultimodalConfig } from "@/stores/wiki-store"
import { useWikiStore } from "@/stores/wiki-store"

const DEFAULT_MULTIMODAL: MultimodalConfig = {
  enabled: false,
  useMainLlm: true,
  provider: "custom",
  apiKey: "",
  model: "",
  ollamaUrl: "http://localhost:11434",
  customEndpoint: "",
  azureApiVersion: "2024-10-21",
  apiMode: "chat_completions",
  concurrency: 4,
}

export function hydrateStoresFromConfig(config: Record<string, unknown>): LlmConfig {
  const llmConfig = config.llmConfig as LlmConfig
  if (!llmConfig?.model || !llmConfig?.apiKey) {
    throw new Error("config.llmConfig with model and apiKey is required")
  }

  useWikiStore.getState().setLlmConfig(llmConfig)

  if (config.embeddingConfig) {
    useWikiStore.getState().setEmbeddingConfig(config.embeddingConfig as EmbeddingConfig)
  }

  useWikiStore.getState().setMultimodalConfig(
    (config.multimodalConfig as MultimodalConfig | undefined) ?? DEFAULT_MULTIMODAL,
  )
  useWikiStore.getState().setOutputLanguage("auto")

  return llmConfig
}
