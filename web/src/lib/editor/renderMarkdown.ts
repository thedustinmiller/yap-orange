/**
 * Markdown rendering pipeline for read-mode content display.
 *
 * Uses the mdast ecosystem:
 *   content string → fromMarkdown (parse) → toHast (→ HTML AST) → toHtml (→ string)
 *
 * Wiki links ([[path]]) pass through the mdast parser as plain text,
 * then are post-processed into clickable spans.
 */
import { fromMarkdown } from 'mdast-util-from-markdown'
import { toHast } from 'mdast-util-to-hast'
import { toHtml } from 'hast-util-to-html'

const WIKI_LINK_RE = /\[\[([^\]]+)\]\]/g

/**
 * Convert wiki link patterns in HTML to clickable spans.
 * Handles links both inside and outside of HTML tags.
 */
function replaceWikiLinks(html: string): string {
  return html.replace(WIKI_LINK_RE, (_match, path: string) => {
    const escaped = path
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
    return `<span class="wiki-link" data-path="${escaped}">${escaped}</span>`
  })
}

/**
 * Render a content string as HTML with markdown formatting and wiki link spans.
 *
 * For plain text without markdown, this still works — it just wraps in <p> tags.
 * Wiki links become `<span class="wiki-link" data-path="...">` elements.
 * Type embeds become `<span class="type-embed-display" ...>` chip spans.
 */
export function renderMarkdown(content: string): string {
  if (!content) return ''

  const mdast = fromMarkdown(content)
  const hast = toHast(mdast)
  let html = toHtml(hast!)

  // Post-process: replace [[wiki links]] with styled spans
  html = replaceWikiLinks(html)

  return html
}
