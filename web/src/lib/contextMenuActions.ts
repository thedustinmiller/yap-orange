/**
 * Context menu action registry for outliner nodes.
 *
 * Each action is a plain object describing label, icon, condition, and handler.
 * To add/remove/reorder actions, just edit the `actions` array below.
 */

import type { TreeNode } from './blockTree.svelte';

export interface MenuAction {
  /** Unique key for this action */
  id: string
  /** Display label */
  label: string
  /** Optional leading icon (emoji or text) */
  icon?: string
  /** Return false to hide this action for a given node */
  visible?: (node: TreeNode) => boolean
  /** The handler — receives the node and a cleanup callback */
  handler: (node: TreeNode, ctx: MenuContext) => void | Promise<void>
  /** Optional divider line after this item */
  dividerAfter?: boolean
}

export interface MenuContext {
  /** Trigger inline rename on the node */
  startRename: () => void
  /** Navigate to (center on) this node */
  navigateTo: (id: string) => void
  /** Export subtree rooted at this node */
  exportSubtree: (id: string, namespace: string) => Promise<void>
}

export const actions: MenuAction[] = [
  {
    id: 'rename',
    label: 'Edit name',
    icon: '✏',
    handler: (_node, ctx) => {
      ctx.startRename();
    },
  },
  {
    id: 'refocus',
    label: 'Focus here',
    icon: '⊙',
    handler: (node, ctx) => {
      ctx.navigateTo(node.id);
    },
  },
  {
    id: 'export',
    label: 'Export subtree',
    icon: '↓',
    handler: (node, ctx) => {
      ctx.exportSubtree(node.id, node.namespace);
    },
    dividerAfter: false,
  },
];
