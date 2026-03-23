/**
 * CM6 autocomplete source for wiki links.
 *
 * Triggers when the user types `[[` and fetches matching blocks
 * from the API with debouncing. On accept, completes the `[[path]]` syntax.
 */
import {
  autocompletion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete'
import type { Extension } from '@codemirror/state'
import { api } from '../api'
import { getSchemas, loadSchemas } from '../schemaStore.svelte'

// Debounce state shared across completion calls
let debounceTimer: ReturnType<typeof setTimeout> | null = null
let lastQuery = ''
let cachedResults: CompletionResult | null = null
let cacheTimestamp = 0
const CACHE_TTL_MS = 5000

/**
 * Clear the wiki-link completion cache. Call after mutations that
 * change the block tree (save, create, delete).
 */
export function clearCompletionCache(): void {
  cachedResults = null
  lastQuery = ''
  cacheTimestamp = 0
}

async function wikiLinkSource(
  context: CompletionContext,
): Promise<CompletionResult | null> {
  const match = context.matchBefore(/\[\[([^\]]*)/)
  if (!match) return null

  // The query is everything after `[[`
  const fullMatch = match.text
  const query = fullMatch.slice(2) // strip `[[`
  const from = match.from + 2 // completion replaces after `[[`

  // Expire stale cache
  if (Date.now() - cacheTimestamp > CACHE_TTL_MS) {
    cachedResults = null
  }

  // If query hasn't changed and we have cached results, reuse them
  if (query === lastQuery && cachedResults) {
    return { ...cachedResults, from }
  }

  // Cancel any pending debounce
  if (debounceTimer) clearTimeout(debounceTimer)

  // For empty query or explicit completion, fetch immediately
  // Otherwise debounce
  const results = await new Promise<CompletionResult | null>((resolve) => {
    const doFetch = async () => {
      try {
        const blocks = await api.blocks.list(
          query.length > 0 ? { search: query } : undefined,
        )
        const options = blocks.slice(0, 8).map((block) => ({
          label: block.namespace,
          apply: `${block.namespace}]]`,
          type: 'text' as const,
        }))

        if (options.length === 0) {
          resolve(null)
          return
        }

        const result: CompletionResult = {
          from,
          options,
          validFor: /^[^\]]*$/,
        }

        lastQuery = query
        cachedResults = result
        cacheTimestamp = Date.now()
        resolve(result)
      } catch {
        console.warn('Wiki-link completion fetch failed')
        resolve(null)
      }
    }

    if (query.length === 0 || context.explicit) {
      doFetch()
    } else {
      debounceTimer = setTimeout(doFetch, 150)
    }
  })

  return results
}

/**
 * Autocomplete source for @type{...} entry creation command.
 * Triggers when user types `@` — offers schema names with field skeleton JSON.
 */
async function typeCommandSource(
  context: CompletionContext,
): Promise<CompletionResult | null> {
  const match = context.matchBefore(/^@(\w*)/)
  if (!match) return null

  const query = match.text.slice(1) // strip `@`
  const from = match.from + 1       // completion replaces after `@`

  await loadSchemas()
  const allSchemas = getSchemas()

  const filtered = allSchemas.filter((s) =>
    query.length === 0 || s.name.toLowerCase().includes(query.toLowerCase()),
  )

  if (filtered.length === 0) return null

  const options = filtered.slice(0, 8).map((schema) => {
    const fieldSkeleton: Record<string, string> = {}
    if (Array.isArray(schema.fields)) {
      for (const field of schema.fields) {
        fieldSkeleton[field.name] = ''
      }
    }
    const skeletonJson = Object.entries(fieldSkeleton)
      .map(([k, v]) => `"${k}":"${v}"`)
      .join(',')

    return {
      label: `@${schema.name}`,
      detail: `${Array.isArray(schema.fields) ? schema.fields.length : 0} fields`,
      apply: `${schema.name}{${skeletonJson}}`,
      type: 'type' as const,
    }
  })

  return {
    from,
    options,
    validFor: /^\w*$/,
  }
}

/**
 * Autocomplete source for ![[embed]] syntax.
 * Triggers when user types `![[` — same block search as wiki links.
 */
async function embedLinkSource(
  context: CompletionContext,
): Promise<CompletionResult | null> {
  const match = context.matchBefore(/!\[\[([^\]]*)/)
  if (!match) return null

  const query = match.text.slice(3) // strip `![[`
  const from = match.from + 3 // completion replaces after `![[`

  try {
    const blocks = await api.blocks.list(
      query.length > 0 ? { search: query } : undefined,
    )
    const options = blocks.slice(0, 8).map((block) => ({
      label: block.namespace,
      apply: `${block.namespace}]]`,
      type: 'text' as const,
    }))

    if (options.length === 0) return null

    return {
      from,
      options,
      validFor: /^[^\]]*$/,
    }
  } catch {
    console.warn('Embed completion fetch failed')
    return null
  }
}

/**
 * Returns a CM6 autocompletion extension configured for wiki link,
 * embed link, and @type{...} entry creation completion.
 */
export function wikiLinkCompletion(): Extension {
  return autocompletion({
    override: [embedLinkSource, wikiLinkSource, typeCommandSource],
    icons: false,
  })
}
