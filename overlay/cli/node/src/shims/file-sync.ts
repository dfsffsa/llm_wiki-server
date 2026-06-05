export function startProjectFileWatcher() {
  return Promise.resolve({ queue: { version: 1, tasks: [] }, changedTasks: [] })
}
export function stopProjectFileWatcher() {
  return Promise.resolve()
}
export function rescanProjectFiles() {
  return Promise.resolve({ queue: { version: 1, tasks: [] }, changedTasks: [] })
}
export function getFileChangeQueue() {
  return Promise.resolve({ version: 1, tasks: [] })
}
export function retryFileChangeTask() {
  return Promise.resolve({ version: 1, tasks: [] })
}
export function ignoreFileChangeTask() {
  return Promise.resolve({ version: 1, tasks: [] })
}
