/**
 * Settings store — persists UI preferences as a regular block under settings::ui.
 *
 * All settings live in ONE block's properties at namespace `settings`, name `ui`,
 * with content_type "setting". This means one network fetch on load and one
 * write per flush — no per-key API calls.
 *
 * Module-level singleton using Svelte 5 $state runes (requires .svelte.ts extension).
 *
 * Key names used by App.svelte:
 *   "sidebar_expanded"   — string[]  block IDs expanded in the sidebar
 *   "outliner_expanded"  — string[]  block IDs expanded in the outliner
 *   "last_location"      — { block_id: string; namespace: string }
 */

import { api } from './api'
import type { Uuid, Properties } from './types'

// Reactive module-level cache — $state so that $derived(getSetting(...)) works
let _blockId: Uuid | null = null
let _lineageId: Uuid | null = null
let _properties: Properties = $state({})
let _loaded = false

// Single debounce timer — any setSetting call resets it
let _flushTimer: ReturnType<typeof setTimeout> | null = null

const NS = 'settings'
const NAME = 'ui'

/**
 * Load settings from the server. No-op if already loaded.
 * On first run with no settings block, silently initialises with empty properties.
 */
export async function loadSettings(): Promise<void> {
  if (_loaded) return
  try {
    const result = await api.resolve({ path: `${NS}::${NAME}` })
    const block = await api.blocks.get(result.block_id)
    _blockId = block.id
    _lineageId = block.lineage_id
    Object.assign(_properties, block.properties ?? {})
  } catch {
    // Intentional: settings::ui may not exist yet on first run
    _blockId = null
    _lineageId = null
  }
  _loaded = true
}

/** Ensure the settings block exists, creating it if needed. */
async function ensureBlock(): Promise<{ blockId: Uuid; lineageId: Uuid }> {
  if (_blockId && _lineageId) return { blockId: _blockId, lineageId: _lineageId }
  const created = await api.blocks.create({
    namespace: NS,
    name: NAME,
    content: '',
    content_type: 'setting',
  })
  const block = await api.blocks.get(created.block_id)
  _blockId = block.id
  _lineageId = block.lineage_id
  Object.assign(_properties, block.properties ?? {})
  return { blockId: block.id, lineageId: block.lineage_id }
}

async function flushSettings(): Promise<void> {
  try {
    const { lineageId } = await ensureBlock()
    await api.atoms.update(lineageId, {
      content: '',
      properties: { ..._properties },
    })
  } catch (err) {
    console.error('Failed to persist settings:', err)
  }
}

/**
 * Get a setting value by key. Returns undefined if not set.
 * Reactive — reads from a $state object, so $derived(getSetting(...)) works.
 */
export function getSetting<T>(key: string): T | undefined {
  return _properties[key] as T | undefined
}

/**
 * Set a setting value. Updates the reactive cache immediately and
 * schedules a debounced write to the server.
 */
export function setSetting(key: string, value: unknown, debounceMs = 2000): void {
  _properties[key] = value

  if (_flushTimer !== null) clearTimeout(_flushTimer)
  _flushTimer = setTimeout(() => {
    _flushTimer = null
    flushSettings()
  }, debounceMs)
}

export function isLoaded(): boolean {
  return _loaded
}

/**
 * Immediately flush any pending settings to the server.
 * Cancels any outstanding debounce timer. Returns when the write completes.
 */
export async function flushSettingsNow(): Promise<void> {
  if (_flushTimer !== null) {
    clearTimeout(_flushTimer)
    _flushTimer = null
  }
  if (_loaded && (_blockId || Object.keys(_properties).length > 0)) {
    await flushSettings()
  }
}

/**
 * Merge properties into the reactive cache.
 * Called by SettingsView after each save so getSetting() returns fresh values.
 */
export function updateCache(properties: Properties): void {
  Object.assign(_properties, properties)
}
