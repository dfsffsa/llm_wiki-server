import type { WikiProject } from "@/types/wiki"
import type {
  ApiConfig,
  EmbeddingConfig,
  GeneralConfig,
  LlmConfig,
  MultimodalConfig,
  OutputLanguage,
  ProviderConfigs,
  ProxyConfig,
  ScheduledImportConfig,
  SourceWatchConfig,
  SearchApiConfig,
} from "@/stores/wiki-store"
import { normalizeSourceWatchConfig } from "@/lib/source-watch-config"
import { normalizePath } from "@/lib/path-utils"

const STORAGE_KEY = "llm-wiki-app-state"
const RECENT_PROJECTS_KEY = "recentProjects"
const LAST_PROJECT_KEY = "lastProject"

type StoreData = Record<string, unknown>

function readAll(): StoreData {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return raw ? (JSON.parse(raw) as StoreData) : {}
  } catch {
    return {}
  }
}

function writeAll(data: StoreData): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(data))
}

const store = {
  async get<T>(key: string): Promise<T | undefined> {
    return readAll()[key] as T | undefined
  },
  async set(key: string, value: unknown): Promise<void> {
    const data = readAll()
    data[key] = value
    writeAll(data)
  },
  async delete(key: string): Promise<void> {
    const data = readAll()
    delete data[key]
    writeAll(data)
  },
  async save(): Promise<void> {
    // no-op — localStorage is synchronous
  },
}

async function getStore() {
  return store
}

export async function getRecentProjects(): Promise<WikiProject[]> {
  const s = await getStore()
  return (await s.get<WikiProject[]>(RECENT_PROJECTS_KEY)) ?? []
}

export async function getLastProject(): Promise<WikiProject | null> {
  const s = await getStore()
  return (await s.get<WikiProject>(LAST_PROJECT_KEY)) ?? null
}

export async function saveLastProject(project: WikiProject): Promise<void> {
  const s = await getStore()
  await s.set(LAST_PROJECT_KEY, project)
  await addToRecentProjects(project)
}

export async function addToRecentProjects(project: WikiProject): Promise<void> {
  const s = await getStore()
  const existing = (await s.get<WikiProject[]>(RECENT_PROJECTS_KEY)) ?? []
  const filtered = existing.filter((p) => p.path !== project.path)
  await s.set(RECENT_PROJECTS_KEY, [project, ...filtered].slice(0, 10))
}

const LLM_CONFIG_KEY = "llmConfig"
const PROVIDER_CONFIGS_KEY = "providerConfigs"
const ACTIVE_PRESET_KEY = "activePresetId"

export async function saveLlmConfig(config: LlmConfig): Promise<void> {
  await (await getStore()).set(LLM_CONFIG_KEY, config)
}

export async function loadLlmConfig(): Promise<LlmConfig | null> {
  return ((await getStore()).get<LlmConfig>(LLM_CONFIG_KEY)) ?? null
}

export async function saveProviderConfigs(configs: ProviderConfigs): Promise<void> {
  await (await getStore()).set(PROVIDER_CONFIGS_KEY, configs)
}

export async function loadProviderConfigs(): Promise<ProviderConfigs | null> {
  return ((await getStore()).get<ProviderConfigs>(PROVIDER_CONFIGS_KEY)) ?? null
}

export async function saveActivePresetId(id: string | null): Promise<void> {
  await (await getStore()).set(ACTIVE_PRESET_KEY, id)
}

export async function loadActivePresetId(): Promise<string | null> {
  return ((await getStore()).get<string | null>(ACTIVE_PRESET_KEY)) ?? null
}

const SEARCH_API_KEY = "searchApiConfig"

export async function saveSearchApiConfig(config: SearchApiConfig): Promise<void> {
  await (await getStore()).set(SEARCH_API_KEY, config)
}

export async function loadSearchApiConfig(): Promise<SearchApiConfig | null> {
  return ((await getStore()).get<SearchApiConfig>(SEARCH_API_KEY)) ?? null
}

const EMBEDDING_KEY = "embeddingConfig"

export async function saveEmbeddingConfig(config: EmbeddingConfig): Promise<void> {
  await (await getStore()).set(EMBEDDING_KEY, config)
}

export async function loadEmbeddingConfig(): Promise<EmbeddingConfig | null> {
  return ((await getStore()).get<EmbeddingConfig>(EMBEDDING_KEY)) ?? null
}

const MULTIMODAL_KEY = "multimodalConfig"

export async function saveMultimodalConfig(config: MultimodalConfig): Promise<void> {
  await (await getStore()).set(MULTIMODAL_KEY, config)
}

export async function loadMultimodalConfig(): Promise<MultimodalConfig | null> {
  return ((await getStore()).get<MultimodalConfig>(MULTIMODAL_KEY)) ?? null
}

const PROXY_CONFIG_KEY = "proxyConfig"

export async function saveProxyConfig(config: ProxyConfig): Promise<void> {
  const s = await getStore()
  await s.set(PROXY_CONFIG_KEY, config)
  await s.save()
}

export async function loadProxyConfig(): Promise<ProxyConfig | null> {
  return ((await getStore()).get<ProxyConfig>(PROXY_CONFIG_KEY)) ?? null
}

const API_CONFIG_KEY = "apiConfig"

export async function saveApiConfig(config: ApiConfig): Promise<void> {
  const s = await getStore()
  await s.set(API_CONFIG_KEY, config)
  await s.save()
}

export async function loadApiConfig(): Promise<ApiConfig | null> {
  return ((await getStore()).get<ApiConfig>(API_CONFIG_KEY)) ?? null
}

const GENERAL_CONFIG_KEY = "generalConfig"

export const DEFAULT_GENERAL_CONFIG: GeneralConfig = {
  autostart: false,
  closeBehavior: "minimize",
}

export function normalizeGeneralConfig(config?: Partial<GeneralConfig> | null): GeneralConfig {
  const closeBehavior = config?.closeBehavior
  return {
    autostart: typeof config?.autostart === "boolean" ? config.autostart : DEFAULT_GENERAL_CONFIG.autostart,
    closeBehavior:
      closeBehavior === "ask" || closeBehavior === "minimize" || closeBehavior === "exit"
        ? closeBehavior
        : DEFAULT_GENERAL_CONFIG.closeBehavior,
  }
}

export async function saveGeneralConfig(config: GeneralConfig): Promise<void> {
  await (await getStore()).set(GENERAL_CONFIG_KEY, normalizeGeneralConfig(config))
}

export async function loadGeneralConfig(): Promise<GeneralConfig> {
  const config = await (await getStore()).get<Partial<GeneralConfig>>(GENERAL_CONFIG_KEY)
  return normalizeGeneralConfig(config)
}

const SCHEDULED_IMPORT_KEY_PREFIX = "scheduledImportConfig:"

function scheduledImportKey(projectPath: string): string {
  return `${SCHEDULED_IMPORT_KEY_PREFIX}${normalizePath(projectPath)}`
}

const SCHEDULED_IMPORT_GLOBAL_KEY = "scheduledImportConfig"

export async function saveScheduledImportConfig(
  projectPath: string,
  config: ScheduledImportConfig,
): Promise<void> {
  const s = await getStore()
  await s.set(scheduledImportKey(projectPath), config)
  await s.save()
}

export async function loadScheduledImportConfig(
  projectPath: string,
): Promise<ScheduledImportConfig | null> {
  const s = await getStore()
  const perProject = await s.get<ScheduledImportConfig>(scheduledImportKey(projectPath))
  if (perProject) return perProject
  const legacy = await s.get<ScheduledImportConfig>(SCHEDULED_IMPORT_GLOBAL_KEY)
  if (legacy) {
    await s.set(scheduledImportKey(projectPath), legacy)
    await s.delete(SCHEDULED_IMPORT_GLOBAL_KEY)
    await s.save()
    return legacy
  }
  return null
}

export async function removeFromRecentProjects(path: string): Promise<void> {
  const s = await getStore()
  const existing = (await s.get<WikiProject[]>(RECENT_PROJECTS_KEY)) ?? []
  await s.set(
    RECENT_PROJECTS_KEY,
    existing.filter((p) => p.path !== path),
  )
  const last = await s.get<WikiProject>(LAST_PROJECT_KEY)
  if (last && last.path === path) {
    await s.delete(LAST_PROJECT_KEY)
  }
}

const LANGUAGE_KEY = "language"

export async function saveLanguage(lang: string): Promise<void> {
  await (await getStore()).set(LANGUAGE_KEY, lang)
}

export async function loadLanguage(): Promise<string | null> {
  return ((await getStore()).get<string>(LANGUAGE_KEY)) ?? null
}

const THEME_KEY = "theme"

export async function saveTheme(theme: "light" | "dark" | "system"): Promise<void> {
  await (await getStore()).set(THEME_KEY, theme)
}

export async function loadTheme(): Promise<"light" | "dark" | "system" | null> {
  return ((await getStore()).get<"light" | "dark" | "system">(THEME_KEY)) ?? null
}

const OUTPUT_LANGUAGE_KEY = "outputLanguage"
const PROJECT_OUTPUT_LANGUAGE_KEY = "projectOutputLanguages"
const PROJECT_FILE_SYNC_KEY = "projectFileSyncEnabled"
const SOURCE_WATCH_CONFIG_KEY = "sourceWatchConfig"

export async function saveOutputLanguage(
  lang: OutputLanguage,
  projectId?: string,
): Promise<void> {
  const s = await getStore()
  if (projectId) {
    const existing =
      (await s.get<Record<string, OutputLanguage>>(PROJECT_OUTPUT_LANGUAGE_KEY)) ?? {}
    await s.set(PROJECT_OUTPUT_LANGUAGE_KEY, { ...existing, [projectId]: lang })
  }
  await s.set(OUTPUT_LANGUAGE_KEY, lang)
}

export async function loadOutputLanguage(projectId?: string): Promise<OutputLanguage | null> {
  const s = await getStore()
  if (projectId) {
    const projectLanguages = await s.get<Record<string, OutputLanguage>>(
      PROJECT_OUTPUT_LANGUAGE_KEY,
    )
    return projectLanguages?.[projectId] ?? null
  }
  return (await s.get<OutputLanguage>(OUTPUT_LANGUAGE_KEY)) ?? null
}

export async function saveProjectFileSyncEnabled(
  enabled: boolean,
  projectId?: string,
): Promise<void> {
  const s = await getStore()
  const existing = (await s.get<Record<string, boolean>>(PROJECT_FILE_SYNC_KEY)) ?? {}
  if (projectId) {
    await s.set(PROJECT_FILE_SYNC_KEY, { ...existing, [projectId]: enabled })
    return
  }
  await s.set(PROJECT_FILE_SYNC_KEY, { ...existing, default: enabled })
}

export async function loadProjectFileSyncEnabled(projectId?: string): Promise<boolean> {
  const s = await getStore()
  const settings = await s.get<Record<string, boolean>>(PROJECT_FILE_SYNC_KEY)
  if (projectId && settings && typeof settings[projectId] === "boolean") {
    return settings[projectId]
  }
  if (settings && typeof settings.default === "boolean") {
    return settings.default
  }
  return true
}

export async function saveSourceWatchConfig(
  config: SourceWatchConfig,
  projectId?: string,
): Promise<void> {
  const s = await getStore()
  const normalized = normalizeSourceWatchConfig(config)
  const existing =
    (await s.get<Record<string, SourceWatchConfig>>(SOURCE_WATCH_CONFIG_KEY)) ?? {}
  await s.set(SOURCE_WATCH_CONFIG_KEY, {
    ...existing,
    [projectId ?? "default"]: normalized,
  })
  await s.save()
}

export async function loadSourceWatchConfig(projectId?: string): Promise<SourceWatchConfig> {
  const s = await getStore()
  const settings = await s.get<Record<string, SourceWatchConfig>>(SOURCE_WATCH_CONFIG_KEY)
  const config = projectId ? settings?.[projectId] : undefined
  if (config) return normalizeSourceWatchConfig(config)
  if (settings?.default) return normalizeSourceWatchConfig(settings.default)
  const legacyEnabled = await loadProjectFileSyncEnabled(projectId)
  return normalizeSourceWatchConfig({ enabled: legacyEnabled })
}

const UPDATE_CHECK_STATE_KEY = "updateCheckState"

export interface PersistedUpdateCheckState {
  enabled: boolean
  lastCheckedAt: number | null
  dismissedVersion: string | null
}

export async function saveUpdateCheckState(state: PersistedUpdateCheckState): Promise<void> {
  await (await getStore()).set(UPDATE_CHECK_STATE_KEY, state)
}

export async function loadUpdateCheckState(): Promise<PersistedUpdateCheckState | null> {
  return ((await getStore()).get<PersistedUpdateCheckState>(UPDATE_CHECK_STATE_KEY)) ?? null
}
