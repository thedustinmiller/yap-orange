/**
 * WASM Worker management for SPA mode.
 *
 * In SPA mode (no backend server), spawns a Dedicated Web Worker that runs
 * the Axum router + SQLite via WASM with OPFS persistence. The worker
 * handles API requests via postMessage instead of HTTP fetch.
 *
 * In Tauri or dev-with-server mode, this is a no-op.
 */

let worker: Worker | null = null
let requestId = 0
const pending = new Map<number, { resolve: (v: string) => void; reject: (e: Error) => void }>()
let readyPromise: Promise<void> | null = null

/**
 * Initialize the WASM worker. Returns true if SPA mode is active.
 */
export async function initWasmWorker(): Promise<boolean> {
  // Skip in Tauri — the embedded server handles API requests
  if ((window as any).__TAURI_INTERNALS__) {
    return false
  }

  // Skip if a backend server is available (dev mode with `cargo run -p yap-server`)
  try {
    const resp = await fetch('/health', { signal: AbortSignal.timeout(1000) })
    if (resp.ok) {
      console.log('[yap] Backend server detected — using server mode')
      return false
    }
  } catch {
    // Server not available — proceed with SPA/WASM mode
  }

  worker = new Worker(import.meta.env.BASE_URL + 'wasm-worker.js', { type: 'module' })

  readyPromise = new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error('WASM worker initialization timed out'))
    }, 30000) // 30s timeout for first-time OPFS + migration setup

    worker!.onmessage = (event) => {
      const data = event.data

      if (data.type === 'ready') {
        clearTimeout(timeout)
        resolve()
        // Switch to request handler after init
        worker!.onmessage = handleWorkerMessage
        return
      }

      if (data.type === 'error') {
        clearTimeout(timeout)
        reject(new Error(data.error))
        return
      }

      // During init, forward any request responses too
      handleWorkerMessage(event)
    }
  })

  try {
    await readyPromise
    console.log('[yap] WASM worker ready — SPA mode active')
    return true
  } catch (e) {
    console.error('[yap] WASM worker failed to initialize:', e)
    worker?.terminate()
    worker = null
    return false
  }
}

function handleWorkerMessage(event: MessageEvent) {
  const { id, result, error } = event.data
  const p = pending.get(id)
  if (!p) return
  pending.delete(id)

  if (error) {
    p.reject(new Error(error))
  } else {
    p.resolve(result)
  }
}

/**
 * Returns true if the WASM worker is active (SPA mode).
 */
export function isWasmMode(): boolean {
  return worker !== null
}

/**
 * Factory reset the WASM database: clear all data, re-run migrations,
 * and re-bootstrap with default seed data. Only works in SPA mode.
 */
export async function wasmFactoryReset(): Promise<void> {
  if (!worker) {
    throw new Error('WASM worker not initialized — not in SPA mode')
  }

  const id = ++requestId
  return new Promise<void>((resolve, reject) => {
    pending.set(id, {
      resolve: () => resolve(),
      reject: (e: Error) => reject(e),
    })
    worker!.postMessage({ id, type: 'factory_reset' })
  })
}

/**
 * Send a request to the WASM worker and get the response.
 * Returns a Response-like object matching the fetch() API shape.
 */
export async function wasmRequest(
  method: string,
  url: string,
  body?: string,
): Promise<Response> {
  if (!worker) {
    throw new Error('WASM worker not initialized')
  }

  const id = ++requestId
  const resultJson = await new Promise<string>((resolve, reject) => {
    pending.set(id, { resolve, reject })
    worker!.postMessage({ id, method, url, body: body || '' })
  })

  const result = JSON.parse(resultJson)

  // Null-body statuses (204, 304) cannot have a body per the Response spec.
  const nullBodyStatuses = [204, 205, 304]
  const responseBody = nullBodyStatuses.includes(result.status) ? null : result.body

  return new Response(responseBody, {
    status: result.status,
    headers: result.headers,
  })
}
