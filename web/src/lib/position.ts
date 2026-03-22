/**
 * Fractional index position utilities.
 *
 * Mirrors the Rust `fractional_index` crate (v2) exactly.
 *
 * KEY INVARIANT: every valid position is a hex string whose last two
 * characters are "80" — the terminator byte (0x80 = 128). The crate's
 * `from_string` rejects any string that doesn't end with it.
 *
 *   "80"      → [0x80]           (default / only-child)
 *   "7f80"    → [0x7F, 0x80]    (before default)
 *   "8180"    → [0x81, 0x80]    (after default)
 *   "817f80"  → [0x81, 0x7F, 0x80]
 *
 * The helper functions below (afterBytes, beforeBytes, betweenBytes)
 * operate on the FULL byte slice including the terminator, matching Rust's
 * internal `new_after` / `new_before` / `new_between` functions. Their
 * return value is the "unterminated" prefix; the caller appends T.
 */

const T = 0x80 // terminator byte

function hexToBytes(hex: string): number[] {
  const bytes: number[] = []
  for (let i = 0; i < hex.length; i += 2) {
    bytes.push(parseInt(hex.substring(i, i + 2), 16))
  }
  return bytes
}

function bytesToHex(bytes: number[]): string {
  return bytes.map(b => b.toString(16).padStart(2, '0')).join('')
}

/** Mirror of Rust's internal `new_after`. Takes full bytes (incl. terminator). */
function afterBytes(bytes: number[]): number[] {
  for (let i = 0; i < bytes.length; i++) {
    if (bytes[i] < T) {
      // Byte less than terminator: truncate here (everything after is "suffix")
      return bytes.slice(0, i)
    }
    if (bytes[i] < 0xff) {
      // Byte we can increment
      const result = bytes.slice(0, i + 1)
      result[i] += 1
      return result
    }
  }
  throw new Error('fractional_index: cannot find position after (all 0xFF bytes)')
}

/** Mirror of Rust's internal `new_before`. Takes full bytes (incl. terminator). */
function beforeBytes(bytes: number[]): number[] {
  for (let i = 0; i < bytes.length; i++) {
    if (bytes[i] > T) {
      // Byte greater than terminator: truncate here
      return bytes.slice(0, i)
    }
    if (bytes[i] > 0) {
      // Byte we can decrement
      const result = bytes.slice(0, i + 1)
      result[i] -= 1
      return result
    }
  }
  throw new Error('fractional_index: cannot find position before (all zero bytes)')
}

/**
 * Mirror of Rust's `new_between`. Takes full byte arrays (incl. terminator).
 * Returns the unterminated prefix bytes, or null if impossible (equal or reversed).
 */
function betweenBytes(left: number[], right: number[]): number[] | null {
  const shorterLen = Math.min(left.length, right.length) - 1

  for (let i = 0; i < shorterLen; i++) {
    if (left[i] < right[i] - 1) {
      // More than one apart — take midpoint at this byte
      const bytes = left.slice(0, i + 1)
      bytes[i] += Math.floor((right[i] - left[i]) / 2)
      return bytes
    }
    if (left[i] === right[i] - 1) {
      // Adjacent — descend into the left suffix
      const prefix = left.slice(0, i + 1)
      const suffix = afterBytes(left.slice(i + 1))
      return [...prefix, ...suffix]
    }
    if (left[i] > right[i]) {
      return null // right < left — invalid
    }
  }

  if (left.length < right.length) {
    // left is a prefix of right (up to shorter_len)
    const prefix = right.slice(0, shorterLen + 1)
    if ((prefix[prefix.length - 1] ?? 0) < T) return null // right side is less
    const newSuffix = beforeBytes(right.slice(shorterLen + 1))
    return [...prefix, ...newSuffix]
  }

  if (left.length > right.length) {
    // right is a prefix of left (up to shorter_len)
    const prefix = left.slice(0, shorterLen + 1)
    if ((prefix[prefix.length - 1] ?? 0) >= T) return null // left side is greater
    const newSuffix = afterBytes(left.slice(shorterLen + 1))
    return [...prefix, ...newSuffix]
  }

  return null // equal
}

/**
 * Generate a position string between two existing positions.
 *
 * All inputs/outputs are valid fractional index hex strings (must end in "80").
 *
 * - `(null, null)` → default position `"80"`
 * - `(before, null)` → position after `before`
 * - `(null, after)` → position before `after`
 * - `(before, after)` → position between the two
 */
export function positionBetween(before: string | null, after: string | null): string {
  if (!before && !after) return bytesToHex([T])

  const a = before ? hexToBytes(before) : null
  const b = after ? hexToBytes(after) : null

  if (!a && b) {
    return bytesToHex([...beforeBytes(b), T])
  }
  if (a && !b) {
    return bytesToHex([...afterBytes(a), T])
  }
  if (a && b) {
    const mid = betweenBytes(a, b)
    if (mid === null) {
      // Fallback: position after `before` (shouldn't normally happen)
      return bytesToHex([...afterBytes(a), T])
    }
    return bytesToHex([...mid, T])
  }

  return bytesToHex([T])
}
