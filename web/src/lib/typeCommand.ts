/**
 * Client-side parser for the @type{...} entry creation command.
 *
 * When the entire content of a block is `@typeName{"field":"value",...}`,
 * this parser extracts the type name and field values. The caller then
 * sets content_type and properties on the block, clearing the text content.
 *
 * This is a one-shot creation shorthand — the server never sees this syntax.
 */

export interface TypeCommand {
  typeName: string
  values: Record<string, unknown>
}

// Matches entire content as @typeName{...json...}
const TYPE_CMD_RE = /^@(\w+)\{([\s\S]*)\}$/

/**
 * Parse a block's content as a type creation command.
 * Returns null if the content is not a valid @type{...} command.
 */
export function parseTypeCommand(content: string): TypeCommand | null {
  const trimmed = content.trim()
  const match = TYPE_CMD_RE.exec(trimmed)
  if (!match) return null
  try {
    const values = JSON.parse(`{${match[2]}}`)
    if (typeof values !== 'object' || values === null || Array.isArray(values)) return null
    return { typeName: match[1], values }
  } catch {
    return null
  }
}
