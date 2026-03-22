/**
 * Schema store — caches schema definitions loaded from the server.
 *
 * Module-level singleton using Svelte 5 $state runes.
 * Call loadSchemas() once at app init to populate.
 */

import { api } from './api'
import type { Schema } from './types'

// Module-level reactive state
let schemas = $state<Schema[]>([])
let loaded = $state(false)
let loading = $state(false)

/**
 * Load all schemas from the server. No-op if already loaded.
 */
export async function loadSchemas(): Promise<void> {
  if (loaded || loading) return
  loading = true
  try {
    schemas = await api.schemas.list()
    loaded = true
  } catch (err) {
    console.error('Failed to load schemas:', err)
  } finally {
    loading = false
  }
}

/**
 * Force reload schemas from server.
 */
export async function reloadSchemas(): Promise<void> {
  loading = true
  try {
    schemas = await api.schemas.list()
    loaded = true
  } catch (err) {
    console.error('Failed to reload schemas:', err)
  } finally {
    loading = false
  }
}

/**
 * Get a schema by name (exact match on schema.name).
 */
export function getSchema(name: string): Schema | undefined {
  return schemas.find((s) => s.name === name)
}

/**
 * Resolve a schema by type name, trying the server's namespace walk-up.
 * Falls back to local cache lookup if server call fails.
 */
export async function resolveSchema(
  typeName: string,
  fromNamespace?: string,
): Promise<Schema | null> {
  try {
    return await api.schemas.resolve(typeName, fromNamespace)
  } catch {
    console.warn('Schema resolve failed, using cache:', typeName)
    return getSchema(typeName) ?? null
  }
}

/**
 * Get all cached schemas (reactive — updates when loadSchemas() completes).
 */
export function getSchemas(): Schema[] {
  return schemas
}

export function isLoaded(): boolean { return loaded }
export function isLoading(): boolean { return loading }
