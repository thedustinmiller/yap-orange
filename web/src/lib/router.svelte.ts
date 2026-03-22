/**
 * Hash-based URL router for yap-orange.
 *
 * URL format:
 *   /#/              — Home (root blocks)
 *   /#/<ns::path>    — Navigate to namespace path (e.g. /#/research::ml::attention)
 *   /#/block/<UUID>  — Navigate to block ID directly
 *
 * Multi-outliner format (pipe-delimited, sorted by outliner ID):
 *   /#/journal::2026|projects::yap-orange|research::ml
 *   ~ represents home/root view.
 *   Which outliner is active is dockview layout state, not encoded here.
 *
 * Circular import avoidance: this module imports navigateTo/navigateHome from
 * appState, and appState calls pushRoute via a registered callback (registerRoutePusher).
 */

import { navigateTo, navigateHome, appState } from './appState.svelte'
import { getAllOutlinerPaths, getOutlinerCount } from './outlinerStore.svelte'
import { openOutlinersFromPaths, isRestoringOutliners } from './dockviewActions.svelte'
import { api } from './api'

// Track the last hash we set internally to suppress re-navigation on the
// hashchange event that fires when pushRoute updates window.location.hash.
let _lastPushedHash = ''

/**
 * Serialize all outliner paths into a hash string.
 * Sorted by outliner ID for stability — no reordering on focus switch.
 * Single outliner uses plain path (backward compatible).
 *
 * @param activePath - The live path for the active outliner. Required because
 *   the active outliner's state lives in appState, not in its OutlinerInstance
 *   (instance only gets synced on focus switch via syncFromAppState).
 */
function serializeOutlinerPaths(activePath: string): string {
  const { entries, activeId } = getAllOutlinerPaths()

  // Override the active outliner's (potentially stale) instance path
  // with the live value from the caller.
  // Normalize '/' to '~' — both mean home, but '/' would produce
  // a double-slash in the hash (e.g. /#//|~|~).
  const normalizedActive = (!activePath || activePath === '/') ? '~' : activePath
  for (const entry of entries) {
    if (entry.id === activeId) {
      entry.path = normalizedActive
      break
    }
  }

  if (entries.length <= 1) {
    const p = entries[0]?.path ?? '/'
    return p === '~' ? '/' : p
  }

  // Already sorted by ID in getAllOutlinerPaths
  return entries.map(e => e.path).join('|')
}

/**
 * Update the browser URL hash to reflect the current navigation state.
 * Called by appState via the registered callback — do not call directly.
 *
 * When multiple outliners exist, serializes all paths sorted by outliner ID.
 */
export function pushRoute(path: string): void {
  // Suppress URL updates while restoring outliners from a multi-path URL
  // (each navigateTo during restore would otherwise push intermediate hashes)
  if (isRestoringOutliners()) return

  // path is the active outliner's live path (from appState via _routePusher).
  // For multi-outliner, serialize all paths using this as the active's value.
  const serialized = getOutlinerCount() > 1 ? serializeOutlinerPaths(path) : path
  const hashValue = serialized === '/' ? '/' : `/${serialized}`
  const fullHash = `#${hashValue}`
  _lastPushedHash = fullHash
  if (window.location.hash !== fullHash) {
    window.location.hash = hashValue
  }
}

async function handleHash(hash: string): Promise<void> {
  // Strip leading '#' and optional leading '/', then decode URI-encoded chars
  // (e.g. %20 → space) so names with spaces resolve correctly.
  const raw = decodeURIComponent(hash.replace(/^#\/?/, ''))

  if (!raw || raw === '/') {
    navigateHome()
    return
  }

  // Check for pipe-delimited multi-outliner paths
  const segments = raw.split('|')
  if (segments.length > 1) {
    await openOutlinersFromPaths(segments)
    // Explicitly push the correct URL after restore — the DockLayout $effect
    // may have been suppressed during restore and won't re-fire if the final
    // appState value matches what it last tracked.
    pushRoute(appState.activeNamespaceFullPath ?? '/')
    return
  }

  // Single path — existing behavior
  const path = segments[0]

  if (path === '~') {
    navigateHome()
    return
  }

  if (path.startsWith('block/')) {
    const blockId = path.slice(6)
    await navigateTo(blockId)
    return
  }

  // Treat as a namespace path — resolve to a block, then navigate
  try {
    const result = await api.resolve({ path })
    await navigateTo(result.block_id)
  } catch {
    console.warn('Failed to resolve namespace:', path)
    navigateHome()
  }
}

/**
 * Handle the initial URL hash on page load.
 * Returns true if a hash was present and navigation was initiated.
 * App.svelte calls this after restoring settings so hash takes priority
 * over the last-location setting.
 */
export async function handleInitialHash(): Promise<boolean> {
  const hash = window.location.hash
  if (hash && hash !== '#' && hash !== '#/') {
    await handleHash(hash)
    return true
  }
  return false
}

/**
 * Initialize the router. Call once from App.svelte onMount (after settings
 * are restored so the initial hash read happens last).
 * Sets up the hashchange listener for browser back/forward navigation.
 */
export function initRouter(): void {
  window.addEventListener('hashchange', () => {
    const hash = window.location.hash
    if (hash === _lastPushedHash) {
      // We pushed this hash internally — consume the flag and skip re-navigation
      _lastPushedHash = ''
      return
    }
    handleHash(hash)
  })
}
