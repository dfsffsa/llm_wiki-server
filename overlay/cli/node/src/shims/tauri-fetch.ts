export async function getHttpFetch(): Promise<typeof fetch> {
  return globalThis.fetch.bind(globalThis)
}

export function isFetchNetworkError(err: unknown): boolean {
  return err instanceof Error && /network|fetch|ECONNREFUSED|ENOTFOUND/i.test(err.message)
}
