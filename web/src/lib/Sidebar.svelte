<script lang="ts">
  import { api } from './api';
  import { blockTree, type TreeNode } from './blockTree.svelte';
  import {
    appState,
    setActiveNamespace,
    toggleSidebarExpanded,
  } from './appState.svelte';
  import { addToast } from './toastStore.svelte';
  import { clearCompletionCache } from './editor/completion';
  import { isBookmarked, toggleBookmark } from './bookmarkStore.svelte';
  import { openInNewOutliner } from './dockviewActions.svelte';

  let deleteTarget: TreeNode | null = $state(null);
  let deleting = $state(false);
  let cancelButton: HTMLButtonElement | undefined = $state(undefined);

  function closeModal() {
    deleteTarget = null;
  }

  function handleModalKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      closeModal();
    }
  }

  $effect(() => {
    if (appState.navigationReady) blockTree.loadRoots();
  });

  $effect(() => {
    if (deleteTarget && cancelButton) {
      cancelButton.focus();
    }
  });

  function handleNodeClick(node: TreeNode, e: MouseEvent) {
    if (e.ctrlKey || e.metaKey) {
      // Ctrl+click: open in new outliner tab
      openInNewOutliner(node.id);
    } else {
      setActiveNamespace(node.id, node.lineage_id, node.namespace, node.name);
    }
  }

  async function handleToggle(e: Event, node: TreeNode) {
    e.stopPropagation();
    toggleSidebarExpanded(node.id);
    // Lazy-load deeper tree on expand
    if (!appState.sidebarExpanded.has(node.id)) return; // was just collapsed
    await blockTree.loadChildrenDeep(node.id);
  }

  function isActive(node: TreeNode): boolean {
    return appState.activeNamespace === node.id;
  }

  function handleBookmarkClick(e: Event, node: TreeNode) {
    e.stopPropagation();
    toggleBookmark(node.id);
  }

  function requestDelete(e: Event, node: TreeNode) {
    e.stopPropagation();
    deleteTarget = node;
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    deleting = true;
    try {
      await api.blocks.deleteRecursive(deleteTarget.id);
      // If we deleted the active namespace, clear it
      if (appState.activeNamespace === deleteTarget.id) {
        setActiveNamespace(null);
      }
      await blockTree.loadRoots();
      clearCompletionCache();
    } catch (err) {
      console.error('Failed to delete block:', err);
      addToast(`Failed to delete "${deleteTarget?.name}"`, 'error');
    } finally {
      deleteTarget = null;
      deleting = false;
    }
  }
</script>

<div class="sidebar">
  <div class="sidebar-header">
    <span class="sidebar-title">Navigator</span>
    <button
      class="sidebar-home"
      onclick={() => setActiveNamespace(null)}
      aria-label="Show all roots"
      title="Show all roots"
    >
      &#x2302;
    </button>
  </div>

  <div class="sidebar-tree" role="tree" aria-label="Namespace tree">
    {#each blockTree.roots as node (node.id)}
      {@render treeNode(node, 0)}
    {/each}
  </div>
</div>

{#if deleteTarget}
  <div
    class="modal-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="Confirm deletion"
    tabindex="-1"
    onkeydown={handleModalKeydown}
  >
    <button class="modal-backdrop-dismiss" onclick={closeModal} aria-label="Close dialog" tabindex="-1"></button>
    <div class="modal">
      <div class="modal-title">Delete namespace?</div>
      <div class="modal-body">
        Delete <strong>{deleteTarget.name}</strong> and all its children? Deleted blocks can be recovered from the Orphans view.
      </div>
      <div class="modal-actions">
        <button class="btn-cancel" bind:this={cancelButton} onclick={closeModal}>Cancel</button>
        <button class="btn-delete" onclick={confirmDelete} disabled={deleting}>
          {deleting ? 'Deleting…' : 'Delete'}
        </button>
      </div>
    </div>
  </div>
{/if}

{#snippet treeNode(node: TreeNode, depth: number)}
  <div
    class="tree-item"
    class:active={isActive(node)}
    style="padding-left: {12 + depth * 16}px"
    role="treeitem"
    aria-selected={isActive(node)}
    aria-expanded={node.children.length > 0 ? appState.sidebarExpanded.has(node.id) : undefined}
    onclick={(e) => handleNodeClick(node, e)}
    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleNodeClick(node, e as unknown as MouseEvent); } }}
    tabindex="0"
  >
    {#if node.children.length > 0}
      <button
        class="tree-toggle"
        class:expanded={appState.sidebarExpanded.has(node.id)}
        onclick={(e) => handleToggle(e, node)}
        aria-label="Toggle expand"
        tabindex="-1"
      >
        &#x25B6;
      </button>
    {:else}
      <span class="tree-toggle-placeholder"></span>
    {/if}
    <span class="tree-name">{node.name}</span>
    {#if node.children.length > 0}
      <span class="tree-count">{node.children.length}</span>
    {/if}
    <button
      class="tree-bookmark"
      class:bookmarked={isBookmarked(node.id)}
      onclick={(e) => handleBookmarkClick(e, node)}
      aria-label={isBookmarked(node.id) ? 'Remove bookmark' : 'Bookmark'}
      title={isBookmarked(node.id) ? 'Remove bookmark' : 'Bookmark'}
      tabindex="-1"
    >{isBookmarked(node.id) ? '★' : '☆'}</button>
    <button class="tree-delete" onclick={(e) => requestDelete(e, node)} aria-label="Delete namespace" title="Delete namespace and children" tabindex="-1">✕</button>
  </div>
  {#if appState.sidebarExpanded.has(node.id)}
    <div role="group">
      {#each node.children as child (child.id)}
        {@render treeNode(child, depth + 1)}
      {/each}
    </div>
  {/if}
{/snippet}

<style>
  .sidebar {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    color: var(--text-primary);
    overflow: hidden;
    user-select: none;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
  }

  .sidebar-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .sidebar-home {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font-size: 16px;
    opacity: 0.6;
    transition: opacity 0.15s;
    padding: 0;
    font: inherit;
  }

  .sidebar-home:hover {
    opacity: 1;
  }

  .sidebar-tree {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .tree-item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 12px;
    cursor: pointer;
    font-size: 13px;
    border-radius: 4px;
    margin: 1px 4px;
    transition: background 0.1s;
  }

  .tree-item:hover {
    background: var(--bg-hover);
  }

  .tree-item.active {
    background: var(--bg-active);
    color: var(--accent-color);
  }

  .tree-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    font-size: 8px;
    flex-shrink: 0;
    transition: transform 0.15s;
    opacity: 0.5;
    color: var(--text-muted);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
  }

  .tree-toggle:hover {
    opacity: 1;
  }

  .tree-toggle.expanded {
    transform: rotate(90deg);
  }

  .tree-toggle-placeholder {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }

  .tree-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tree-count {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-tertiary);
    padding: 0 5px;
    border-radius: 8px;
    flex-shrink: 0;
  }

  .tree-bookmark {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    display: none;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    color: var(--text-muted);
    cursor: pointer;
    transition: color 0.1s;
    background: none;
    border: none;
    padding: 0;
  }

  .tree-bookmark.bookmarked {
    display: inline-flex;
    color: var(--accent-color, #e2b86b);
  }

  .tree-item:hover .tree-bookmark {
    display: inline-flex;
  }

  .tree-bookmark:hover {
    color: var(--accent-color, #e2b86b);
  }

  .tree-delete {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    display: none;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    color: var(--text-muted);
    border-radius: 3px;
    cursor: pointer;
    background: none;
    border: none;
    padding: 0;
  }

  .tree-delete:hover {
    color: var(--error-color, #e06c75);
    background: var(--bg-hover);
  }

  .tree-item:hover .tree-delete {
    display: inline-flex;
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal-backdrop-dismiss {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    border: none;
    padding: 0;
    cursor: default;
  }

  .modal {
    position: relative;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 20px;
    min-width: 300px;
    max-width: 400px;
  }

  .modal-title {
    font-size: 14px;
    font-weight: 600;
    margin-bottom: 10px;
  }

  .modal-body {
    font-size: 13px;
    color: var(--text-secondary);
    margin-bottom: 16px;
    line-height: 1.5;
  }

  .modal-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .btn-cancel {
    padding: 6px 14px;
    font-size: 12px;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    cursor: pointer;
  }

  .btn-cancel:hover {
    background: var(--bg-hover);
  }

  .btn-delete {
    padding: 6px 14px;
    font-size: 12px;
    border: none;
    border-radius: 4px;
    background: var(--error-color, #e06c75);
    color: white;
    cursor: pointer;
  }

  .btn-delete:hover:not(:disabled) {
    opacity: 0.85;
  }

  .btn-delete:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
