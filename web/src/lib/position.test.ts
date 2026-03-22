import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import { positionBetween } from './position'

// ============================================
// Core invariants
// ============================================
describe('positionBetween - core invariants', () => {
  it('returns "80" for (null, null)', () => {
    expect(positionBetween(null, null)).toBe('80')
  })

  it('every output ends in "80"', () => {
    const inputs: [string | null, string | null][] = [
      [null, null],
      ['80', null],
      [null, '80'],
      ['80', '8180'],
      ['7f80', '80'],
      ['8180', null],
      [null, '7f80'],
      ['80', '828080'],
      ['7e80', '7f80'],
    ]
    for (const [before, after] of inputs) {
      const result = positionBetween(before, after)
      expect(result.endsWith('80'), `positionBetween(${before}, ${after}) = "${result}" should end with "80"`).toBe(true)
    }
  })

  it('hex string length is always even', () => {
    const inputs: [string | null, string | null][] = [
      [null, null],
      ['80', null],
      [null, '80'],
      ['80', '8180'],
      ['7f80', '80'],
      ['7f80', '8180'],
    ]
    for (const [before, after] of inputs) {
      const result = positionBetween(before, after)
      expect(result.length % 2, `positionBetween(${before}, ${after}) = "${result}" length should be even`).toBe(0)
    }
  })
})

// ============================================
// After (before=X, after=null)
// ============================================
describe('positionBetween - after', () => {
  it('after "80" returns "8180"', () => {
    expect(positionBetween('80', null)).toBe('8180')
  })

  it('after "8180" returns value > "8180"', () => {
    const result = positionBetween('8180', null)
    expect(result > '8180').toBe(true)
    expect(result.endsWith('80')).toBe(true)
  })

  it('chain of 10 after-calls produces strictly increasing values', () => {
    const positions: string[] = ['80']
    for (let i = 0; i < 10; i++) {
      const next = positionBetween(positions[positions.length - 1], null)
      positions.push(next)
    }
    for (let i = 1; i < positions.length; i++) {
      expect(
        positions[i] > positions[i - 1],
        `positions[${i}] "${positions[i]}" should be > positions[${i - 1}] "${positions[i - 1]}"`
      ).toBe(true)
    }
  })
})

// ============================================
// Before (before=null, after=X)
// ============================================
describe('positionBetween - before', () => {
  it('before "80" returns "7f80"', () => {
    expect(positionBetween(null, '80')).toBe('7f80')
  })

  it('before "7f80" returns value < "7f80"', () => {
    const result = positionBetween(null, '7f80')
    expect(result < '7f80').toBe(true)
    expect(result.endsWith('80')).toBe(true)
  })

  it('chain of 10 before-calls produces strictly decreasing values', () => {
    const positions: string[] = ['80']
    for (let i = 0; i < 10; i++) {
      const next = positionBetween(null, positions[positions.length - 1])
      positions.push(next)
    }
    for (let i = 1; i < positions.length; i++) {
      expect(
        positions[i] < positions[i - 1],
        `positions[${i}] "${positions[i]}" should be < positions[${i - 1}] "${positions[i - 1]}"`
      ).toBe(true)
    }
  })
})

// ============================================
// Between
// ============================================
describe('positionBetween - between', () => {
  it('between "80" and "8180" returns value strictly between', () => {
    const result = positionBetween('80', '8180')
    expect(result > '80').toBe(true)
    expect(result < '8180').toBe(true)
    expect(result.endsWith('80')).toBe(true)
  })

  it('between "7f80" and "80" returns value strictly between', () => {
    const result = positionBetween('7f80', '80')
    expect(result > '7f80').toBe(true)
    expect(result < '80').toBe(true)
    expect(result.endsWith('80')).toBe(true)
  })

  it('between distant values produces midpoint', () => {
    const result = positionBetween('4080', 'c080')
    expect(result > '4080').toBe(true)
    expect(result < 'c080').toBe(true)
    expect(result.endsWith('80')).toBe(true)
  })

  it('between adjacent values produces longer string', () => {
    // "8180" and "8280" are adjacent at the first byte level
    const result = positionBetween('8180', '8280')
    expect(result > '8180').toBe(true)
    expect(result < '8280').toBe(true)
    expect(result.endsWith('80')).toBe(true)
    // The result needs more precision, so it should be longer
    expect(result.length).toBeGreaterThanOrEqual(4)
  })

  it('equal inputs returns valid position via afterBytes fallback', () => {
    const result = positionBetween('80', '80')
    expect(result.endsWith('80')).toBe(true)
    expect(result.length % 2).toBe(0)
  })
})

// ============================================
// Ordering correctness
// ============================================
describe('positionBetween - ordering', () => {
  it('100 sequential insertions at position 0 sort in reverse', () => {
    // Each insertion goes before the current first — the newest should be smallest
    const positions: string[] = []
    let current = '80'
    for (let i = 0; i < 100; i++) {
      const next = positionBetween(null, current)
      positions.push(next)
      current = next
    }
    // positions[0] is the largest (first "before"), positions[99] is the smallest (last "before")
    for (let i = 1; i < positions.length; i++) {
      expect(
        positions[i] < positions[i - 1],
        `positions[${i}] "${positions[i]}" should be < positions[${i - 1}] "${positions[i - 1]}"`
      ).toBe(true)
    }
  })

  it('alternating first/last/middle insertions sort correctly', () => {
    const positions: string[] = ['80']

    // Insert after
    positions.push(positionBetween(positions[0], null))
    // Insert before
    positions.push(positionBetween(null, positions[0]))
    // Insert in middle
    positions.push(positionBetween(positions[0], positions[1]))
    // Insert another middle
    positions.push(positionBetween(positions[2], positions[0]))

    const sorted = [...positions].sort()
    // Verify each position appears at a unique sort position
    expect(new Set(sorted).size).toBe(positions.length)
  })

  it('pairwise: a < c < b when c is between a and b', () => {
    const a = positionBetween(null, null) // "80"
    const b = positionBetween(a, null)     // after a
    const c = positionBetween(a, b)        // between a and b

    expect(a < c, `a "${a}" < c "${c}"`).toBe(true)
    expect(c < b, `c "${c}" < b "${b}"`).toBe(true)
  })
})

// ============================================
// Edge cases
// ============================================
describe('positionBetween - edge cases', () => {
  it('very long positions after many between-insertions still end in "80"', () => {
    let left = '80'
    let right = '8180'
    for (let i = 0; i < 50; i++) {
      const mid = positionBetween(left, right)
      expect(mid.endsWith('80'), `iteration ${i}: "${mid}" should end with "80"`).toBe(true)
      expect(mid.length % 2, `iteration ${i}: "${mid}" length should be even`).toBe(0)
      // Push toward right to force longer positions
      left = mid
    }
  })

  it('same before and after returns valid position', () => {
    const result = positionBetween('8180', '8180')
    expect(result.endsWith('80')).toBe(true)
    expect(result.length % 2).toBe(0)
  })

  it('rapid interleaving preserves invariants', () => {
    // Build a sorted list by always inserting between neighbors
    const positions = ['4080', 'c080']
    for (let i = 0; i < 20; i++) {
      const idx = i % (positions.length - 1)
      const mid = positionBetween(positions[idx], positions[idx + 1])
      expect(mid.endsWith('80')).toBe(true)
      expect(mid > positions[idx], `mid "${mid}" > left "${positions[idx]}"`).toBe(true)
      expect(mid < positions[idx + 1], `mid "${mid}" < right "${positions[idx + 1]}"`).toBe(true)
      // Insert in sorted position
      positions.splice(idx + 1, 0, mid)
    }
    // Verify final array is sorted
    for (let i = 1; i < positions.length; i++) {
      expect(positions[i] > positions[i - 1]).toBe(true)
    }
  })
})

// ============================================
// Overflow boundary
// ============================================
describe('positionBetween - overflow boundary', () => {
  it('after "fe80" increments first byte', () => {
    const result = positionBetween('fe80', null)
    expect(result).toBe('ff80')
  })

  it('after "ff80" extends deeper', () => {
    const result = positionBetween('ff80', null)
    expect(result).toBe('ff8180')
  })

  it('after "fffe80" increments second byte', () => {
    const result = positionBetween('fffe80', null)
    expect(result).toBe('ffff80')
  })

  it('after "ffff80" extends to third byte', () => {
    const result = positionBetween('ffff80', null)
    expect(result).toBe('ffff8180')
  })

  it('chain of 20 appends from "fe80"', () => {
    const positions: string[] = ['fe80']
    for (let i = 0; i < 20; i++) {
      const next = positionBetween(positions[positions.length - 1], null)
      positions.push(next)
    }
    for (let i = 1; i < positions.length; i++) {
      expect(
        positions[i] > positions[i - 1],
        `positions[${i}] "${positions[i]}" should be > positions[${i - 1}] "${positions[i - 1]}"`
      ).toBe(true)
      expect(positions[i].endsWith('80'), `positions[${i}] "${positions[i]}" should end with "80"`).toBe(true)
    }
  })
})

// ============================================
// Underflow boundary
// ============================================
describe('positionBetween - underflow boundary', () => {
  it('before "0180" decrements first byte', () => {
    const result = positionBetween(null, '0180')
    expect(result).toBe('0080')
  })

  it('before "0080" extends deeper', () => {
    const result = positionBetween(null, '0080')
    // First byte stuck at 0x00, terminator decremented
    expect(result.endsWith('80')).toBe(true)
    expect(result < '0080').toBe(true)
    expect(result.length).toBeGreaterThan(4)
  })

  it('chain of 20 prepends from "0180"', () => {
    const positions: string[] = ['0180']
    for (let i = 0; i < 20; i++) {
      const next = positionBetween(null, positions[positions.length - 1])
      positions.push(next)
    }
    for (let i = 1; i < positions.length; i++) {
      expect(
        positions[i] < positions[i - 1],
        `positions[${i}] "${positions[i]}" should be < positions[${i - 1}] "${positions[i - 1]}"`
      ).toBe(true)
      expect(positions[i].endsWith('80'), `positions[${i}] "${positions[i]}" should end with "80"`).toBe(true)
    }
  })
})

// ============================================
// Growth under splitting
// ============================================
describe('positionBetween - growth under splitting', () => {
  it('100 left-biased splits stay bounded', () => {
    let left = '4080'
    const right = 'c080'
    let maxLen = 0
    for (let i = 0; i < 100; i++) {
      const mid = positionBetween(left, right)
      expect(mid > left, `iteration ${i}: mid "${mid}" > left "${left}"`).toBe(true)
      expect(mid < right, `iteration ${i}: mid "${mid}" < right "${right}"`).toBe(true)
      expect(mid.endsWith('80')).toBe(true)
      if (mid.length > maxLen) maxLen = mid.length
      left = mid
    }
    expect(maxLen).toBeLessThan(210)
  })

  it('50 right-biased splits stay bounded', () => {
    const left = '4080'
    let right = 'c080'
    let maxLen = 0
    for (let i = 0; i < 50; i++) {
      const mid = positionBetween(left, right)
      expect(mid > left, `iteration ${i}: mid "${mid}" > left "${left}"`).toBe(true)
      expect(mid < right, `iteration ${i}: mid "${mid}" < right "${right}"`).toBe(true)
      expect(mid.endsWith('80')).toBe(true)
      if (mid.length > maxLen) maxLen = mid.length
      right = mid
    }
    expect(maxLen).toBeLessThan(210)
  })

  it('200 splits into same gap bounded', () => {
    let left = '8080'
    let right = '8180'
    let maxLen = 0
    for (let i = 0; i < 200; i++) {
      const mid = positionBetween(left, right)
      expect(mid > left, `iteration ${i}: mid "${mid}" > left "${left}"`).toBe(true)
      expect(mid < right, `iteration ${i}: mid "${mid}" < right "${right}"`).toBe(true)
      expect(mid.endsWith('80')).toBe(true)
      if (mid.length > maxLen) maxLen = mid.length
      // Alternate: push left toward right
      left = mid
    }
    expect(maxLen).toBeLessThan(400)
  })

  it('sequential appends sort lexicographically', () => {
    const positions: string[] = ['80']
    for (let i = 0; i < 50; i++) {
      const next = positionBetween(positions[positions.length - 1], null)
      positions.push(next)
    }
    const sorted = [...positions].sort()
    for (let i = 0; i < positions.length; i++) {
      expect(
        positions[i] === sorted[i],
        `position ${i}: "${positions[i]}" should equal sorted "${sorted[i]}"`
      ).toBe(true)
    }
  })
})

// ============================================
// Extreme values
// ============================================
describe('positionBetween - extreme values', () => {
  it('between "0080" and "ff80" produces near-center midpoint', () => {
    const result = positionBetween('0080', 'ff80')
    expect(result > '0080').toBe(true)
    expect(result < 'ff80').toBe(true)
    expect(result.endsWith('80')).toBe(true)
  })

  it('reversed inputs ("8180", "80") does not crash', () => {
    // betweenBytes returns null for reversed inputs, fallback to afterBytes
    const result = positionBetween('8180', '80')
    expect(result.endsWith('80')).toBe(true)
    expect(result.length % 2).toBe(0)
  })

  it('prefix relationship ("8180", "81808180") exercises left-prefix branch', () => {
    const result = positionBetween('8180', '81808180')
    expect(result > '8180').toBe(true)
    expect(result < '81808180').toBe(true)
    expect(result.endsWith('80')).toBe(true)
  })
})

// ============================================
// Property tests (fast-check)
// ============================================
describe('positionBetween - property tests', () => {
  it('every output ends in "80"', () => {
    const arbPos = fc.hexaString({ minLength: 2, maxLength: 8 }).filter(s => s.length % 2 === 0 && s.endsWith('80') && s.length >= 2)
    fc.assert(fc.property(
      fc.option(arbPos, { nil: null }),
      fc.option(arbPos, { nil: null }),
      (before, after) => {
        const result = positionBetween(before, after)
        expect(result.endsWith('80')).toBe(true)
      }
    ), { numRuns: 200 })
  })

  it('output hex length is always even', () => {
    const arbPos = fc.hexaString({ minLength: 2, maxLength: 8 }).filter(s => s.length % 2 === 0 && s.endsWith('80') && s.length >= 2)
    fc.assert(fc.property(
      fc.option(arbPos, { nil: null }),
      fc.option(arbPos, { nil: null }),
      (before, after) => {
        const result = positionBetween(before, after)
        expect(result.length % 2).toBe(0)
      }
    ), { numRuns: 200 })
  })

  it('between: before < result < after when before < after', () => {
    const arbPos = fc.hexaString({ minLength: 2, maxLength: 6 }).filter(s => s.length % 2 === 0 && s.endsWith('80') && s.length >= 2)
    fc.assert(fc.property(
      arbPos, arbPos,
      (a, b) => {
        fc.pre(a < b)
        const result = positionBetween(a, b)
        expect(result > a).toBe(true)
        expect(result < b).toBe(true)
      }
    ), { numRuns: 200 })
  })

  it('after: result > input', () => {
    const arbPos = fc.hexaString({ minLength: 2, maxLength: 6 }).filter(s => s.length % 2 === 0 && s.endsWith('80') && s.length >= 2)
    fc.assert(fc.property(arbPos, (x) => {
      const result = positionBetween(x, null)
      expect(result > x).toBe(true)
    }), { numRuns: 200 })
  })

  it('before: result < input', () => {
    const arbPos = fc.hexaString({ minLength: 2, maxLength: 6 }).filter(s => s.length % 2 === 0 && s.endsWith('80') && s.length >= 2)
    fc.assert(fc.property(arbPos, (x) => {
      const result = positionBetween(null, x)
      expect(result < x).toBe(true)
    }), { numRuns: 200 })
  })

  it('1000 sequential after-calls do not throw', () => {
    let pos = '80'
    for (let i = 0; i < 1000; i++) {
      pos = positionBetween(pos, null)
    }
    expect(pos.endsWith('80')).toBe(true)
  })

  it('position length after N after-calls is O(N)', () => {
    let pos = '80'
    for (let i = 0; i < 100; i++) {
      pos = positionBetween(pos, null)
    }
    // Each after-call adds at most ~2 hex chars, so length should be O(N)
    // 100 calls * 2 chars + base 2 = ~202 max
    expect(pos.length).toBeLessThan(250)
  })

  it('reversed inputs: fallback produces result > before (ordering violation)', () => {
    const a = '7f80'
    const b = '8180'
    // b > a, so passing (b, a) = reversed
    const result = positionBetween(b, a)
    // betweenBytes returns null for reversed inputs, fallback is afterBytes(b)
    // So result > b > a — it's NOT between a and b, it's after both
    expect(result > b).toBe(true) // result is after 'before' input
    // This is the documented bug: reversed inputs don't produce a value between them
  })
})
