/**
 * File drop dispatcher — handles external file drops on the outliner.
 *
 * Dispatches by file type:
 * - .json with yap-tree format → import tree at drop position
 * - images → create image block
 * - PDFs → create pdf block
 * - everything else → create generic file block
 */

import { api } from './api'
import { uploadAndCreateBlock } from './fileUpload'
import type { TreeNode } from './blockTree.svelte'

export interface FileDropResult {
  type: 'media' | 'import'
  blockId?: string
  importResult?: { created: number; merged: number; skipped: number }
}

/**
 * Handle files dropped onto the outliner.
 *
 * @param files - FileList from the drop event
 * @param zone - Drop zone relative to target node
 * @param targetNode - The node the files were dropped on
 * @param computePosition - Function to compute fractional index for above/below drops
 * @returns Array of results describing what was created/imported
 */
export async function handleFileDrop(
  files: FileList,
  zone: 'above' | 'below' | 'inside',
  targetNode: TreeNode,
  computePosition: (zone: 'above' | 'below') => string,
): Promise<FileDropResult[]> {
  const parentId = zone === 'inside' ? targetNode.id : (targetNode.parent_id ?? null)
  const results: FileDropResult[] = []

  for (const file of Array.from(files)) {
    // Check if it's a yap export JSON
    if (file.name.endsWith('.json') || file.type === 'application/json') {
      const imported = await tryYapImport(file, parentId)
      if (imported) {
        results.push({ type: 'import', importResult: imported })
        continue
      }
      // Not a yap export — fall through to file attachment
    }

    // Regular file — upload and create media block
    const position = zone === 'inside' ? undefined : computePosition(zone)
    const block = await uploadAndCreateBlock(file, parentId, position)
    results.push({ type: 'media', blockId: block.block_id })
  }

  return results
}

/**
 * Try to parse a JSON file as a yap export and import it.
 * Returns null if the file is not a valid yap export.
 */
async function tryYapImport(
  file: File,
  parentId: string | null,
): Promise<{ created: number; merged: number; skipped: number } | null> {
  try {
    const text = await file.text()
    const data = JSON.parse(text)
    if (!data.format?.startsWith('yap-tree-')) return null

    // Valid yap export — import it
    if (parentId) {
      return await api.importExport.import(parentId, data, 'merge')
    } else {
      return await api.importExport.importAtRoot(data, 'merge')
    }
  } catch {
    return null
  }
}
