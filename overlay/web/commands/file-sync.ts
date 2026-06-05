import type { SourceWatchConfig } from "@/stores/wiki-store"

export type FileChangeKind = "created" | "modified" | "deleted"
export type FileChangeStatus = "pending" | "processing" | "done" | "failed" | "superseded"

export interface FileChangeTask {
  id: string
  projectId: string
  path: string
  kind: FileChangeKind
  status: FileChangeStatus
  hashBefore?: string | null
  hashAfter?: string | null
  size?: number | null
  mtimeMs?: number | null
  createdAt: number
  updatedAt: number
  retryCount: number
  error?: string | null
  needsRerun: boolean
}

export interface FileChangeQueue {
  version: number
  tasks: FileChangeTask[]
}

export interface FileChangeRescanResult {
  queue: FileChangeQueue
  changedTasks: FileChangeTask[]
}

export interface FileSyncPayload {
  projectId: string
  tasks: FileChangeTask[]
}

const EMPTY_QUEUE: FileChangeQueue = { version: 1, tasks: [] }

export function startProjectFileWatcher(
  _projectId: string,
  _projectPath: string,
  _sourceWatchConfig?: SourceWatchConfig,
): Promise<FileChangeRescanResult> {
  return Promise.resolve({ queue: EMPTY_QUEUE, changedTasks: [] })
}

export function stopProjectFileWatcher(): Promise<void> {
  return Promise.resolve()
}

export function rescanProjectFiles(
  _projectId: string,
  _projectPath: string,
  _sourceWatchConfig?: SourceWatchConfig,
): Promise<FileChangeRescanResult> {
  return Promise.resolve({ queue: EMPTY_QUEUE, changedTasks: [] })
}

export function getFileChangeQueue(_projectPath: string): Promise<FileChangeQueue> {
  return Promise.resolve(EMPTY_QUEUE)
}

export function retryFileChangeTask(
  _projectId: string,
  _projectPath: string,
  _taskId: string,
): Promise<FileChangeQueue> {
  return Promise.resolve(EMPTY_QUEUE)
}

export function ignoreFileChangeTask(
  _projectId: string,
  _projectPath: string,
  _taskId: string,
): Promise<FileChangeQueue> {
  return Promise.resolve(EMPTY_QUEUE)
}
