/**
 * Application state.
 *
 * Key concept: "virtual root" / "center perspective"
 * navigateTo(blockId) centers the outliner on any block — showing
 * its content at the top and children below. Both sidebar clicks
 * and the center perspective button use the same code path.
 */

import { SvelteSet } from 'svelte/reactivity'
import { api } from './api'
import { blockTree, type TreeNode } from './blockTree.svelte'
import { addToast } from './toastStore.svelte'
import type { Uuid, BlockWithContent } from './types'

// Route pusher — registered by router.svelte.ts to avoid circular imports
let _routePusher: ((path: string) => void) | null = null

/** Called by router.svelte.ts once at init. */
export function registerRoutePusher(fn: (path: string) => void): void {
  _routePusher = fn
}

/** Breadcrumb entry for navigating back up the tree */
export interface Breadcrumb {
  id: Uuid
  name: string
  namespace: string
}

// --- Core app state ---
export const appState = $state({
  /** Current interaction mode */
  mode: 'navigate' as 'navigate' | 'edit',
  /** Cursor placement hint when entering edit mode */
  editCursorHint: 'end' as 'start' | 'end',
  /** Currently selected block IDs (multi-select) */
  selectedBlockIds: [] as Uuid[],
  /** Anchor block for shift-selection range */
  selectionAnchor: null as Uuid | null,
  /** Block currently being edited */
  editingBlockId: null as Uuid | null,
  /** Set of expanded block IDs in the outliner */
  expandedBlocks: new SvelteSet<string>(),
  /** Currently active namespace block ID (what the outliner shows) */
  activeNamespace: null as Uuid | null,
  /** Sidebar expanded nodes */
  sidebarExpanded: new SvelteSet<string>(),

  // Navigation context
  activeNamespaceBlockId: null as Uuid | null,
  activeNamespaceLineageId: null as Uuid | null,
  activeNamespaceName: null as string | null,
  activeNamespaceFullPath: null as string | null,

  /** The block we're currently centered on (virtual root).
   *  Its content is rendered at the top of the outliner. */
  currentBlock: null as BlockWithContent | null,

  /** The active tree node (same as getNode(activeNamespaceBlockId) but reactive) */
  activeTreeNode: null as any,

  /** Breadcrumb trail from the sidebar root to the current block */
  breadcrumbs: [] as Breadcrumb[],

  /** Server connection status */
  serverConnected: false,
  /** Set to true by App.svelte after initial navigation completes */
  navigationReady: false,
  /** Whether any editor has unsaved changes (for beforeunload warning) */
  hasUnsavedChanges: false,
})

// --- Actions ---

export function selectBlock(blockId: Uuid | null) {
  appState.selectedBlockIds = blockId ? [blockId] : []
  appState.selectionAnchor = blockId
}

export function selectBlocks(blockIds: Uuid[]) {
  appState.selectedBlockIds = blockIds
}

export function isSelected(blockId: Uuid): boolean {
  return appState.selectedBlockIds.includes(blockId)
}

// --- Mode transition functions (the ONLY way to change modes) ---

/**
 * Enter navigation mode. Optionally select a block.
 * Clears editing state.
 */
export function enterNavigationMode(blockId?: Uuid | null) {
  appState.mode = 'navigate'
  appState.editingBlockId = null
  if (blockId != null) {
    appState.selectedBlockIds = [blockId]
    appState.selectionAnchor = blockId
  }
}

/**
 * Enter edit mode for a specific block.
 * Sets cursor placement hint for BlockEditor.
 */
export function enterEditMode(blockId: Uuid, cursor: 'start' | 'end' = 'end') {
  appState.mode = 'edit'
  appState.editingBlockId = blockId
  appState.selectedBlockIds = [blockId]
  appState.selectionAnchor = blockId
  appState.editCursorHint = cursor
}

/**
 * Get the adjacent block ID in the visible tree.
 * Used for edit-mode block-to-block navigation (ArrowUp at top, ArrowDown at bottom).
 */
export function getAdjacentBlockId(blockId: Uuid, direction: 'prev' | 'next'): Uuid | null {
  // Compute treeRoots from current appState
  const nsId = appState.activeNamespaceBlockId
  let roots: TreeNode[]
  if (nsId) {
    const node = blockTree.getNode(nsId)
    roots = node ? [node] : []
  } else {
    roots = blockTree.roots
  }

  const flat = flattenVisibleNodes(roots)
  const idx = flat.findIndex(n => n.id === blockId)
  if (idx < 0) return null

  if (direction === 'prev' && idx > 0) return flat[idx - 1].id
  if (direction === 'next' && idx < flat.length - 1) return flat[idx + 1].id
  return null
}

// --- Deprecated aliases (delegate to mode functions) ---

/** @deprecated Use enterEditMode() instead */
export function startEditing(blockId: Uuid) {
  enterEditMode(blockId)
}

/** @deprecated Use enterNavigationMode() instead */
export function stopEditing() {
  enterNavigationMode()
}

export function toggleExpanded(blockId: Uuid) {
  if (appState.expandedBlocks.has(blockId)) {
    appState.expandedBlocks.delete(blockId)
  } else {
    appState.expandedBlocks.add(blockId)
  }
}

/**
 * Collapse all expanded blocks in the outliner.
 * Preserves the virtual root (if navigated into a block) so that
 * child components don't unmount/remount — which would trigger
 * their onMount auto-expand logic and undo the collapse.
 */
export function collapseAll() {
  const virtualRoot = appState.activeNamespaceBlockId
  appState.expandedBlocks.clear()
  if (virtualRoot) {
    appState.expandedBlocks.add(virtualRoot)
  }
}

/**
 * Expand all nodes recursively up to maxDepth.
 * Loads children for each node as it goes.
 * Works with block IDs and looks up fresh node references from blockTree
 * to avoid Svelte $state double-proxy issues when nodes are stored in
 * appState.activeTreeNode.
 */
export async function expandAll(rootIds: Uuid[], maxDepth: number = Infinity) {
  async function expandRecursive(nodeId: Uuid, depth: number) {
    if (depth >= maxDepth) return
    const node = blockTree.getNode(nodeId)
    if (!node) return
    appState.expandedBlocks.add(nodeId)
    if (!node.childrenLoaded) {
      await blockTree.loadChildrenWithContent(nodeId)
    }
    // Re-read node to get fresh children after load
    const updated = blockTree.getNode(nodeId)
    if (!updated) return
    await Promise.all(
      updated.children.map(child => expandRecursive(child.id, depth + 1))
    )
  }
  await Promise.all(rootIds.map(id => expandRecursive(id, 0)))
}

export function toggleSidebarExpanded(blockId: Uuid) {
  if (appState.sidebarExpanded.has(blockId)) {
    appState.sidebarExpanded.delete(blockId)
  } else {
    appState.sidebarExpanded.add(blockId)
  }
}

/**
 * Set the active namespace — used by the sidebar to select a namespace.
 * This now delegates to navigateTo() for unified behavior.
 */
export function setActiveNamespace(
  blockId: Uuid | null,
  lineageId?: Uuid | null,
  namespace?: string | null,
  name?: string | null,
) {
  if (blockId) {
    navigateTo(blockId)
  } else {
    // Navigate to the root
    navigateHome()
  }
}

/**
 * Navigate home — show all root blocks.
 */
export function navigateHome() {
  appState.mode = 'navigate'
  appState.editingBlockId = null
  appState.activeNamespace = null
  appState.activeNamespaceBlockId = null
  appState.activeNamespaceLineageId = null
  appState.activeNamespaceFullPath = null
  appState.activeNamespaceName = null
  appState.currentBlock = null
  appState.activeTreeNode = null
  appState.breadcrumbs = []
  appState.selectedBlockIds = []
  appState.selectionAnchor = null
  _routePusher?.('/')
}

/**
 * Navigate to (center on) a specific block by ID.
 * This is the unified entry point — both sidebar clicks and the
 * center perspective button call this. It fetches the block's full
 * data, builds breadcrumbs, and loads children.
 */
export async function navigateTo(blockId: Uuid) {
  try {
    const block = await api.blocks.get(blockId)

    // Set all namespace state
    appState.mode = 'navigate'
    appState.editingBlockId = null
    appState.activeNamespace = blockId
    appState.activeNamespaceBlockId = blockId
    appState.activeNamespaceLineageId = block.lineage_id
    appState.activeNamespaceFullPath = block.namespace
    appState.activeNamespaceName = block.name
    appState.currentBlock = block
    appState.selectedBlockIds = []
    appState.selectionAnchor = null

    // Build breadcrumb trail by walking parent_id chain
    const crumbs: Breadcrumb[] = []
    let walkId = block.parent_id
    while (walkId) {
      try {
        const parent = await api.blocks.get(walkId)
        crumbs.unshift({
          id: parent.id,
          name: parent.name || parent.namespace.split('::').pop() || '?',
          namespace: parent.namespace,
        })
        walkId = parent.parent_id
      } catch {
        console.warn('Failed to load breadcrumb parent:', walkId)
        break
      }
    }
    appState.breadcrumbs = crumbs

    // Ensure the block is in the tree with its content populated,
    // then load children
    const treeNode = blockTree.ensureNode(block)
    treeNode.content = block.content
    treeNode.content_type = block.content_type
    treeNode.properties = block.properties
    treeNode.contentLoaded = true
    await blockTree.loadChildrenWithContent(blockId)

    // Set reactive tree node reference so Outliner can derive from it
    appState.activeTreeNode = treeNode

    _routePusher?.(block.namespace)
  } catch (err) {
    console.error('Failed to navigate to block:', err)
  }
}

/**
 * Navigate to a specific block — determines the right namespace context
 * and selects the block.
 */
export async function navigateToBlock(block: BlockWithContent) {
  if (block.parent_id) {
    // Non-root block: navigate to its parent namespace and highlight this block
    try {
      const parent = await api.blocks.get(block.parent_id)
      await navigateTo(parent.id)
    } catch {
      console.warn('navigateToBlock: parent fetch failed, going home')
      navigateHome()
    }
  } else {
    // Root block: navigate INTO it (treat as namespace, show its children)
    await navigateTo(block.id)
  }
  selectBlock(block.id)
}

/**
 * Navigate to a wiki link path — resolves it and navigates.
 */
export async function navigateToLink(path: string) {
  try {
    const result = await api.resolve({
      path,
      from_namespace: appState.activeNamespaceFullPath ?? undefined,
    })
    const block = await api.blocks.get(result.block_id)
    await navigateToBlock(block)
  } catch {
    console.warn('Link resolve failed, creating block:', path)
    // Block doesn't exist — create it, then navigate
    const segments = path.split('::')
    const name = segments.pop()!
    const ns = segments.join('::')
    try {
      const created = await api.blocks.create({
        namespace: ns,
        name,
        content: '',
      })
      const block = await api.blocks.get(created.block_id)
      await navigateToBlock(block)
    } catch {
      console.warn('Failed to create block from link:', path)
      addToast(`Failed to create block from link: ${path}`, 'error')
    }
  }
}

/**
 * Navigate to any block for a given lineage (atom) ID.
 */
export async function navigateToAtom(lineageId: Uuid) {
  try {
    const blocks = await api.blocks.list({ lineage_id: lineageId })
    if (blocks.length > 0) {
      const full = await api.blocks.get(blocks[0].id)
      await navigateToBlock(full)
    }
  } catch {
    console.warn('Failed to navigate to atom:', lineageId)
  }
}

/**
 * Toggle a block in/out of the current selection.
 * Updates the selection anchor to the toggled block.
 */
export function toggleBlockSelection(blockId: Uuid) {
  const idx = appState.selectedBlockIds.indexOf(blockId)
  if (idx >= 0) {
    appState.selectedBlockIds = appState.selectedBlockIds.filter(id => id !== blockId)
  } else {
    appState.selectedBlockIds = [...appState.selectedBlockIds, blockId]
  }
  appState.selectionAnchor = blockId
}

/**
 * Extend selection from anchor to the target block (range select).
 * Used by Shift+Click. Anchor is preserved so further Shift+Clicks extend from the same point.
 */
export function extendSelectionTo(blockId: Uuid) {
  const nsId = appState.activeNamespaceBlockId
  let roots: TreeNode[]
  if (nsId) {
    const node = blockTree.getNode(nsId)
    roots = node ? [node] : []
  } else {
    roots = blockTree.roots
  }

  const flat = flattenVisibleNodes(roots)
  const anchor = appState.selectionAnchor
  if (!anchor) {
    selectBlock(blockId)
    return
  }

  const anchorIndex = flat.findIndex(n => n.id === anchor)
  const targetIndex = flat.findIndex(n => n.id === blockId)
  if (anchorIndex < 0 || targetIndex < 0) return

  const start = Math.min(anchorIndex, targetIndex)
  const end = Math.max(anchorIndex, targetIndex)
  appState.selectedBlockIds = flat.slice(start, end + 1).map(n => n.id)
  // Anchor stays fixed so subsequent Shift+Clicks extend from same origin
}

// --- Computed helpers ---

/**
 * Flatten visible tree nodes (respecting expanded state) for keyboard nav.
 */
export function flattenVisibleNodes(nodes: TreeNode[]): TreeNode[] {
  const result: TreeNode[] = []
  function walk(node: TreeNode) {
    result.push(node)
    if (appState.expandedBlocks.has(node.id) && node.children.length > 0) {
      for (const child of node.children) {
        walk(child)
      }
    }
  }
  for (const node of nodes) {
    walk(node)
  }
  return result
}
