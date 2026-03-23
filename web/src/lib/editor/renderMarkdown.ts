/**
 * Markdown rendering pipeline for read-mode content display.
 *
 * Uses the mdast ecosystem:
 *   content string → fromMarkdown (parse) → toHast (→ HTML AST) → toHtml (→ string)
 *
 * Wiki links ([[path]]) pass through the mdast parser as plain text,
 * then are post-processed into clickable spans.
 * Embed links (![[path]]) become placeholder spans for async mounting.
 */
import { fromMarkdown } from 'mdast-util-from-markdown'
import { toHast } from 'mdast-util-to-hast'
import { toHtml } from 'hast-util-to-html'

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/**
 * Regex that matches both embeds (![[...]]) and regular links ([[...]]).
 * Embeds are matched first due to the optional ! prefix.
 * Group 1: optional ! prefix
 * Group 2: link path
 */
const LINK_RE = /(!?)\[\[([^\]]+)\]\]/g

/**
 * Replace wiki link and embed patterns in HTML with styled elements.
 * - Regular links → clickable `<span class="wiki-link">` spans
 * - Embeds → `<span class="block-embed">` placeholder spans (mounted by ContentRenderer)
 */
function replaceLinks(html: string): string {
  return html.replace(LINK_RE, (_match, bang: string, path: string) => {
    const escaped = escapeHtml(path)
    if (bang === '!') {
      return `<span class="block-embed" data-embed-path="${escaped}"></span>`
    }
    return `<span class="wiki-link" data-path="${escaped}">${escaped}</span>`
  })
}

/**
 * Render a content string as HTML with markdown formatting and wiki link spans.
 *
 * For plain text without markdown, this still works — it just wraps in <p> tags.
 * Wiki links become `<span class="wiki-link" data-path="...">` elements.
 * Embed links become `<span class="block-embed" data-embed-path="...">` placeholders.
 */
export function renderMarkdown(content: string): string {
  if (!content) return ''

  const mdast = fromMarkdown(content)
  const hast = toHast(mdast)
  let html = toHtml(hast!)

  // Post-process: replace [[wiki links]] and ![[embeds]] with styled spans
  html = replaceLinks(html)

  return html
}
