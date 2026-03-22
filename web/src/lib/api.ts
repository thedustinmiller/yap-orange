/**
 * Typed API client for yap-orange server
 */

import type {
  Atom,
  RenderedAtom,
  Block,
  BlockWithContent,
  Edge,
  CreateBlockRequest,
  CreateBlockResponse,
  UpdateBlockRequest,
  MoveBlockRequest,
  UpdateAtomRequest,
  CreateEdgeRequest,
  ResolveRequest,
  ResolveResponse,
  HealthResponse,
  Backlink,
  AtomGraph,
  ApiErrorResponse,
  Uuid,
  Schema,
  BenchmarkResults,
  SubtreeGraphResponse,
} from './types'

// ============================================
// Tauri / WASM mode integration
// ============================================

import { isWasmMode, wasmRequest } from '../sw-register'

/**
 * Detect whether the frontend is running inside a Tauri webview. If so,
 * retrieve the dynamic server port from the Rust backend and update BASE_URL.
 *
 * Call this once before mounting the app. In a normal browser context it is
 * a no-op, so the same build works for both web and desktop.
 */
export async function initApi(): Promise<void> {
  if ((window as any).__TAURI_INTERNALS__) {
    const { invoke } = await import('@tauri-apps/api/core')
    const port = await invoke<number>('get_server_port')
    BASE_URL = `http://localhost:${port}`
  }
}

// ============================================
// Error Handling
// ============================================

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly statusText: string,
    public readonly serverMessage?: string,
  ) {
    super(serverMessage || `${status} ${statusText}`)
    this.name = 'ApiError'
  }

  static async fromResponse(response: Response): Promise<ApiError> {
    let serverMessage: string | undefined
    try {
      const json = (await response.json()) as ApiErrorResponse
      serverMessage = json.error
    } catch {
      // Intentional: JSON parse fallback for error responses
    }
    return new ApiError(response.status, response.statusText, serverMessage)
  }
}

// ============================================
// Base Fetch Wrapper
// ============================================

let BASE_URL = '' // Empty → Vite proxy in browser dev; set by initApi() in Tauri

interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE'
  body?: unknown
  headers?: Record<string, string>
}

async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { method = 'GET', body, headers = {} } = options

  let response: Response

  if (isWasmMode()) {
    // SPA mode: route through the in-WASM Axum router via Dedicated Worker
    const bodyStr = body !== undefined ? JSON.stringify(body) : ''
    response = await wasmRequest(method || 'GET', path, bodyStr)
  } else {
    // Server mode: standard HTTP fetch (Tauri or dev proxy)
    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), 30_000)
    const config: RequestInit = {
      method,
      headers: {
        'Content-Type': 'application/json',
        ...headers,
      },
      signal: controller.signal,
    }

    if (body !== undefined) {
      config.body = JSON.stringify(body)
    }

    try {
      response = await fetch(`${BASE_URL}${path}`, config)
    } finally {
      clearTimeout(timeoutId)
    }
  }

  if (!response.ok) {
    throw await ApiError.fromResponse(response)
  }

  if (response.status === 204) {
    return undefined as T
  }

  return response.json() as Promise<T>
}

function get<T>(path: string): Promise<T> {
  return request<T>(path, { method: 'GET' })
}

function post<T>(path: string, body?: unknown): Promise<T> {
  return request<T>(path, { method: 'POST', body })
}

function put<T>(path: string, body?: unknown): Promise<T> {
  return request<T>(path, { method: 'PUT', body })
}

function del<T>(path: string): Promise<T> {
  return request<T>(path, { method: 'DELETE' })
}

// ============================================
// Health Check
// ============================================

async function health(): Promise<HealthResponse> {
  return get<HealthResponse>('/health')
}

// ============================================
// Block Operations
// ============================================

const blocks = {
  create(data: CreateBlockRequest): Promise<CreateBlockResponse> {
    return post<CreateBlockResponse>('/api/blocks', data)
  },

  get(id: Uuid): Promise<BlockWithContent> {
    return get<BlockWithContent>(`/api/blocks/${id}`)
  },

  list(params?: { namespace?: string; search?: string; lineage_id?: string; content_type?: string }): Promise<Block[]> {
    const parts: string[] = []
    if (params?.namespace) parts.push(`namespace=${encodeURIComponent(params.namespace)}`)
    if (params?.search) parts.push(`search=${encodeURIComponent(params.search)}`)
    if (params?.lineage_id) parts.push(`lineage_id=${encodeURIComponent(params.lineage_id)}`)
    if (params?.content_type) parts.push(`content_type=${encodeURIComponent(params.content_type)}`)
    const query = parts.length > 0 ? `?${parts.join('&')}` : ''
    return get<Block[]>(`/api/blocks${query}`)
  },

  children(id: Uuid): Promise<Block[]> {
    return get<Block[]>(`/api/blocks/${id}/children`)
  },

  orphans(): Promise<Block[]> {
    return get<Block[]>('/api/blocks/orphans')
  },

  update(id: Uuid, data: UpdateBlockRequest): Promise<Block> {
    return put<Block>(`/api/blocks/${id}`, data)
  },

  delete(id: Uuid): Promise<void> {
    return del<void>(`/api/blocks/${id}`)
  },

  deleteRecursive(id: Uuid): Promise<void> {
    return del<void>(`/api/blocks/${id}/recursive`)
  },

  restore(id: Uuid): Promise<Block> {
    return post<Block>(`/api/blocks/${id}/restore`)
  },

  move(id: Uuid, data: MoveBlockRequest): Promise<Block> {
    return post<Block>(`/api/blocks/${id}/move`, data)
  },
}

// ============================================
// Atom Operations
// ============================================

const atoms = {
  get(id: Uuid): Promise<Atom> {
    return get<Atom>(`/api/atoms/${id}`)
  },

  getRendered(id: Uuid): Promise<RenderedAtom> {
    return get<RenderedAtom>(`/api/atoms/${id}/rendered`)
  },

  update(id: Uuid, data: UpdateAtomRequest): Promise<Atom> {
    return put<Atom>(`/api/atoms/${id}`, data)
  },

  snapshot(atomId: Uuid): Promise<Atom> {
    return get<Atom>(`/api/atoms/snapshot/${atomId}`)
  },

  backlinks(id: Uuid): Promise<Backlink[]> {
    return get<Backlink[]>(`/api/atoms/${id}/backlinks`)
  },

  references(id: Uuid): Promise<Edge[]> {
    return get<Edge[]>(`/api/atoms/${id}/references`)
  },

  graph(id: Uuid): Promise<AtomGraph> {
    return get<AtomGraph>(`/api/atoms/${id}/graph`)
  },

  edges(id: Uuid): Promise<Edge[]> {
    return get<Edge[]>(`/api/atoms/${id}/edges`)
  },
}

// ============================================
// Edge Operations
// ============================================

const edges = {
  create(data: CreateEdgeRequest): Promise<Edge> {
    return post<Edge>('/api/edges', data)
  },

  delete(id: Uuid): Promise<void> {
    return del<void>(`/api/edges/${id}`)
  },
}

// ============================================
// Utility Operations
// ============================================

const roots = {
  list(): Promise<Block[]> {
    return get<Block[]>('/api/blocks/roots')
  },
}

async function resolve(data: ResolveRequest): Promise<ResolveResponse> {
  return post<ResolveResponse>('/api/resolve', data)
}

// ============================================
// Schema Operations
// ============================================

const schemas = {
  list(): Promise<Schema[]> {
    return get<Schema[]>('/api/schemas')
  },

  resolve(typeName: string, fromNamespace?: string): Promise<Schema> {
    return post<Schema>('/api/schemas/resolve', {
      type_name: typeName,
      from_namespace: fromNamespace,
    })
  },
}

// ============================================
// Debug Operations
// ============================================

const debug = {
  logs(sinceId: number = 0): Promise<any[]> {
    return get<any[]>(`/api/debug/logs?since=${sinceId}`)
  },

  runBenchmarks(suites?: string[], seed?: number): Promise<BenchmarkResults> {
    const body: Record<string, unknown> = {}
    if (suites && suites.length > 0) body.suites = suites
    if (seed !== undefined) body.seed = seed
    return post<BenchmarkResults>('/api/debug/benchmarks', body)
  },
}

// ============================================
// Graph Operations
// ============================================

const graph = {
  subtree(lineageIds: Uuid[]): Promise<SubtreeGraphResponse> {
    return post<SubtreeGraphResponse>('/api/graph/subtree', { lineage_ids: lineageIds })
  },
}

// ============================================
// Import / Export Operations
// ============================================

const importExport = {
  export(blockId: Uuid, includeKeys?: string[]): Promise<any> {
    let url = `/api/blocks/${blockId}/export`
    if (includeKeys && includeKeys.length > 0) {
      url += `?include_keys=${encodeURIComponent(includeKeys.join(','))}`
    }
    return get<any>(url)
  },
  import(parentId: Uuid, tree: any, mode: 'merge' | 'copy' = 'merge', matchBy?: string, globalLink?: boolean): Promise<any> {
    let url = `/api/blocks/${parentId}/import?mode=${mode}`
    if (matchBy) url += `&match_by=${matchBy}`
    if (globalLink) url += `&global_link=true`
    return post<any>(url, tree)
  },
  importAtRoot(tree: any, mode: 'merge' | 'copy' = 'merge', matchBy?: string, globalLink?: boolean): Promise<any> {
    let url = `/api/import?mode=${mode}`
    if (matchBy) url += `&match_by=${matchBy}`
    if (globalLink) url += `&global_link=true`
    return post<any>(url, tree)
  },
  propertyKeys(blockId: Uuid): Promise<string[]> {
    return get<string[]>(`/api/blocks/${blockId}/property-keys`)
  },
}

// ============================================
// Export API Client
// ============================================

export const api = {
  health,
  blocks,
  atoms,
  edges,
  roots,
  schemas,
  graph,
  debug,
  importExport,
  resolve,
}

export { blocks, atoms, edges, roots, schemas, graph, debug, importExport, resolve }

// Expose API on window for e2e tests (dev mode only)
if (import.meta.env.DEV) {
  ;(window as any).__yap = { api }
}
