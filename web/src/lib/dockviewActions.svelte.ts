/**
 * Dockview actions — module-level ref to the DockviewComponent
 * so other modules can add/remove panels without prop drilling.
 */

import type { DockviewComponent, DockviewGroupPanel } from 'dockview-core'
import { createOutliner, setActiveOutliner, outlinerState } from './outlinerStore.svelte'
import { navigateTo, navigateHome, appState } from './appState.svelte'
import { api } from './api'
import { setSetting } from './settingsStore.svelte'

/**
 * Static panel definitions — shared between DockLayout and GroupHeaderMenu.
 * Outliner is dynamic (not listed here).
 * Panels with `devOnly: true` are only shown when dev_mode is enabled.
 */
export const PANEL_DEFS: readonly { readonly id: string; readonly component: string; readonly title: string; readonly devOnly?: boolean }[] = [
  { id: 'sidebar',      component: 'sidebar',      title: 'Navigator' },
  { id: 'bookmarks',    component: 'bookmarks',    title: 'Bookmarks' },
  { id: 'graph',        component: 'graph',        title: 'Graph' },
  { id: 'backlinks',    component: 'backlinks',    title: 'Links' },
  { id: 'properties',   component: 'properties',   title: 'Properties' },
  { id: 'debuglog',     component: 'debuglog',     title: 'Debug Log',    devOnly: true },
  { id: 'benchmarks',   component: 'benchmarks',   title: 'Benchmarks',   devOnly: true },
  { id: 'importexport', component: 'importexport', title: 'Import/Export' },
]

/** IDs of panels that require dev_mode to be visible. */
export const DEV_PANEL_IDS = PANEL_DEFS.filter(d => d.devOnly).map(d => d.id)

export type PanelDef = typeof PANEL_DEFS[number]

let _dockview: DockviewComponent | null = null

// Promise that resolves when dockview is first set (layout mounted).
let _dockviewReadyResolve: (() => void) | null = null
let _dockviewReadyPromise = new Promise<void>(r => { _dockviewReadyResolve = r })

export function setDockviewRef(dv: DockviewComponent | null): void {
  _dockview = dv
  if (dv && _dockviewReadyResolve) {
    _dockviewReadyResolve()
    _dockviewReadyResolve = null
  }
}

export function getDockviewRef(): DockviewComponent | null {
  return _dockview
}

/**
 * Whether we're in the middle of restoring outliners from a URL.
 * While true, pushRoute calls are suppressed to avoid URL thrashing.
 */
let _restoring = false
export function isRestoringOutliners(): boolean {
  return _restoring
}

/**
 * Update a dockview panel's visible tab title.
 */
export function updatePanelTitle(panelId: string, title: string): void {
  if (!_dockview) return
  const panel = _dockview.panels.find(p => p.id === panelId)
  if (panel) {
    panel.api.updateParameters({ title })
    // setTitle is the dockview method to update the visible tab label
    ;(panel as any).setTitle?.(title) || panel.api.setTitle?.(title)
  }
}

/** Check if a panel ID belongs to an outliner. */
function isOutlinerPanel(id: string): boolean {
  return id === 'outliner' || id.startsWith('outliner-') || id.startsWith('ol-')
}

/**
 * Open a block in a new outliner tab.
 * Creates an outliner instance with a random ID, adds a dockview panel,
 * and navigates the new outliner to the block.
 */
export function openInNewOutliner(blockId: string): void {
  if (!_dockview) return

  // Random ID — never collides with restored layouts
  const instance = createOutliner()

  // Find an existing outliner group to place the new tab "within" (as a tab sibling)
  const existingOutlinerPanel = _dockview.panels.find(
    p => p.id === 'outliner' || p.id.startsWith('outliner') || p.id.startsWith('ol-')
  )

  const position = existingOutlinerPanel
    ? { referencePanel: existingOutlinerPanel, direction: 'within' as const }
    : undefined

  _dockview.addPanel({
    id: instance.id,
    component: 'outliner',
    title: 'Outliner',
    position,
  })

  // Navigate the new outliner to the target block.
  // Small delay lets the panel mount and become active via onDidActivePanelChange.
  setTimeout(() => {
    navigateTo(blockId)
  }, 50)
}

/**
 * Restore multiple outliners from URL paths.
 * Waits for dockview to be ready, reuses existing outliner panels from
 * layout restore, and creates new panels (next to the active outliner)
 * if the URL has more paths than existing outliners.
 * Does not change which outliner is active — that's dockview layout state.
 */
export async function openOutlinersFromPaths(paths: string[]): Promise<void> {
  if (paths.length === 0) return

  // Set _restoring BEFORE awaiting dockview — during layout restore, dockview
  // fires onDidActivePanelChange which triggers $effects that would push ~|~|~
  // to the URL (all outliners start with empty state). This closes that race window.
  _restoring = true
  try {
    // Wait for dockview to mount (it renders after settingsReady=true)
    await _dockviewReadyPromise
    if (!_dockview) return

    // Get existing outliner panels from the restored layout, sorted by ID
    // to match the URL's sorted-by-ID order (so path[i] maps to the right panel)
    const existingPanels = _dockview.panels
      .filter(p => isOutlinerPanel(p.id))
      .sort((a, b) => a.id.localeCompare(b.id))

    // Remember which outliner was active before we start switching around
    const originalActiveId = outlinerState.activeOutlinerId

    for (let i = 0; i < paths.length; i++) {
      let panelId: string

      if (i < existingPanels.length) {
        // Reuse existing panel from layout restore
        panelId = existingPanels[i].id
      } else {
        // Create a new panel — place next to any existing outliner
        const instance = createOutliner()
        const refPanel = _dockview.panels.find(p => isOutlinerPanel(p.id))
        const position = refPanel
          ? { referencePanel: refPanel, direction: 'within' as const }
          : undefined
        _dockview.addPanel({
          id: instance.id,
          component: 'outliner',
          title: 'Outliner',
          position,
        })
        panelId = instance.id
        // Let the panel mount
        await new Promise(r => setTimeout(r, 50))
      }

      // Switch to this outliner so navigateTo targets it
      setActiveOutliner(panelId, appState)
      await navigateToPath(paths[i])
    }

    // Restore the original active outliner (or fall back to the first one).
    // Which tab is active is dockview layout state, not the URL's concern.
    const restoreId = originalActiveId && outlinerState.outliners.has(originalActiveId)
      ? originalActiveId
      : existingPanels[0]?.id ?? outlinerState.outliners.keys().next().value
    if (restoreId) {
      setActiveOutliner(restoreId, appState)
      const panel = _dockview!.panels.find(p => p.id === restoreId)
      if (panel) panel.api.setActive()
    }
  } finally {
    _restoring = false
  }
}

/** Returns IDs of all currently visible (mounted) panels. */
export function getVisiblePanelIds(): string[] {
  if (!_dockview) return []
  return _dockview.panels.map(p => p.id)
}

/** Add a panel to a specific group (or the active group as fallback). */
export function addPanel(panelId: string, targetGroup?: DockviewGroupPanel): void {
  if (!_dockview) return
  const def = PANEL_DEFS.find(d => d.id === panelId)
  if (!def) return
  // Don't add if already exists
  if (_dockview.panels.some(p => p.id === panelId)) return

  const position = targetGroup
    ? { referenceGroup: targetGroup, direction: 'within' as const }
    : undefined

  _dockview.addPanel({
    id: def.id,
    component: def.component,
    title: def.title,
    position,
  })
}

/** Close/remove a panel by ID. */
export function removePanel(panelId: string): void {
  if (!_dockview) return
  const panel = _dockview.panels.find(p => p.id === panelId)
  if (panel) panel.api.close()
}

/**
 * Show a panel and give it focus. If the panel was removed (hidden),
 * re-add it first, then activate it.
 */
export function showAndFocusPanel(panelId: string): void {
  if (!_dockview) return
  let panel = _dockview.panels.find(p => p.id === panelId)
  if (!panel) {
    // Panel was removed — re-add it
    const def = PANEL_DEFS.find(d => d.id === panelId)
    if (!def) return
    _dockview.addPanel({
      id: def.id,
      component: def.component,
      title: def.title,
    })
    panel = _dockview.panels.find(p => p.id === panelId)
  }
  if (panel) panel.api.setActive()
}

/** Check whether a panel currently exists in the layout. */
export function isPanelVisible(panelId: string): boolean {
  if (!_dockview) return false
  return _dockview.panels.some(p => p.id === panelId)
}

/** Focus (activate) an existing panel by ID. No-op if it doesn't exist. */
export function focusPanel(panelId: string): void {
  if (!_dockview) return
  const panel = _dockview.panels.find(p => p.id === panelId)
  if (panel) panel.api.setActive()
}

/** Get the currently active panel's ID, or null. */
export function getActivePanelId(): string | null {
  if (!_dockview) return null
  return _dockview.activePanel?.id ?? null
}

/** Serialize and persist the current dockview layout. */
export function saveLayout(): void {
  if (!_dockview) return
  try {
    const state = _dockview.toJSON()
    setSetting('layout_state', state, 500)
  } catch {
    // Serialization can fail during transitions
  }
}

/**
 * Navigate to a namespace path or '~' (home).
 * Resolves path → block ID, handling errors gracefully.
 */
async function navigateToPath(path: string): Promise<void> {
  if (!path || path === '~' || path === '/') {
    navigateHome()
    return
  }

  if (path.startsWith('block/')) {
    await navigateTo(path.slice(6))
    return
  }

  try {
    const result = await api.resolve({ path })
    await navigateTo(result.block_id)
  } catch {
    console.warn('Failed to resolve path for outliner restore:', path)
    navigateHome()
  }
}
