/**
 * Blob URL helper for media views.
 *
 * In server/desktop mode, returns a direct HTTP URL.
 * In WASM mode, fetches the file via the worker as base64,
 * decodes to a Blob, and returns an ObjectURL.
 *
 * ObjectURLs are cached by hash to avoid re-fetching.
 */

import { isWasmMode } from '../sw-register'
import { api } from './api'

const cache = new Map<string, string>()

/**
 * Get a usable URL for a file hash. In server mode this is synchronous
 * (returns a direct URL). In WASM mode this is async (fetches + creates ObjectURL).
 *
 * Use `resolveFileUrl()` in reactive contexts — it returns a promise.
 */
export async function resolveFileUrl(hash: string, mime?: string): Promise<string> {
  if (!isWasmMode()) {
    return api.files.url(hash, mime)
  }

  // Check cache
  const cached = cache.get(hash)
  if (cached) return cached

  // Fetch via worker and create ObjectURL
  try {
    const blob = await api.files.download(hash, mime)
    const url = URL.createObjectURL(blob)
    cache.set(hash, url)
    return url
  } catch {
    return '' // file not found
  }
}

/**
 * Synchronous URL getter — works in server/desktop mode only.
 * In WASM mode, returns cached ObjectURL or empty string (call resolveFileUrl first).
 */
export function getFileUrl(hash: string, mime?: string): string {
  if (!isWasmMode()) {
    return api.files.url(hash, mime)
  }
  return cache.get(hash) ?? ''
}
