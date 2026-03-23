/**
 * Content transformation helpers — pure functions, no framework dependency.
 * Handles client-side link rendering and parsing.
 */
/**
 * Represents a parsed link in content
 */
export interface ParsedLink {
  /** Full match including brackets: [[path::to::block]] or ![[path::to::block]] */
  raw: string
  /** Path without brackets: path::to::block */
  path: string
  /** Start index in content */
  start: number
  /** End index in content */
  end: number
  /** Whether link target exists */
  resolved: boolean
  /** Whether this is an embed (![[...]]) rather than a regular link ([[...]]) */
  isEmbed: boolean
}

/**
 * Regex for matching wiki links and embeds: [[...]] and ![[...]]
 * Group 1: optional ! prefix
 * Group 2: link path
 */
const LINK_REGEX = /(!?)\[\[([^\]]+)\]\]/g

/**
 * Parse wiki links and embeds from content
 */
export function parseLinks(content: string): ParsedLink[] {
  LINK_REGEX.lastIndex = 0
  const links: ParsedLink[] = []
  let match: RegExpExecArray | null

  while ((match = LINK_REGEX.exec(content)) !== null) {
    links.push({
      raw: match[0],
      path: match[2],
      start: match.index,
      end: match.index + match[0].length,
      resolved: true,
      isEmbed: match[1] === '!',
    })
  }

  return links
}

/**
 * A segment of content — either plain text, a wiki link, or an embed
 */
export interface ContentSegment {
  type: 'text' | 'link' | 'embed'
  value: string
  link?: ParsedLink
}

/**
 * Split content into segments for safe rendering.
 * Alternates between text, link, and embed segments.
 */
export function segmentContent(content: string): ContentSegment[] {
  const links = parseLinks(content)
  const segments: ContentSegment[] = []

  let lastEnd = 0

  for (const link of links) {
    if (link.start > lastEnd) {
      segments.push({
        type: 'text',
        value: content.slice(lastEnd, link.start),
      })
    }

    segments.push({
      type: link.isEmbed ? 'embed' : 'link',
      value: link.path,
      link,
    })

    lastEnd = link.end
  }

  if (lastEnd < content.length) {
    segments.push({
      type: 'text',
      value: content.slice(lastEnd),
    })
  }

  return segments
}
