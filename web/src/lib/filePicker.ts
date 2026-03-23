/**
 * Reusable file picker helpers.
 *
 * Uses the File System Access API when available (Chrome/Edge 86+)
 * with a fallback to a hidden <input type="file"> element.
 */

export interface FilePickerOptions {
  /** File type filter, e.g. '.json,.zip' or 'image/*' */
  accept?: string
  /** Allow selecting multiple files */
  multiple?: boolean
}

/**
 * Open a file picker dialog and return the selected files.
 * Returns an empty array if the user cancels.
 */
export async function openFilePicker(options: FilePickerOptions = {}): Promise<File[]> {
  // Try File System Access API first (Chrome/Edge 86+)
  if ('showOpenFilePicker' in window) {
    try {
      const types = options.accept
        ? [{ accept: parseAcceptTypes(options.accept) }]
        : undefined
      const handles = await (window as any).showOpenFilePicker({
        multiple: options.multiple ?? false,
        types,
      })
      const files: File[] = []
      for (const handle of handles) {
        files.push(await handle.getFile())
      }
      return files
    } catch (e: any) {
      // User cancelled (AbortError) — return empty
      if (e?.name === 'AbortError') return []
      // Fall through to input fallback on other errors
    }
  }

  // Fallback: hidden <input type="file">
  return new Promise<File[]>((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    if (options.accept) input.accept = options.accept
    if (options.multiple) input.multiple = true

    input.onchange = () => {
      const files = input.files ? Array.from(input.files) : []
      resolve(files)
    }

    // Handle cancel — no native event, but if focus returns without change, resolve empty
    const onFocus = () => {
      setTimeout(() => {
        if (!input.files?.length) resolve([])
        window.removeEventListener('focus', onFocus)
      }, 300)
    }
    window.addEventListener('focus', onFocus)

    input.click()
  })
}

/**
 * Convert a comma-separated accept string like ".json,.zip" or "image/*"
 * into the format expected by showOpenFilePicker.
 */
function parseAcceptTypes(accept: string): Record<string, string[]> {
  const result: Record<string, string[]> = {}
  for (const part of accept.split(',').map((s) => s.trim())) {
    if (part.startsWith('.')) {
      // Extension filter
      const mime = extensionToMime(part) || 'application/octet-stream'
      if (!result[mime]) result[mime] = []
      result[mime].push(part)
    } else if (part.includes('/')) {
      // MIME type filter
      if (!result[part]) result[part] = []
    }
  }
  return result
}

function extensionToMime(ext: string): string | null {
  const map: Record<string, string> = {
    '.json': 'application/json',
    '.zip': 'application/zip',
    '.png': 'image/png',
    '.jpg': 'image/jpeg',
    '.jpeg': 'image/jpeg',
    '.gif': 'image/gif',
    '.svg': 'image/svg+xml',
    '.webp': 'image/webp',
    '.pdf': 'application/pdf',
  }
  return map[ext.toLowerCase()] ?? null
}
