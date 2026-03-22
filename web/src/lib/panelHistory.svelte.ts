/**
 * MRU (Most Recently Used) panel history.
 *
 * Tracks the order in which dockview panels are activated so the
 * Quick Switcher can list them by recency and Alt+N shortcuts
 * can "bounce back" to the last outliner.
 */

let _mruList = $state<string[]>([])

/** Push a panel ID to the front of the MRU list. Deduplicates. */
export function pushPanel(id: string): void {
  const idx = _mruList.indexOf(id)
  if (idx === 0) return // already at front
  if (idx > 0) _mruList.splice(idx, 1)
  _mruList.unshift(id)
  // Cap at 50 entries to avoid unbounded growth
  if (_mruList.length > 50) _mruList.length = 50
}

/** Get the MRU list (most recent first). */
export function getMruList(): readonly string[] {
  return _mruList
}

/**
 * Get the last outliner panel ID in the MRU list.
 * Used by Alt+N toggle: when pressing Alt+N on an already-focused
 * utility panel, we bounce back to the last active outliner.
 */
export function getLastOutlinerId(): string | null {
  for (const id of _mruList) {
    if (id === 'outliner' || id.startsWith('outliner-') || id.startsWith('ol-')) {
      return id
    }
  }
  return null
}

/** Remove a panel from the MRU list (e.g. when panel is closed). */
export function removeFromMru(id: string): void {
  const idx = _mruList.indexOf(id)
  if (idx >= 0) _mruList.splice(idx, 1)
}
