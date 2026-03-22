/**
 * Outliner instance store — manages multiple outliner tabs.
 *
 * Each outliner has its own per-instance state (expanded blocks, navigation context, etc.).
 * The "active" outliner syncs bidirectionally with appState so that consumer panels
 * (Backlinks, Graph, Properties) continue reading from appState unchanged.
 *
 * IDs are random hex strings (not sequential) so restored layouts never collide
 * with newly created outliners.
 */

import { SvelteSet } from 'svelte/reactivity'
import type { Uuid, BlockWithContent } from './types'
import type { Breadcrumb } from './appState.svelte'

export interface OutlinerInstance {
  id: string
  /** Display label for the tab */
  label: string
  /** Interaction mode */
  mode: 'navigate' | 'edit'
  editCursorHint: 'start' | 'end'
  selectedBlockIds: Uuid[]
  selectionAnchor: Uuid | null
  editingBlockId: Uuid | null
  expandedBlocks: SvelteSet<string>
  activeNamespaceBlockId: Uuid | null
  activeNamespaceLineageId: Uuid | null
  activeNamespaceName: string | null
  activeNamespaceFullPath: string | null
  currentBlock: BlockWithContent | null
  activeTreeNode: any
  breadcrumbs: Breadcrumb[]
}

// --- Module-level reactive state ---
let _outliners = $state(new Map<string, OutlinerInstance>())
let _activeOutlinerId = $state<string | null>(null)

export const outlinerState = {
  get outliners() { return _outliners },
  get activeOutlinerId() { return _activeOutlinerId },
  set activeOutlinerId(id: string | null) { _activeOutlinerId = id },
}

/** Generate a short random hex ID for outliner panels. */
function randomId(): string {
  const bytes = new Uint8Array(6)
  crypto.getRandomValues(bytes)
  return 'ol-' + Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('')
}

/** Compute a display label from the outliner's navigation state. */
function labelFor(instance: OutlinerInstance): string {
  if (instance.activeNamespaceName) return `Outliner - ${instance.activeNamespaceName}`
  if (instance.activeNamespaceFullPath) return `Outliner - ${instance.activeNamespaceFullPath}`
  return 'Outliner'
}

function makeInstance(id: string): OutlinerInstance {
  return {
    id,
    label: 'Outliner',
    mode: 'navigate',
    editCursorHint: 'end',
    selectedBlockIds: [],
    selectionAnchor: null,
    editingBlockId: null,
    expandedBlocks: new SvelteSet<string>(),
    activeNamespaceBlockId: null,
    activeNamespaceLineageId: null,
    activeNamespaceName: null,
    activeNamespaceFullPath: null,
    currentBlock: null,
    activeTreeNode: null,
    breadcrumbs: [],
  }
}

/**
 * Create a new outliner instance. Returns the instance.
 * If an instance with this ID already exists, returns the existing one.
 * If no ID is given, generates a random one.
 */
export function createOutliner(id?: string): OutlinerInstance {
  if (id && _outliners.has(id)) return _outliners.get(id)!

  const actualId = id ?? randomId()
  const instance = makeInstance(actualId)
  _outliners.set(actualId, instance)

  // If this is the first outliner, make it active
  if (_outliners.size === 1) {
    _activeOutlinerId = actualId
  }

  return instance
}

/**
 * Remove an outliner instance. Prevents removing the last one.
 */
export function removeOutliner(id: string): boolean {
  if (_outliners.size <= 1) return false
  _outliners.delete(id)

  // If we removed the active outliner, switch to another
  if (_activeOutlinerId === id) {
    _activeOutlinerId = _outliners.keys().next().value ?? null
  }
  return true
}

export function getOutliner(id: string): OutlinerInstance | undefined {
  return _outliners.get(id)
}

export function getActiveOutliner(): OutlinerInstance | undefined {
  if (!_activeOutlinerId) return undefined
  return _outliners.get(_activeOutlinerId)
}

/**
 * Save appState fields into the given outliner instance.
 */
export function syncFromAppState(
  instance: OutlinerInstance,
  appState: any
): void {
  instance.mode = appState.mode
  instance.editCursorHint = appState.editCursorHint
  instance.selectedBlockIds = [...appState.selectedBlockIds]
  instance.selectionAnchor = appState.selectionAnchor
  instance.editingBlockId = appState.editingBlockId
  instance.expandedBlocks = new SvelteSet(appState.expandedBlocks)
  instance.activeNamespaceBlockId = appState.activeNamespaceBlockId
  instance.activeNamespaceLineageId = appState.activeNamespaceLineageId
  instance.activeNamespaceName = appState.activeNamespaceName
  instance.activeNamespaceFullPath = appState.activeNamespaceFullPath
  instance.currentBlock = appState.currentBlock
  instance.activeTreeNode = appState.activeTreeNode
  instance.breadcrumbs = [...appState.breadcrumbs]
  instance.label = labelFor(instance)
}

/**
 * Load outliner instance state into appState.
 */
export function syncToAppState(
  instance: OutlinerInstance,
  appState: any
): void {
  appState.mode = instance.mode
  appState.editCursorHint = instance.editCursorHint
  appState.selectedBlockIds = [...instance.selectedBlockIds]
  appState.selectionAnchor = instance.selectionAnchor
  appState.editingBlockId = instance.editingBlockId
  appState.expandedBlocks.clear()
  for (const id of instance.expandedBlocks) {
    appState.expandedBlocks.add(id)
  }
  appState.activeNamespaceBlockId = instance.activeNamespaceBlockId
  appState.activeNamespace = instance.activeNamespaceBlockId
  appState.activeNamespaceLineageId = instance.activeNamespaceLineageId
  appState.activeNamespaceName = instance.activeNamespaceName
  appState.activeNamespaceFullPath = instance.activeNamespaceFullPath
  appState.currentBlock = instance.currentBlock
  appState.activeTreeNode = instance.activeTreeNode
  appState.breadcrumbs = [...instance.breadcrumbs]
}

/**
 * Switch the active outliner. Saves state from appState into the old instance,
 * then loads the new instance's state into appState.
 */
export function setActiveOutliner(id: string, appState: any): void {
  if (id === _activeOutlinerId) return
  if (!_outliners.has(id)) return

  // Save current outliner state
  if (_activeOutlinerId) {
    const prev = _outliners.get(_activeOutlinerId)
    if (prev) syncFromAppState(prev, appState)
  }

  // Load new outliner state
  _activeOutlinerId = id
  const next = _outliners.get(id)!
  syncToAppState(next, appState)
}

/**
 * Get the total outliner count.
 */
export function getOutlinerCount(): number {
  return _outliners.size
}

/**
 * Get all outliner paths for URL serialization, sorted by outliner ID.
 * Returns each outliner's namespace path (or '~' for home/root).
 * Sorted by ID so the URL is stable regardless of which outliner is active.
 * Also returns the active outliner's ID so the caller can override its
 * (potentially stale) path with the live value from appState.
 */
export function getAllOutlinerPaths(): { entries: { id: string, path: string }[], activeId: string | null } {
  const entries: { id: string, path: string }[] = []
  for (const [id, instance] of _outliners) {
    entries.push({ id, path: instance.activeNamespaceFullPath || '~' })
  }
  entries.sort((a, b) => a.id.localeCompare(b.id))
  return { entries, activeId: _activeOutlinerId }
}
