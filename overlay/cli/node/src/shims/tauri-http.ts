export async function fetch(_input: string, init?: RequestInit): Promise<Response> {
  return globalThis.fetch(_input, init)
}

export function isFetchNetworkError(err: unknown): boolean {
  return err instanceof Error && /network|fetch|ECONNREFUSED|ENOTFOUND/i.test(err.message)
}
