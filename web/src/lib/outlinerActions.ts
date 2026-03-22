/**
 * Outliner tree manipulation actions: indent, outdent, reload.
 *
 * These call the server's move API and reload the tree to reflect changes.
 */

import { api } from './api'
import { appState } from './appState.svelte'
import { blockTree } from './blockTree.svelte'
import { positionBetween } from './position'
import type { Uuid } from './types'

/**
 * Reload the children of the current block (or root blocks if at home).
 */
export async function reloadCurrentTree(): Promise<void> {
  const nsId = appState.activeNamespaceBlockId
  if (nsId) {
    await blockTree.loadChildrenWithContent(nsId, true)
  } else {
    await blockTree.loadRoots()
  }
}

/**
 * Reload specific parent nodes' children so the tree reflects moves
 * at any depth, not just the namespace root level.
 */
export async function reloadAffectedParents(parentIds: Uuid[]): Promise<void> {
  const unique = [...new Set(parentIds)]
  await Promise.all(
    unique.map(id => blockTree.loadChildrenWithContent(id, true))
  )
}

/**
 * Indent blocks — reparent each under its previous sibling.
 * This makes the block the last child of the sibling above it.
 * Auto-expands new parent blocks so indented blocks remain visible.
 *
 * When multiple blocks are selected, they all target the same
 * previous sibling (the first non-selected sibling above the group).
 * Moves are computed from the pre-move tree state, then executed
 * sequentially so that position ordering is preserved.
 */
export async function indentBlocks(blockIds: Uuid[]): Promise<void> {
  const blockIdSet = new Set(blockIds)
  const oldParentIds: Uuid[] = []
  const newParentIds: Uuid[] = []

  // Plan all moves from the pre-move tree state
  const moves: { blockId: Uuid; newParentId: Uuid }[] = []
  for (const blockId of blockIds) {
    const node = blockTree.getNode(blockId)
    if (!node?.parent_id) continue

    const parent = blockTree.getNode(node.parent_id)
    if (!parent) continue

    // Find the first previous sibling that is NOT also being indented
    const siblings = parent.children
    const idx = siblings.findIndex(c => c.id === blockId)
    if (idx <= 0) continue

    let targetSibling = null
    for (let i = idx - 1; i >= 0; i--) {
      if (!blockIdSet.has(siblings[i].id)) {
        targetSibling = siblings[i]
        break
      }
    }
    if (!targetSibling) continue

    oldParentIds.push(node.parent_id)
    newParentIds.push(targetSibling.id)
    moves.push({ blockId, newParentId: targetSibling.id })
  }

  // Execute all moves
  for (const { blockId, newParentId } of moves) {
    await api.blocks.move(blockId, { parent_id: newParentId })
  }

  // Reload the namespace root + both old and new parents so the
  // tree properly removes the block from its old position and
  // shows it in the new one
  await reloadCurrentTree()
  await reloadAffectedParents([...oldParentIds, ...newParentIds])

  // Auto-expand new parents so the indented blocks are visible
  for (const parentId of newParentIds) {
    appState.expandedBlocks.add(parentId)
  }
}

/**
 * Outdent blocks — reparent each under its grandparent.
 * This makes the block a sibling of its current parent,
 * positioned immediately after it.
 */
export async function outdentBlocks(blockIds: Uuid[]): Promise<void> {
  const oldParentIds: Uuid[] = []
  const newParentIds: Uuid[] = []
  for (const blockId of blockIds) {
    const node = blockTree.getNode(blockId)
    if (!node?.parent_id) continue

    const parent = blockTree.getNode(node.parent_id)
    if (!parent) continue

    const grandparentId = parent.parent_id // null = becomes root
    oldParentIds.push(node.parent_id)
    if (grandparentId) newParentIds.push(grandparentId)

    // Compute position immediately after the former parent block
    // so the outdented block lands right below it, not at the end.
    const grandparent = grandparentId ? blockTree.getNode(grandparentId) : null
    const gpChildren = grandparent ? grandparent.children : blockTree.roots
    const parentIdx = gpChildren.findIndex(c => c.id === parent.id)
    const nextSibling = parentIdx >= 0 && parentIdx < gpChildren.length - 1
      ? gpChildren[parentIdx + 1]
      : null
    const position = positionBetween(parent.position, nextSibling?.position ?? null)

    await api.blocks.move(blockId, { parent_id: grandparentId, position })
  }

  await reloadCurrentTree()
  await reloadAffectedParents([...oldParentIds, ...newParentIds])
}
