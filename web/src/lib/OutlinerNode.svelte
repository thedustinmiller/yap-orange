<script lang="ts">
  import { onMount, getContext } from 'svelte';
  import type { SvelteSet } from 'svelte/reactivity';
  import type { TreeNode } from './blockTree.svelte';
  import BlockEditor from './BlockEditor.svelte';
  import ContentRenderer from './ContentRenderer.svelte';
  import OutlinerNode from './OutlinerNode.svelte';
  import { blockTree } from './blockTree.svelte';
  import { api } from './api';
  import {
    appState,
    isSelected,
    selectBlock,
    enterEditMode,
    enterNavigationMode,
    getAdjacentBlockId,
    toggleExpanded,
    navigateToLink,
    navigateTo,
    toggleBlockSelection,
    extendSelectionTo,
  } from './appState.svelte';
  import { positionBetween } from './position';
  import { reloadCurrentTree, reloadAffectedParents, indentBlocks, outdentBlocks } from './outlinerActions';
  import { hasCustomView, getCustomView, getViewIcon, preloadViews } from './views/typeViewRegistry';
  import { parseTypeCommand } from './typeCommand';
  import { resolveSchema } from './schemaStore.svelte';
  import { getSetting } from './settingsStore.svelte';
  import { addToast } from './toastStore.svelte';
  import { clearCompletionCache } from './editor/completion';
  import { handleFileDrop } from './fileDrop';
  import ContextMenu from './ContextMenu.svelte';
  import { actions as menuActions, type MenuContext } from './contextMenuActions';

  function focusOnMount(el: HTMLElement) { el.focus(); }

  let { node, depth = 0 }: { node: TreeNode; depth?: number } = $props();

  // Get the per-outliner expandedBlocks set from context.
  // Active outliner uses appState.expandedBlocks; non-active uses instance's own set.
  const getExpandedBlocks = getContext<() => SvelteSet<string>>('get-expanded-blocks');

  // Inline name editing state
  let renamingName = $state(false);
  let renameValue = $state('');

  // Context menu state
  let showContextMenu = $state(false);
  let contextMenuX = $state(0);
  let contextMenuY = $state(0);

  let selected = $derived(isSelected(node.id));
  let isEditing = $derived(appState.editingBlockId === node.id);

  // System content types that don't use EntryView
  const CONTENT_TYPES = new Set(['content', 'raw_text', '', 'namespace', 'setting', 'type_registry', 'schema']);

  // Custom type view — loaded async, replaces BlockEditor/ContentRenderer pair
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let customView = $state<import('svelte').Component<any> | undefined>(undefined);
  let entryView = $state<import('svelte').Component<any> | undefined>(undefined);

  $effect(() => {
    const ct = node.content_type;
    if (hasCustomView(ct)) {
      preloadViews().then(() => {
        customView = getCustomView(ct!);
        entryView = undefined;
      });
    } else if (ct && !CONTENT_TYPES.has(ct)) {
      // Non-system type → use generic EntryView (it handles missing schemas gracefully)
      import('./views/EntryView.svelte').then((mod) => {
        entryView = mod.default;
        customView = undefined;
      });
    } else {
      customView = undefined;
      entryView = undefined;
    }
  });
  let isExpanded = $derived(getExpandedBlocks().has(node.id));
  let hasChildren = $derived(node.children.length > 0);
  let loadingChildren = $state(false);

  // Content checks
  let hasContent = $derived(node.content !== null && node.content.trim() !== '');

  // Auto-expand blocks with empty content (namespace/container blocks).
  // Also trigger a load if the block was already marked expanded (restored
  // from saved settings) but its children haven't been fetched yet.
  // Respects max_expand_depth setting to prevent deep auto-expansion.
  onMount(() => {
    const maxDepth = (getSetting<number>('max_expand_depth') ?? 0) || Infinity;
    if (maxDepth !== Infinity && depth >= maxDepth) return;

    const contentIsEmpty = !node.content || node.content.trim() === '';
    if (contentIsEmpty && !isExpanded) {
      doExpand();
    } else if (isExpanded && !node.childrenLoaded) {
      doExpand();
    }
  });

  async function doExpand() {
    if (!node.childrenLoaded || node.children.some(c => !c.contentLoaded)) {
      loadingChildren = true;
      try {
        await blockTree.loadChildrenWithContent(node.id);
      } catch {
        console.warn('Failed to expand block:', node.id);
      } finally {
        loadingChildren = false;
      }
    }
    if (!getExpandedBlocks().has(node.id)) {
      getExpandedBlocks().add(node.id);
    }
  }

  async function handleToggleExpand(e: MouseEvent) {
    e.stopPropagation();
    if (isExpanded) {
      getExpandedBlocks().delete(node.id);
    } else {
      await doExpand();
    }
  }

  /** Click on the row (indent, icon, name-hover area) — select in nav mode.
   *  Ctrl/Meta+Click toggles this block in/out of selection.
   *  Shift+Click range-selects from anchor to this block.
   *  Plain click enters nav mode and selects only this block. */
  function handleRowClick(e: MouseEvent) {
    if (e.ctrlKey || e.metaKey) {
      toggleBlockSelection(node.id);
    } else if (e.shiftKey) {
      extendSelectionTo(node.id);
    } else {
      enterNavigationMode(node.id);
    }
  }

  /** Click on the content area — enter edit mode.
   *  Skip for empty-label clicks so dblclick-to-rename can work. */
  function handleContentClick(e: MouseEvent) {
    e.stopPropagation();
    const target = e.target as HTMLElement;
    if (target.classList.contains('empty-label')) return;
    enterEditMode(node.id);
  }

  async function handleSave(content: string) {
    try {
      const typeCmd = parseTypeCommand(content);
      if (typeCmd) {
        // @type{...} command — set content_type, extract properties, pin schema version
        const schema = await resolveSchema(typeCmd.typeName, node.namespace);
        const schemaAtomId = schema ? (await api.atoms.get(schema.lineage_id)).id : undefined;
        await api.atoms.update(node.lineage_id, {
          content: '',
          content_type: typeCmd.typeName,
          properties: {
            ...(node.properties ?? {}),
            ...typeCmd.values,
            ...(schemaAtomId ? { _schema_atom_id: schemaAtomId } : {}),
          },
        });
      } else {
        await api.atoms.update(node.lineage_id, { content });
      }
      await blockTree.loadContent(node.id, true);
      clearCompletionCache();
    } catch (err) {
      console.error('Failed to save:', err);
    }
  }

  function handleNavigateLink(path: string) {
    navigateToLink(path);
  }

  function handleCenterOn(e: MouseEvent) {
    e.stopPropagation();
    navigateTo(node.id);
  }

  /** Edit-mode: navigate to adjacent block (ArrowUp at top / ArrowDown at bottom) */
  function handleEditorNavigate(direction: 'prev' | 'next') {
    const adjacentId = getAdjacentBlockId(node.id, direction);
    if (adjacentId) {
      enterEditMode(adjacentId, direction === 'prev' ? 'end' : 'start');
    } else {
      enterNavigationMode(node.id);
    }
  }

  /** Enter in edit mode: create a new sibling block below and jump to it */
  async function handleCreateBlock() {
    const ns = node.namespace;
    // Split namespace to get the parent path (everything but the last segment)
    const parts = ns.split('::');
    parts.pop();
    const parentNs = parts.join('::');

    // Generate a short timestamp-based name for the new block
    const now = new Date();
    const name = `note-${now.getFullYear()}${String(now.getMonth()+1).padStart(2,'0')}${String(now.getDate()).padStart(2,'0')}-${String(now.getHours()).padStart(2,'0')}${String(now.getMinutes()).padStart(2,'0')}${String(now.getSeconds()).padStart(2,'0')}`;

    // Compute position between this block and its next sibling
    const parent = node.parent_id ? blockTree.getNode(node.parent_id) : null;
    const siblings = parent ? parent.children : blockTree.roots;
    const idx = siblings.findIndex(c => c.id === node.id);
    const nextSibling = idx >= 0 && idx < siblings.length - 1 ? siblings[idx + 1] : null;
    const position = positionBetween(node.position, nextSibling?.position ?? null);

    try {
      const result = await api.blocks.create({
        namespace: parentNs,
        name,
        content: '',
        position,
      });

      // Reload the parent's children so the new block appears
      if (node.parent_id) {
        await blockTree.loadChildrenWithContent(node.parent_id, true);
      } else {
        await blockTree.loadRoots();
      }

      // Enter edit mode on the newly created block
      enterEditMode(result.block_id, 'start');
      clearCompletionCache();
    } catch (err) {
      console.error('Failed to create block:', err);
    }
  }

  /** Tab in edit mode: indent this block (reparent under previous sibling) */
  async function handleIndent() {
    await indentBlocks([node.id]);
    // Re-enter edit mode (tree reload may have cleared it)
    enterEditMode(node.id, 'end');
  }

  /** Shift+Tab in edit mode: outdent this block (reparent under grandparent) */
  async function handleOutdent() {
    await outdentBlocks([node.id]);
    enterEditMode(node.id, 'end');
  }

  // --- Inline Rename ---
  let renameCancelled = false;

  function startRename(e?: MouseEvent | KeyboardEvent) {
    e?.stopPropagation();
    renameValue = node.name;
    renameCancelled = false;
    renamingName = true;
  }

  async function commitRename() {
    renamingName = false;
    if (renameCancelled) return;
    const newName = renameValue.trim();
    if (!newName || newName === node.name) return;
    try {
      await api.blocks.update(node.id, { name: newName });
      // Reload this node's data from server so name + namespace stay in sync
      await blockTree.loadContent(node.id, true);
      // If this is the currently centered block, refresh appState too
      if (appState.activeNamespaceBlockId === node.id) {
        await navigateTo(node.id);
      }
      // Reload parent to update child names in the sidebar
      if (node.parent_id) {
        await blockTree.loadChildren(node.parent_id, true);
      }
      clearCompletionCache();
    } catch (err) {
      console.error('Failed to rename block:', err);
      addToast('Failed to rename block', 'error');
    }
  }

  function handleRenameKeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Enter') {
      e.preventDefault();
      commitRename();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      renameCancelled = true;
      renamingName = false;
    }
  }

  // --- Context Menu ---
  function openContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    contextMenuX = e.clientX;
    contextMenuY = e.clientY;
    showContextMenu = true;
  }

  function openContextMenuFromButton(e: MouseEvent) {
    e.stopPropagation();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenuX = rect.right;
    contextMenuY = rect.bottom;
    showContextMenu = true;
  }

  async function exportSubtree(blockId: string, namespace: string) {
    try {
      const data = await api.importExport.export(blockId);
      const nodeCount = data.nodes?.length ?? 0;
      const edgeCount = data.edges?.length ?? 0;
      const name = namespace.replace(/::/g, '_') || 'export';
      // Download via blob
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = `${name}.json`;
      a.click();
      URL.revokeObjectURL(a.href);
      addToast(`Exported ${nodeCount} nodes, ${edgeCount} edges`, 'info');
    } catch (err) {
      console.error('Failed to export subtree:', err);
      addToast('Failed to export subtree', 'error');
    }
  }

  // Filter actions based on node visibility predicates
  let visibleActions = $derived(
    menuActions.filter(a => !a.visible || a.visible(node))
  );

  let menuCtx: MenuContext = {
    startRename: () => {
      renameValue = node.name;
      renamingName = true;
    },
    navigateTo: (id: string) => navigateTo(id),
    exportSubtree,
    reloadTree: async () => {
      await reloadCurrentTree();
      await reloadAffectedParents([node.id]);
    },
  };

  // --- Drag and Drop ---
  let dropZone: 'above' | 'below' | 'inside' | null = $state(null);

  function handleDragStart(e: DragEvent) {
    if (!e.dataTransfer) return;
    e.dataTransfer.effectAllowed = 'move';
    // If dragging a selected block, drag all selected; otherwise just this one
    const ids = appState.selectedBlockIds.includes(node.id)
      ? appState.selectedBlockIds
      : [node.id];
    e.dataTransfer.setData('application/yap-block-ids', JSON.stringify(ids));
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    if (!e.dataTransfer) return;
    // External files get 'copy' indicator; internal blocks get 'move'
    e.dataTransfer.dropEffect = e.dataTransfer.types.includes('Files') ? 'copy' : 'move';
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const y = (e.clientY - rect.top) / rect.height;
    dropZone = y < 0.25 ? 'above' : y > 0.75 ? 'below' : 'inside';
  }

  function handleDragLeave() {
    dropZone = null;
  }

  function computeDropPosition(zone: 'above' | 'below'): string {
    const parent = node.parent_id ? blockTree.getNode(node.parent_id) : null;
    const siblings = parent ? parent.children : blockTree.roots;
    const idx = siblings.findIndex(c => c.id === node.id);

    if (zone === 'above') {
      const before = idx > 0 ? siblings[idx - 1].position : null;
      return positionBetween(before, node.position);
    } else {
      const after = idx < siblings.length - 1 ? siblings[idx + 1].position : null;
      return positionBetween(node.position, after);
    }
  }

  async function handleDrop(e: DragEvent) {
    e.preventDefault();
    const zone = dropZone;
    dropZone = null;
    if (!e.dataTransfer || !zone) return;

    // External file drop — dispatch by type
    if (e.dataTransfer.files.length > 0) {
      try {
        const results = await handleFileDrop(
          e.dataTransfer.files,
          zone,
          node,
          computeDropPosition,
        );
        await reloadCurrentTree();
        await reloadAffectedParents([zone === 'inside' ? node.id : (node.parent_id ?? '')].filter(Boolean));
        if (zone === 'inside') getExpandedBlocks().add(node.id);

        const mediaCount = results.filter(r => r.type === 'media').length;
        const importCount = results.filter(r => r.type === 'import').length;
        if (mediaCount) addToast(`Added ${mediaCount} file(s)`, 'info');
        if (importCount) addToast(`Imported ${importCount} tree(s)`, 'info');
      } catch (err) {
        console.error('File drop failed:', err);
        addToast('File drop failed', 'error');
      }
      return;
    }

    // Internal block reorder
    const raw = e.dataTransfer.getData('application/yap-block-ids');
    if (!raw) return;
    const draggedIds: string[] = JSON.parse(raw);

    // Don't drop on self
    if (draggedIds.includes(node.id)) return;

    // Don't drop onto a descendant of any dragged block (would create cycle)
    function isAncestorOf(potentialAncestorId: string, targetId: string): boolean {
      let walkId: string | null | undefined = targetId;
      while (walkId) {
        if (walkId === potentialAncestorId) return true;
        walkId = blockTree.getNode(walkId)?.parent_id;
      }
      return false;
    }

    const effectiveTarget = zone === 'inside' ? node.id : (node.parent_id ?? null);
    if (effectiveTarget && draggedIds.some(id => isAncestorOf(id, effectiveTarget))) return;

    try {
      const oldParentIds: string[] = [];
      const newParentIds: string[] = [];

      // For above/below drops with multiple blocks, each subsequent block
      // needs a position after the previous one to maintain ordering.
      let lastAssignedPosition: string | null = null;

      for (const id of draggedIds) {
        const dragged = blockTree.getNode(id);
        if (dragged?.parent_id) oldParentIds.push(dragged.parent_id);

        if (zone === 'inside') {
          newParentIds.push(node.id);
          await api.blocks.move(id, { parent_id: node.id });
        } else {
          if (node.parent_id) newParentIds.push(node.parent_id);
          let position: string;
          if (lastAssignedPosition === null) {
            // First block: use the standard drop position computation
            position = computeDropPosition(zone);
          } else {
            // Subsequent blocks: place after the last assigned position
            // (before the next sibling from the original computation)
            const parent = node.parent_id ? blockTree.getNode(node.parent_id) : null;
            const siblings = parent ? parent.children : blockTree.roots;
            const idx = siblings.findIndex(c => c.id === node.id);
            const nextBound = zone === 'above'
              ? node.position
              : (idx < siblings.length - 1 ? siblings[idx + 1].position : null);
            position = positionBetween(lastAssignedPosition, nextBound);
          }
          lastAssignedPosition = position;
          await api.blocks.move(id, { parent_id: node.parent_id, position });
        }
      }

      await reloadCurrentTree();
      await reloadAffectedParents([...oldParentIds, ...newParentIds]);

      // Expand the drop target so dropped-inside blocks are visible
      if (zone === 'inside') getExpandedBlocks().add(node.id);
    } catch (err) {
      console.error('Failed to move block(s):', err);
    }
  }
</script>

<div
  class="outliner-node"
  class:selected={selected}
  class:editing={isEditing}
  data-block-id={node.id}
  style="--depth: {depth}"
  role="treeitem"
  aria-expanded={isExpanded}
  aria-selected={selected}
  aria-level={depth + 1}
>
  <div
    class="node-row"
    class:drop-above={dropZone === 'above'}
    class:drop-below={dropZone === 'below'}
    class:drop-inside={dropZone === 'inside'}
    role="presentation"
    draggable="true"
    onclick={handleRowClick}
    onkeydown={(e) => { if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey && !e.altKey) handleRowClick(e as unknown as MouseEvent); }}
    oncontextmenu={openContextMenu}
    ondragstart={handleDragStart}
    ondragover={handleDragOver}
    ondragleave={handleDragLeave}
    ondrop={handleDrop}
  >
    <!-- Indent -->
    <div class="node-indent" style="width: {depth * 24}px"></div>

    <!-- Expand toggle — always visible (children load lazily) -->
    <button
      class="node-bullet"
      class:expanded={isExpanded}
      onclick={handleToggleExpand}
      aria-label={isExpanded ? 'Collapse' : 'Expand'}
    >
      {#if loadingChildren}
        <span class="loading-spinner">&#x27F3;</span>
      {:else}
        <svg width="12" height="12" viewBox="0 0 12 12">
          <path d="M4 2 L9 6 L4 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      {/if}
    </button>

    <!-- Block type icon — registry-driven for custom types, fallback for standard -->
    <span class="node-icon">
      {#if getViewIcon(node.content_type)}
        {getViewIcon(node.content_type)}
      {:else if entryView}
        ◈
      {:else if !hasContent}
        &#x1F4C1;
      {:else if hasChildren}
        &#x1F4CB;
      {:else}
        &#x1F4C4;
      {/if}
    </span>

    <!-- Content area — clicking enters edit mode for all block types -->
    <div class="node-content" role="presentation" onclick={handleContentClick} onkeydown={(e) => { if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey && !e.altKey) handleContentClick(e as unknown as MouseEvent); }}>
      {#if customView}
        {@const CustomView = customView}
        <CustomView {node} {isEditing} />
      {:else if entryView}
        {@const EntryViewComponent = entryView}
        <EntryViewComponent {node} {isEditing} />
      {:else if node.content_type === 'raw_text' && hasContent}
        <pre class="raw-text">{node.content}</pre>
      {:else if isEditing}
        <BlockEditor
          initialContent={node.content ?? ''}
          blockId={node.id}
          initialCursorPosition={appState.editCursorHint}
          onSave={handleSave}
          onNavigateToBlock={handleEditorNavigate}
          onCreateBlock={handleCreateBlock}
          onIndent={handleIndent}
          onOutdent={handleOutdent}
        />
      {:else if hasContent}
        <div class="content-text">
          <ContentRenderer
            content={node.content ?? ''}
            onNavigateLink={handleNavigateLink}
          />
        </div>
      {:else}
        <span class="empty-label" role="button" tabindex="-1" ondblclick={startRename} onkeydown={(e) => { if (e.key === 'Enter') startRename(); }}>{node.name || 'Empty block'}</span>
      {/if}
    </div>

    <!-- Block name on hover — double-click to rename -->
    {#if renamingName}
      <input
        class="rename-input"
        type="text"
        bind:value={renameValue}
        onkeydown={handleRenameKeydown}
        onblur={commitRename}
        onclick={(e) => e.stopPropagation()}
        use:focusOnMount
      />
    {:else}
      <span class="node-name-hover" role="button" tabindex="-1" ondblclick={startRename} onkeydown={(e) => { if (e.key === 'Enter') startRename(); }}>
        {#if node.name}
          {node.name}
        {/if}
      </span>
    {/if}

    <!-- Center perspective button — navigate INTO this block -->
    <button
      class="center-btn"
      onclick={handleCenterOn}
      title="Center on this block"
      aria-label="Center on this block"
    >&#x29BE;</button>

    <!-- Three-dot context menu button -->
    <button
      class="context-menu-btn"
      onclick={openContextMenuFromButton}
      title="Actions"
      aria-label="Actions"
    >⋯</button>
  </div>

  <!-- Children -->
  {#if isExpanded && hasChildren}
    <div class="node-children" role="group">
      {#each node.children as child (child.id)}
        <OutlinerNode node={child} depth={depth + 1} />
      {/each}
    </div>
  {/if}

  <!-- No children indicator removed — absence is visually obvious -->

  <!-- Context menu (rendered via portal at fixed position) -->
  {#if showContextMenu}
    <ContextMenu
      x={contextMenuX}
      y={contextMenuY}
      {node}
      actions={visibleActions}
      ctx={menuCtx}
      onclose={() => { showContextMenu = false; }}
    />
  {/if}
</div>

<style>
  .outliner-node {
    position: relative;
  }

  .node-row {
    display: flex;
    align-items: flex-start;
    padding: 3px 8px 3px 0;
    cursor: pointer;
    border-radius: 3px;
    transition: background 0.08s;
    min-height: 28px;
  }

  .node-row:hover {
    background: var(--bg-hover);
  }

  .selected > .node-row {
    background: var(--bg-selected);
  }

  .selected > .node-row:hover {
    background: var(--bg-selected-hover);
  }

  .editing > .node-row {
    background: var(--bg-editing);
  }

  .node-indent {
    flex-shrink: 0;
  }

  .node-bullet {
    background: none;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    color: var(--text-muted);
    cursor: pointer;
    transition: transform 0.15s;
    margin-top: 2px;
    padding: 0;
  }

  .node-bullet:hover {
    color: var(--text-primary);
  }

  .node-bullet.expanded {
    transform: rotate(90deg);
  }

  .loading-spinner {
    font-size: 12px;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .node-icon {
    font-size: 14px;
    flex-shrink: 0;
    margin-top: 1px;
    margin-right: 4px;
  }

  .node-content {
    flex: 1;
    min-width: 0;
    padding-top: 1px;
    line-height: 1.5;
    font-size: 13px;
  }

  .content-text {
    color: var(--text-primary);
  }

  .raw-text {
    margin: 0;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    color: var(--text-primary);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .empty-label {
    color: var(--text-muted);
    font-style: italic;
    font-size: 13px;
  }

  .node-name-hover {
    flex-shrink: 0;
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-muted);
    opacity: 0;
    transition: opacity 0.15s;
  }

  .node-row:hover .node-name-hover {
    opacity: 1;
  }

  .rename-input {
    flex-shrink: 0;
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-primary);
    background: var(--bg-input, var(--bg-secondary));
    border: 1px solid var(--accent-color);
    border-radius: 3px;
    padding: 1px 4px;
    outline: none;
    max-width: 180px;
  }

  .center-btn {
    background: none;
    border: none;
    flex-shrink: 0;
    font-size: 14px;
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s, color 0.1s;
    margin-left: 4px;
    line-height: 1;
    padding: 0;
  }

  .node-row:hover .center-btn {
    opacity: 0.6;
  }

  .center-btn:hover {
    opacity: 1 !important;
    color: var(--accent-color);
  }

  .context-menu-btn {
    background: none;
    border: none;
    flex-shrink: 0;
    font-size: 16px;
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s, color 0.1s;
    margin-left: 2px;
    line-height: 1;
    padding: 0 2px;
    user-select: none;
    letter-spacing: 1px;
  }

  .node-row:hover .context-menu-btn {
    opacity: 0.6;
  }

  .context-menu-btn:hover {
    opacity: 1 !important;
    color: var(--accent-color);
  }

  .node-children {
    position: relative;
  }

  .node-children::before {
    content: '';
    position: absolute;
    left: calc(var(--depth) * 24px + 20px);
    top: 0;
    bottom: 8px;
    width: 1px;
    background: var(--border-color);
    opacity: 0.4;
  }

  /* Drag and drop indicators */
  .node-row.drop-above {
    border-top: 2px solid var(--accent-color);
  }

  .node-row.drop-below {
    border-bottom: 2px solid var(--accent-color);
  }

  .node-row.drop-inside {
    background: var(--bg-active, var(--bg-selected));
  }

</style>
