/**
 * Content transformation helpers — pure functions, no framework dependency.
 * Handles client-side link rendering and parsing.
 */
/**
 * Represents a parsed link in content
 */
export interface ParsedLink {
  /** Full match including brackets: [[path::to::block]] */
  raw: string
  /** Path without brackets: path::to::block */
  path: string
  /** Start index in content */
  start: number
  /** End index in content */
  end: number
  /** Whether link target exists */
  resolved: boolean
}

/**
 * Regex for matching wiki links: [[...]]
 */
const LINK_REGEX = /\[\[([^\]]+)\]\]/g

/**
 * Parse wiki links from content
 */
export function parseLinks(content: string): ParsedLink[] {
  LINK_REGEX.lastIndex = 0
  const links: ParsedLink[] = []
  let match: RegExpExecArray | null

  while ((match = LINK_REGEX.exec(content)) !== null) {
    links.push({
      raw: match[0],
      path: match[1],
      start: match.index,
      end: match.index + match[0].length,
      resolved: true,
    })
  }

  return links
}

/**
 * A segment of content — either plain text or a wiki link
 */
export interface ContentSegment {
  type: 'text' | 'link'
  value: string
  link?: ParsedLink
}

/**
 * Split content into segments for safe rendering.
 * Alternates between text and link segments.
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
      type: 'link',
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
