/**
 * Shared reactive tree store for block hierarchy.
 *
 * Both the sidebar and outliner reference the same TreeNode objects.
 */

import { api } from './api'
import { addToast } from './toastStore.svelte'
import type { Uuid, Properties } from './types'

export interface TreeNode {
  id: Uuid
  lineage_id: Uuid
  namespace: string
  name: string
  position: string
  parent_id: Uuid | null

  // Content (lazily loaded by outliner)
  content: string | null   // null = not yet fetched
  contentLoaded: boolean
  content_type: string | null
  properties: Properties | null

  // Children
  children: TreeNode[]
  childrenLoaded: boolean
}

// Module-level singleton — Svelte 5 $state at module scope
let nodeMap = $state(new Map<Uuid, TreeNode>())
let roots: TreeNode[] = $state([])
// Bump this when structural changes (new nodes, children loaded) happen
// so $derived expressions that read it will re-evaluate.
let version = $state(0)

/**
 * Get or create a TreeNode for a block. If the node already exists,
 * update its mutable fields but preserve loaded content and children.
 */
function ensureNode(block: {
  id: Uuid
  lineage_id: Uuid
  namespace: string
  name: string
  position: string
  parent_id: Uuid | null
}): TreeNode {
  const existing = nodeMap.get(block.id)
  if (existing) {
    existing.namespace = block.namespace
    existing.name = block.name
    existing.position = block.position
    existing.parent_id = block.parent_id
    return existing
  }

  const node: TreeNode = $state({
    id: block.id,
    lineage_id: block.lineage_id,
    namespace: block.namespace,
    name: block.name,
    position: block.position,
    parent_id: block.parent_id,
    content: null,
    contentLoaded: false,
    content_type: null,
    properties: null,
    children: [],
    childrenLoaded: false,
  })
  nodeMap.set(block.id, node)
  version++
  return node
}

function sortChildren(arr: TreeNode[]) {
  arr.sort((a, b) => a.position.localeCompare(b.position))
}

/**
 * Load all root-level blocks from the server (parent_id IS NULL).
 * Called by the sidebar on mount and after root-level mutations.
 */
async function loadRoots(): Promise<void> {
  try {
    const blocks = await api.roots.list()

    roots.splice(0, roots.length)

    for (const block of blocks) {
      const node = ensureNode(block)
      if (!node.childrenLoaded) {
        node.children.splice(0, node.children.length)
      }
    }

    for (const block of blocks) {
      const node = nodeMap.get(block.id)!
      if (block.parent_id && nodeMap.has(block.parent_id)) {
        const parent = nodeMap.get(block.parent_id)!
        if (!parent.children.some(c => c.id === node.id)) {
          parent.children.push(node)
        }
      } else {
        if (!roots.some(r => r.id === node.id)) {
          roots.push(node)
        }
      }
    }

    for (const node of nodeMap.values()) {
      sortChildren(node.children)
    }
    sortChildren(roots)
  } catch (err) {
    console.error('Failed to load root blocks:', err)
    addToast('Failed to load blocks', 'error')
  }
}

/**
 * Load all children for a node from the server.
 */
async function loadChildren(nodeId: Uuid, force = false): Promise<void> {
  const node = nodeMap.get(nodeId)
  if (!node) return
  if (node.childrenLoaded && !force) return

  try {
    const children = await api.blocks.children(nodeId)
    const newChildren: TreeNode[] = children.map(c => ensureNode(c))
    sortChildren(newChildren)

    node.children.splice(0, node.children.length, ...newChildren)
    node.childrenLoaded = true
    version++
  } catch (err) {
    console.error('Failed to load children:', nodeId, err)
    throw err
  }
}

/**
 * Load rendered content for a single node.
 */
async function loadContent(nodeId: Uuid, force = false): Promise<void> {
  const node = nodeMap.get(nodeId)
  if (!node) return
  if (node.contentLoaded && !force) return

  try {
    const bwc = await api.blocks.get(nodeId)
    node.content = bwc.content
    node.content_type = bwc.content_type
    node.properties = bwc.properties
    node.namespace = bwc.namespace
    node.name = bwc.name
    node.contentLoaded = true
  } catch (err) {
    console.error('Failed to load content:', nodeId, err)
    node.contentLoaded = true
    node.content = null
  }
}

/**
 * Load children + their content.
 * Used by the outliner so every child has rendered content available.
 */
async function loadChildrenWithContent(nodeId: Uuid, force = false): Promise<void> {
  await loadChildren(nodeId, force)
  const node = nodeMap.get(nodeId)
  if (!node) return

  await Promise.all(
    node.children.map(child => loadContent(child.id, force))
  )
}

/**
 * Load children + their content + grandchildren (one level deeper).
 * Used by the sidebar so child icons can distinguish folder vs leaf.
 */
async function loadChildrenDeep(nodeId: Uuid): Promise<void> {
  await loadChildren(nodeId)
  const node = nodeMap.get(nodeId)
  if (!node) return

  await Promise.all(
    node.children.map(async (child) => {
      await Promise.all([
        loadContent(child.id),
        loadChildren(child.id),
      ])
    })
  )
}

function getNode(id: Uuid): TreeNode | undefined {
  return nodeMap.get(id)
}

export const blockTree = {
  get roots() { return roots },
  get nodeMap() { return nodeMap },
  get version() { return version },
  getNode,
  ensureNode,
  loadRoots,
  loadChildren,
  loadContent,
  loadChildrenWithContent,
  loadChildrenDeep,
}
