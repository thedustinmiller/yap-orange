/**
 * Type view registry — maps content_type strings to view definitions.
 *
 * Each ViewDefinition specifies a lazy-loaded Svelte component and
 * display metadata (icon, label). When a block's content_type has a
 * registered definition, OutlinerNode renders the custom component
 * instead of the normal BlockEditor/ContentRenderer pair.
 *
 * The registry is data-driven and extensible — new content types can
 * be added by inserting entries into VIEW_DEFINITIONS.
 */

import type { Component } from 'svelte'

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface ViewDefinition {
  /** Lazy component loader — called once, result is cached */
  load: () => Promise<{ default: Component<any> }>
  /** Icon string displayed in the outliner node row */
  icon: string
  /** Human-readable label (used in tooltips, headers, etc.) */
  label: string
}

// ---------------------------------------------------------------------------
// Registry data
// ---------------------------------------------------------------------------

const VIEW_DEFINITIONS: Record<string, ViewDefinition> = {
  setting: {
    load: () => import('./SettingsView.svelte'),
    icon: '⚙',
    label: 'Settings',
  },
  schema: {
    load: () => import('./SchemaView.svelte'),
    icon: '⬡',
    label: 'Schema',
  },
  type_registry: {
    load: () => import('./TypeRegistryView.svelte'),
    icon: '⬡',
    label: 'Type Registry',
  },
  todo: {
    load: () => import('./TodoView.svelte'),
    icon: '☑',
    label: 'Todo',
  },
  image: {
    load: () => import('./ImageView.svelte'),
    icon: '🖼',
    label: 'Image',
  },
  pdf: {
    load: () => import('./PdfView.svelte'),
    icon: '📄',
    label: 'PDF',
  },
  file: {
    load: () => import('./FileView.svelte'),
    icon: '📎',
    label: 'File',
  },
}

// Cache: content_type → loaded Component
const componentCache = new Map<string, Component<any>>()

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Pre-load all registered view components. Call once at app startup
 * or lazily on first use.
 */
export async function preloadViews(): Promise<void> {
  const entries = Object.entries(VIEW_DEFINITIONS)
  const modules = await Promise.all(entries.map(([, def]) => def.load()))
  for (let i = 0; i < entries.length; i++) {
    componentCache.set(entries[i][0], modules[i].default)
  }
}

/**
 * Check if a content_type has a registered custom view.
 */
export function hasCustomView(contentType: string | null | undefined): boolean {
  if (!contentType) return false
  return contentType in VIEW_DEFINITIONS
}

/**
 * Get the loaded component for a content_type synchronously.
 * Returns undefined if not yet loaded — call preloadViews() first.
 */
export function getCustomView(contentType: string): Component<any> | undefined {
  return componentCache.get(contentType)
}

/**
 * Get the ViewDefinition for a content_type (icon, label, etc.).
 * Returns undefined if the content_type has no registered view.
 */
export function getViewDefinition(contentType: string | null | undefined): ViewDefinition | undefined {
  if (!contentType) return undefined
  return VIEW_DEFINITIONS[contentType]
}

/**
 * Get the icon for a content_type, or undefined if none registered.
 */
export function getViewIcon(contentType: string | null | undefined): string | undefined {
  return getViewDefinition(contentType)?.icon
}
