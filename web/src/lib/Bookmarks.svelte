<script lang="ts">
  import { getBookmarks, toggleBookmark } from './bookmarkStore.svelte';
  import { blockTree, type TreeNode } from './blockTree.svelte';
  import { api } from './api';
  import { setActiveNamespace } from './appState.svelte';

  // Resolved bookmark entries
  interface BookmarkEntry {
    id: string;
    name: string;
    namespace: string;
    lineage_id: string;
  }

  let entries = $state<BookmarkEntry[]>([]);
  let bookmarks = $derived(getBookmarks());

  // Re-resolve bookmarks when the set changes
  $effect(() => {
    const ids = [...bookmarks];
    resolveBookmarks(ids);
  });

  async function resolveBookmarks(ids: string[]) {
    const resolved: BookmarkEntry[] = [];
    for (const id of ids) {
      const node = blockTree.getNode(id);
      if (node) {
        resolved.push({
          id: node.id,
          name: node.name || node.namespace?.split('::').pop() || '?',
          namespace: node.namespace,
          lineage_id: node.lineage_id,
        });
      } else {
        // Fallback: fetch from API
        try {
          const block = await api.blocks.get(id);
          resolved.push({
            id: block.id,
            name: block.name || block.namespace?.split('::').pop() || '?',
            namespace: block.namespace,
            lineage_id: block.lineage_id,
          });
        } catch {
          // Block may have been deleted — skip it
        }
      }
    }
    entries = resolved;
  }

  function handleClick(entry: BookmarkEntry) {
    setActiveNamespace(entry.id, entry.lineage_id, entry.namespace, entry.name);
  }

  function handleUnbookmark(e: Event, id: string) {
    e.stopPropagation();
    toggleBookmark(id);
  }
</script>

<div class="bookmarks">
  <div class="bookmarks-header">
    <span class="bookmarks-title">Bookmarks</span>
  </div>

  <div class="bookmarks-list">
    {#if entries.length === 0}
      <div class="empty-state">
        <span class="empty-icon">&#x2606;</span>
        <span class="empty-text">No bookmarks yet</span>
        <span class="empty-hint">Star blocks in the Navigator to add them here</span>
      </div>
    {:else}
      {#each entries as entry (entry.id)}
        <div
          class="bookmark-item"
          role="button"
          tabindex="0"
          onclick={() => handleClick(entry)}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleClick(entry); } }}
        >
          <span class="bookmark-name">{entry.name}</span>
          <span class="bookmark-ns">{entry.namespace}</span>
          <button
            class="bookmark-remove"
            onclick={(e) => handleUnbookmark(e, entry.id)}
            aria-label="Remove bookmark"
          >&#x2605;</button>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .bookmarks {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    color: var(--text-primary);
    overflow: hidden;
    user-select: none;
  }

  .bookmarks-header {
    display: flex;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
  }

  .bookmarks-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .bookmarks-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 32px 16px;
    gap: 4px;
  }

  .empty-icon {
    font-size: 24px;
    color: var(--text-muted);
    opacity: 0.4;
  }

  .empty-text {
    font-size: 12px;
    color: var(--text-muted);
  }

  .empty-hint {
    font-size: 11px;
    color: var(--text-muted);
    opacity: 0.6;
    text-align: center;
  }

  .bookmark-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    cursor: pointer;
    font-size: 13px;
    border-radius: 4px;
    margin: 1px 4px;
    transition: background 0.1s;
    background: none;
    border: none;
    width: calc(100% - 8px);
    text-align: left;
    color: inherit;
    font-family: inherit;
  }

  .bookmark-item:hover {
    background: var(--bg-hover);
  }

  .bookmark-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bookmark-ns {
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100px;
    flex-shrink: 0;
  }

  .bookmark-remove {
    flex-shrink: 0;
    font-size: 14px;
    color: var(--accent-color, #e2b86b);
    cursor: pointer;
    opacity: 0.6;
    transition: opacity 0.15s;
    background: none;
    border: none;
    padding: 0;
    line-height: 1;
  }

  .bookmark-remove:hover {
    opacity: 1;
  }
</style>
