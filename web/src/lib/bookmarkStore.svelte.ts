/**
 * Bookmark store — reactive set of bookmarked block IDs,
 * persisted to the 'bookmarked_blocks' setting key.
 */

import { SvelteSet } from 'svelte/reactivity'
import { getSetting, setSetting } from './settingsStore.svelte'

let _bookmarks = $state(new SvelteSet<string>())

export function isBookmarked(blockId: string): boolean {
  return _bookmarks.has(blockId)
}

export function toggleBookmark(blockId: string): void {
  if (_bookmarks.has(blockId)) {
    _bookmarks.delete(blockId)
  } else {
    _bookmarks.add(blockId)
  }
  // Persist
  setSetting('bookmarked_blocks', [..._bookmarks], 500)
}

export function loadBookmarks(): void {
  const saved = getSetting<string[]>('bookmarked_blocks') ?? []
  for (const id of saved) {
    _bookmarks.add(id)
  }
}

export function getBookmarks(): SvelteSet<string> {
  return _bookmarks
}
