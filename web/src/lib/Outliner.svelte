<script lang="ts">
  import OutlinerNode from './OutlinerNode.svelte';
  import { blockTree, type TreeNode } from './blockTree.svelte';
  import { api } from './api';
  import {
    appState,
    flattenVisibleNodes,
    selectBlock,
    selectBlocks,
    enterEditMode,
    enterNavigationMode,
    toggleExpanded,
    navigateTo,
    navigateHome,
    collapseAll,
    expandAll,
  } from './appState.svelte';
  import { indentBlocks, outdentBlocks, reloadCurrentTree, reloadAffectedParents } from './outlinerActions';
  import { getSetting } from './settingsStore.svelte';
  import type { Uuid } from './types';
  import { tick, setContext, onMount } from 'svelte';
  import { clearCompletionCache } from './editor/completion';
  import { addToast } from './toastStore.svelte';
  import { setActiveOutliner, getOutliner, outlinerState } from './outlinerStore.svelte';

  let { outlinerId = 'outliner' }: { outlinerId?: string } = $props();

  // Provide the correct expandedBlocks set to children via context.
  // Active outliner → appState.expandedBlocks (so consumer panels stay in sync).
  // Non-active outliner → instance's own expandedBlocks (so it keeps its own tree state).
  setContext('get-expanded-blocks', () => {
    if (outlinerState.activeOutlinerId === outlinerId) {
      return appState.expandedBlocks;
    }
    return getOutliner(outlinerId)?.expandedBlocks ?? appState.expandedBlocks;
  });

  let containerEl: HTMLDivElement | undefined = $state();
  let isActiveOutliner = $derived(outlinerState.activeOutlinerId === outlinerId);
  let devMode = $derived(getSetting<boolean>('dev_mode') ?? false);

  // When returning to navigate mode (e.g. after Escape from BlockEditor),
  // refocus the outliner container so keyboard events reach handleKeydown.
  // Only refocus if this is the active outliner to avoid stealing focus.
  $effect(() => {
    if (isActiveOutliner && appState.mode === 'navigate' && containerEl) {
      queueMicrotask(() => containerEl?.focus());
    }
  });

  // Top-level nodes for the outliner tree.
  // Active outliner reads from appState (navigateTo writes there).
  // Non-active outliners read from their saved instance state.
  let treeRoots = $derived.by(() => {
    let nsId: string | null;
    let treeNode: any;

    if (isActiveOutliner) {
      nsId = appState.activeNamespaceBlockId;
      treeNode = appState.activeTreeNode ?? (nsId ? blockTree.getNode(nsId) : null);
    } else {
      const inst = getOutliner(outlinerId);
      nsId = inst?.activeNamespaceBlockId ?? null;
      treeNode = inst?.activeTreeNode ?? (nsId ? blockTree.getNode(nsId) : null);
    }

    if (!nsId) return blockTree.roots;
    const node = treeNode ?? blockTree.getNode(nsId);
    return node ? [node] : [];
  });

  let flatNodes = $derived(flattenVisibleNodes(treeRoots));

  // Current block info for header display — per-instance
  let currentBlock = $derived(
    isActiveOutliner ? appState.currentBlock : getOutliner(outlinerId)?.currentBlock ?? null
  );
  let breadcrumbs = $derived(
    isActiveOutliner ? appState.breadcrumbs : getOutliner(outlinerId)?.breadcrumbs ?? []
  );

  // Quick-create state
  let showQuickCreate = $state(false);
  let quickCreateName = $state('');

  // Inline rename state for the current (header) block.
  // We track which block ID we're renaming so navigation resets it.
  let renamingHeader = $state(false);
  let headerRenameValue = $state('');
  let renamingBlockId = $state<string | null>(null);
  let headerRenameCancelled = false;
  let headerRenameInput = $state<HTMLInputElement | undefined>();

  // Cancel rename whenever the focused block changes (navigation, breadcrumb click, etc.)
  $effect(() => {
    const id = appState.activeNamespaceBlockId;
    if (renamingHeader && id !== renamingBlockId) {
      renamingHeader = false;
      renamingBlockId = null;
    }
  });

  // Programmatic focus — `autofocus` attr doesn't re-fire in Svelte {#if} blocks
  $effect(() => {
    if (renamingHeader && headerRenameInput) {
      tick().then(() => {
        headerRenameInput?.focus();
        headerRenameInput?.select();
      });
    }
  });

  function startHeaderRename() {
    if (!currentBlock) return;
    headerRenameValue = currentBlock.name || currentBlock.namespace.split('::').pop() || '';
    renamingBlockId = currentBlock.id;
    headerRenameCancelled = false;
    renamingHeader = true;
  }

  async function commitHeaderRename() {
    renamingHeader = false;
    const blockId = renamingBlockId;
    renamingBlockId = null;
    if (headerRenameCancelled) return;
    if (!blockId || !currentBlock || blockId !== currentBlock.id) return;
    const newName = headerRenameValue.trim();
    if (!newName || newName === currentBlock.name) return;
    try {
      await api.blocks.update(blockId, { name: newName });
      // Re-navigate to refresh all state (namespace, URL, breadcrumbs)
      await navigateTo(blockId);
      clearCompletionCache();
    } catch (err) {
      console.error('Failed to rename block:', err);
      addToast('Failed to rename block', 'error');
    }
  }

  function handleHeaderRenameKeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Enter') {
      e.preventDefault();
      commitHeaderRename();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      headerRenameCancelled = true;
      renamingHeader = false;
      renamingBlockId = null;
    }
  }

  // Expand/collapse
  let expanding = $state(false);

  async function handleExpandAll() {
    if (expanding) return;
    expanding = true;
    try {
      const maxDepth = (getSetting<number>('max_expand_depth') ?? 0) || Infinity;
      await expandAll(treeRoots.map(r => r.id), maxDepth);
    } finally {
      expanding = false;
    }
  }

  function handleCollapseAll() {
    collapseAll();
  }

  // Load children and auto-expand the current block when namespace changes.
  // The current block is the tree root, so it must always be expanded
  // for its children to be visible. Uses per-instance state.
  $effect(() => {
    const nsId = isActiveOutliner
      ? appState.activeNamespaceBlockId
      : getOutliner(outlinerId)?.activeNamespaceBlockId ?? null;
    if (nsId) {
      blockTree.loadChildrenWithContent(nsId);
      // Always expand the virtual root so children are visible
      const expanded = isActiveOutliner
        ? appState.expandedBlocks
        : getOutliner(outlinerId)?.expandedBlocks;
      if (expanded && !expanded.has(nsId)) {
        expanded.add(nsId);
      }
    }
  });

  /**
   * Extend selection from anchor in the given direction (+1 = down, -1 = up).
   * Selects a contiguous range from anchor to cursor in the flat list.
   */
  function extendSelection(direction: 1 | -1) {
    if (flatNodes.length === 0) return;

    const selectedIds = appState.selectedBlockIds;
    // Set anchor if not yet set
    if (!appState.selectionAnchor && selectedIds.length > 0) {
      appState.selectionAnchor = selectedIds[0];
    }
    const anchor = appState.selectionAnchor;
    if (!anchor) return;

    const anchorIndex = flatNodes.findIndex(n => n.id === anchor);
    if (anchorIndex < 0) return;

    // Current cursor is the edge of the selection furthest from anchor
    // in the direction of movement
    const cursorId = direction > 0
      ? selectedIds[selectedIds.length - 1]
      : selectedIds[0];
    let cursorIndex = flatNodes.findIndex(n => n.id === cursorId);
    if (cursorIndex < 0) cursorIndex = anchorIndex;

    // Move cursor
    const newCursorIndex = Math.max(0, Math.min(cursorIndex + direction, flatNodes.length - 1));

    // Select range from anchor to new cursor
    const start = Math.min(anchorIndex, newCursorIndex);
    const end = Math.max(anchorIndex, newCursorIndex);
    const ids = flatNodes.slice(start, end + 1).map(n => n.id);
    selectBlocks(ids);
    scrollIntoView(flatNodes[newCursorIndex].id);
  }

  function handleKeydown(e: KeyboardEvent) {
    // In edit mode, Outliner handles NO keys — BlockEditor's CM6 keybindings handle everything
    if (appState.mode === 'edit') return;
    handleNavigationKeydown(e);
  }

  async function handleIndentOutdent(ids: Uuid[], outdent: boolean) {
    if (outdent) {
      await outdentBlocks(ids);
    } else {
      await indentBlocks(ids);
    }
    selectBlocks(ids);
  }

  async function handleDeleteSelected(ids: Uuid[]) {
    try {
      // Collect parent IDs before deleting so we know which subtrees to refresh
      const parentIds = new Set<Uuid>();
      let hasRootBlock = false;
      for (const id of ids) {
        const node = blockTree.getNode(id);
        if (node?.parent_id) {
          parentIds.add(node.parent_id);
        } else {
          hasRootBlock = true;
        }
      }

      for (const id of ids) {
        await api.blocks.delete(id);
      }
      selectBlock(null);

      // Reload the specific parents whose children changed
      if (parentIds.size > 0) {
        await reloadAffectedParents([...parentIds]);
      }
      if (hasRootBlock || parentIds.size === 0) {
        await reloadCurrentTree();
      }
      clearCompletionCache();
    } catch (err) {
      console.error('Failed to delete block(s):', err);
      addToast('Failed to delete block(s)', 'error');
    }
  }

  function handleNavigationKeydown(e: KeyboardEvent) {
    const selectedIds = appState.selectedBlockIds;
    const selectedId = selectedIds.length > 0 ? selectedIds[selectedIds.length - 1] : null;
    const currentIndex = flatNodes.findIndex(n => n.id === selectedId);

    switch (e.key) {
      case 'ArrowDown': {
        e.preventDefault();
        if (e.shiftKey) {
          extendSelection(1);
        } else {
          const nextIndex = Math.min(currentIndex + 1, flatNodes.length - 1);
          if (nextIndex >= 0) {
            selectBlock(flatNodes[nextIndex].id);
            scrollIntoView(flatNodes[nextIndex].id);
          }
        }
        break;
      }
      case 'ArrowUp': {
        e.preventDefault();
        if (e.shiftKey) {
          extendSelection(-1);
        } else {
          const prevIndex = Math.max(currentIndex - 1, 0);
          if (prevIndex >= 0 && flatNodes.length > 0) {
            selectBlock(flatNodes[prevIndex].id);
            scrollIntoView(flatNodes[prevIndex].id);
          }
        }
        break;
      }
      case 'ArrowRight': {
        e.preventDefault();
        if (selectedId) {
          const node = flatNodes[currentIndex];
          if (node?.children.length > 0) {
            if (!appState.expandedBlocks.has(selectedId)) {
              toggleExpanded(selectedId);
            } else {
              selectBlock(node.children[0].id);
            }
          }
        }
        break;
      }
      case 'ArrowLeft': {
        e.preventDefault();
        if (selectedId) {
          const node = flatNodes[currentIndex];
          if (node && appState.expandedBlocks.has(selectedId) && node.children.length > 0) {
            toggleExpanded(selectedId);
          } else if (node?.parent_id) {
            selectBlock(node.parent_id);
            scrollIntoView(node.parent_id);
          }
        }
        break;
      }
      case 'Enter': {
        e.preventDefault();
        if (selectedId) {
          enterEditMode(selectedId);
        }
        break;
      }
      case 'Escape': {
        e.preventDefault();
        selectBlock(null);
        break;
      }
      case 'Tab': {
        e.preventDefault();
        const ids = appState.selectedBlockIds;
        if (ids.length > 0) {
          handleIndentOutdent(ids, e.shiftKey);
        }
        break;
      }
      case 'Delete':
      case 'Backspace': {
        e.preventDefault();
        const ids = appState.selectedBlockIds;
        if (ids.length > 0) {
          handleDeleteSelected(ids);
        }
        break;
      }
    }
  }

  function scrollIntoView(blockId: string) {
    if (!containerEl) return;
    const el = containerEl.querySelector(`[data-block-id="${blockId}"]`);
    el?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }

  function handleBreadcrumbClick(blockId: string) {
    navigateTo(blockId);
  }

  async function handleQuickCreate() {
    const name = quickCreateName.trim();
    if (!name) return;

    const ns = appState.activeNamespaceFullPath ?? '';
    try {
      const result = await api.blocks.create({
        namespace: ns,
        name,
        content: '',
      });
      quickCreateName = '';
      showQuickCreate = false;
      // Reload children
      const nsId = appState.activeNamespaceBlockId;
      if (nsId) {
        await blockTree.loadChildrenWithContent(nsId, true);
      } else {
        await blockTree.loadRoots();
      }
    } catch (err) {
      console.error('Failed to create block:', err);
    }
  }

  function handleQuickCreateKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleQuickCreate();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      showQuickCreate = false;
      quickCreateName = '';
    }
  }

  // Register keyboard/mouse handlers programmatically — the <div role="application">
  // legitimately manages its own focus and keyboard events, but Svelte's a11y
  // checker doesn't recognize `application` as an interactive role.
  onMount(() => {
    if (!containerEl) return;
    containerEl.tabIndex = 0;
    const onKey = (e: KeyboardEvent) => handleKeydown(e);
    const onFocusIn = () => setActiveOutliner(outlinerId, appState);
    const onClick = () => { if (appState.mode === 'navigate') containerEl?.focus(); };
    containerEl.addEventListener('keydown', onKey);
    containerEl.addEventListener('focusin', onFocusIn);
    containerEl.addEventListener('click', onClick);
    return () => {
      containerEl?.removeEventListener('keydown', onKey);
      containerEl?.removeEventListener('focusin', onFocusIn);
      containerEl?.removeEventListener('click', onClick);
    };
  });
</script>

<div
  class="outliner"
  class:active-outliner={isActiveOutliner}
  bind:this={containerEl}
  role="application"
  aria-label="Block outliner"
>
  <!-- Header: breadcrumbs + debug info + content -->
  <div class="outliner-header">
    {#if currentBlock}
      <!-- Breadcrumb trail -->
      <div class="breadcrumbs">
        <button class="crumb clickable" onclick={navigateHome} aria-label="Navigate to root">~</button>
        {#each breadcrumbs as crumb}
          <span class="crumb-sep">::</span>
          <button
            class="crumb clickable"
            onclick={() => handleBreadcrumbClick(crumb.id)}
          >
            {crumb.name}
          </button>
        {/each}
        {#if breadcrumbs.length > 0}
          <span class="crumb-sep">::</span>
        {/if}
        {#if renamingHeader}
          <input
            class="header-rename-input"
            type="text"
            bind:this={headerRenameInput}
            bind:value={headerRenameValue}
            onkeydown={handleHeaderRenameKeydown}
            onblur={commitHeaderRename}
          />
        {:else}
          <button class="crumb current clickable" onclick={startHeaderRename} aria-label="Rename block" title="Click to rename">
            {currentBlock.name || currentBlock.namespace.split('::').pop() || '?'}
          </button>
        {/if}
      </div>

      {#if devMode}
        <div class="debug-info">
          <span>id: {currentBlock.id}</span>
          <span>lineage: {currentBlock.lineage_id}</span>
          <span>ns: {currentBlock.namespace}</span>
          <span>name: {currentBlock.name}</span>
        </div>
      {/if}
    {:else}
      <div class="breadcrumbs">
        <span class="crumb current">Root</span>
      </div>
    {/if}
    <div class="header-actions">
      <button
        class="header-action-btn"
        onclick={handleExpandAll}
        title="Expand all"
        aria-label="Expand all"
        class:disabled={expanding}
      >{expanding ? '⟳' : '⊞'}</button>
      <button
        class="header-action-btn"
        onclick={handleCollapseAll}
        title="Collapse all"
        aria-label="Collapse all"
      >⊟</button>
      <button
        class="header-action-btn"
        onclick={() => { showQuickCreate = !showQuickCreate; }}
        title="Create block"
        aria-label="Create block"
      >+</button>
    </div>
  </div>

  <!-- Quick create bar -->
  {#if showQuickCreate}
    <div class="quick-create-bar">
      <input
        type="text"
        bind:value={quickCreateName}
        onkeydown={handleQuickCreateKeydown}
        placeholder="New block name..."
        class="quick-create-input"
      />
    </div>
  {/if}

  <!-- Block tree — when centered on a block, it is the root node
       with children indented one level beneath it -->
  <div class="outliner-content" role="tree" aria-label="Block outliner">
    {#if treeRoots.length === 0}
      <div class="empty-state">
        <p>No blocks here yet.</p>
        <p class="hint">Create a block to get started.</p>
      </div>
    {:else}
      {#each treeRoots as node (node.id)}
        <OutlinerNode {node} depth={0} />
      {/each}
    {/if}
  </div>

  <!-- Status bar -->
  <div class="outliner-status">
    <span class="status-mode" class:status-mode-edit={appState.mode === 'edit'}>
      {appState.mode === 'edit' ? 'EDIT' : 'NAV'}
    </span>
    <span class="status-sep">|</span>
    <span>{flatNodes.length} blocks</span>
    {#if appState.selectedBlockIds.length > 0}
      <span class="status-sep">|</span>
      <span class="status-selected">
        {#if appState.selectedBlockIds.length === 1}
          {blockTree.getNode(appState.selectedBlockIds[0])?.namespace ?? ''}
        {:else}
          {appState.selectedBlockIds.length} selected
        {/if}
      </span>
    {/if}
    {#if appState.mode === 'edit'}
      <span class="status-hints">
        <kbd>Esc</kbd> done
        <kbd>Enter</kbd> new block
        <kbd>Shift+Enter</kbd> newline
        <kbd>Tab</kbd> indent
        <kbd>Shift+Tab</kbd> outdent
        <kbd>[[</kbd> link
      </span>
    {/if}
  </div>
</div>

<style>
  .outliner {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
    outline: none;
  }

  .outliner:focus-visible {
    outline: none;
  }

  .outliner.active-outliner {
    border-top: 2px solid var(--accent-color);
  }

  .outliner-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
    flex-wrap: wrap;
    gap: 2px;
  }

  .debug-info {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--text-muted);
    opacity: 0.7;
    width: 100%;
    margin-top: 2px;
  }

  .breadcrumbs {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: 12px;
    font-family: var(--font-mono);
  }

  button.crumb {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: inherit;
  }

  .crumb {
    color: var(--text-muted);
  }

  .crumb.clickable {
    cursor: pointer;
    transition: color 0.1s;
  }

  .crumb.clickable:hover {
    color: var(--accent-color);
  }

  .crumb.current {
    color: var(--text-primary);
    font-weight: 600;
  }

  .header-rename-input {
    font-size: 12px;
    font-family: var(--font-mono);
    font-weight: 600;
    color: var(--text-primary);
    background: var(--bg-input, var(--bg-secondary));
    border: 1px solid var(--accent-color);
    border-radius: 3px;
    padding: 1px 6px;
    outline: none;
    min-width: 60px;
  }

  .crumb-sep {
    color: var(--text-muted);
    opacity: 0.5;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .header-action-btn {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 16px;
    color: var(--text-muted);
    transition: color 0.1s;
    line-height: 1;
    padding: 0 2px;
    user-select: none;
  }

  .header-action-btn:hover {
    color: var(--accent-color);
  }

  .header-action-btn.disabled {
    opacity: 0.4;
    pointer-events: none;
  }

  .quick-create-bar {
    padding: 6px 16px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
  }

  .quick-create-input {
    width: 100%;
    padding: 4px 8px;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    border-radius: 3px;
    font-size: 12px;
    outline: none;
    font-family: inherit;
  }

  .quick-create-input:focus {
    border-color: var(--accent-color);
  }

  .outliner-content {
    flex: 1;
    overflow-y: auto;
    padding: 8px 8px 8px 4px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 200px;
    color: var(--text-muted);
    font-size: 14px;
  }

  .empty-state .hint {
    font-size: 12px;
    opacity: 0.6;
  }

  .outliner-status {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 16px;
    border-top: 1px solid var(--border-color);
    font-size: 11px;
    color: var(--text-muted);
    flex-shrink: 0;
    font-family: var(--font-mono);
  }

  .status-sep {
    opacity: 0.3;
  }

  .status-mode {
    font-weight: 700;
    color: var(--text-secondary);
    letter-spacing: 0.05em;
  }

  .status-mode-edit {
    color: var(--accent-color);
  }

  .status-selected {
    color: var(--text-secondary);
  }

  .status-hints {
    margin-left: auto;
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 10px;
    color: var(--text-muted);
  }

  .status-hints kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 0 3px;
    border: 1px solid var(--border-color);
    border-radius: 2px;
    background: var(--bg-tertiary);
    margin-right: 2px;
  }
</style>
