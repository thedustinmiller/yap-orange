/**
 * File upload helper — uploads a file to the blob store and creates
 * a media block (image, pdf, or generic file) in the hierarchy.
 */

import { api } from './api'

/**
 * Determine the block content_type from a file's MIME type.
 */
export function mediaContentType(mimeType: string): string {
  if (mimeType.startsWith('image/')) return 'image'
  if (mimeType === 'application/pdf') return 'pdf'
  return 'file'
}

/**
 * Upload a file and create a media block under the given parent.
 *
 * @param file - The File object to upload
 * @param parentId - Parent block ID, or null for root
 * @param position - Optional fractional index position
 * @returns The created block response
 */
export async function uploadAndCreateBlock(
  file: File,
  parentId: string | null,
  position?: string,
): Promise<{ block_id: string; lineage_id: string }> {
  // 1. Upload file bytes to content-addressed store
  const { hash, size } = await api.files.upload(file)

  // 2. Determine content_type from MIME
  const contentType = mediaContentType(file.type)

  // 3. Strip file extension for block name
  const name = file.name.replace(/\.[^.]+$/, '') || file.name

  // 4. Create the block
  return api.blocks.create({
    namespace: '',
    name,
    content: '',
    content_type: contentType,
    properties: {
      file_hash: hash,
      filename: file.name,
      mime: file.type || 'application/octet-stream',
      size,
    },
    parent_id: parentId ?? undefined,
    position,
  })
}
