/**
 * Toast notification store — module-level $state singleton.
 *
 * Shows transient messages (errors, warnings, info) in a fixed overlay.
 * Follows the same pattern as schemaStore.svelte.ts.
 */

export interface Toast {
  id: number
  message: string
  type: 'error' | 'warning' | 'info'
}

let toasts = $state<Toast[]>([])
let nextId = 0

export function addToast(
  message: string,
  type: 'error' | 'warning' | 'info' = 'error',
  duration = 5000,
): void {
  const id = nextId++
  toasts.push({ id, message, type })
  if (duration > 0) setTimeout(() => removeToast(id), duration)
}

export function removeToast(id: number): void {
  const idx = toasts.findIndex((t) => t.id === id)
  if (idx >= 0) toasts.splice(idx, 1)
}

export function getToasts(): Toast[] {
  return toasts
}
