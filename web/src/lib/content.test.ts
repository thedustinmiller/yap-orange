import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import {
  parseLinks,
  segmentContent,
} from './content'

// ============================================
// parseLinks
// ============================================
describe('parseLinks', () => {
  it('returns empty array for empty string', () => {
    expect(parseLinks('')).toEqual([])
  })

  it('returns empty array when no links present', () => {
    expect(parseLinks('hello world')).toEqual([])
  })

  it('parses a single link with correct path, start, end, resolved', () => {
    const result = parseLinks('see [[foo::bar]]')
    expect(result).toHaveLength(1)
    expect(result[0]).toEqual({
      raw: '[[foo::bar]]',
      path: 'foo::bar',
      start: 4,
      end: 16,
      resolved: true,
      isEmbed: false,
    })
  })

  it('parses multiple links with correct indices', () => {
    const result = parseLinks('go to [[alpha]] and [[beta]]')
    expect(result).toHaveLength(2)
    expect(result[0].path).toBe('alpha')
    expect(result[0].start).toBe(6)
    expect(result[0].end).toBe(15)
    expect(result[1].path).toBe('beta')
    expect(result[1].start).toBe(20)
    expect(result[1].end).toBe(28)
  })

  it('parses adjacent links', () => {
    const result = parseLinks('[[a]][[b]]')
    expect(result).toHaveLength(2)
    expect(result[0]).toMatchObject({ path: 'a', start: 0, end: 5 })
    expect(result[1]).toMatchObject({ path: 'b', start: 5, end: 10 })
  })

  it('parses link at start of string', () => {
    const result = parseLinks('[[foo]] bar')
    expect(result).toHaveLength(1)
    expect(result[0]).toMatchObject({ path: 'foo', start: 0, end: 7 })
  })

  it('parses link at end of string', () => {
    const result = parseLinks('bar [[foo]]')
    expect(result).toHaveLength(1)
    expect(result[0]).toMatchObject({ path: 'foo', start: 4, end: 11 })
  })

  it('parses link with quoted segment', () => {
    const result = parseLinks('[[foo::"bar baz"]]')
    expect(result).toHaveLength(1)
    expect(result[0].path).toBe('foo::"bar baz"')
  })

  it('returns empty array for empty brackets [[]]', () => {
    // Regex requires [^\]]+ which means at least one non-] character
    const result = parseLinks('[[]]')
    expect(result).toEqual([])
  })

  it('handles consecutive calls correctly (lastIndex reset)', () => {
    // The LINK_REGEX has the global flag. If lastIndex is not reset between
    // calls, the second call may miss matches or return incorrect results.
    const first = parseLinks('[[a]]')
    const second = parseLinks('[[b]]')
    expect(first).toHaveLength(1)
    expect(first[0].path).toBe('a')
    expect(second).toHaveLength(1)
    expect(second[0].path).toBe('b')
  })

  it('handles many consecutive calls without drift', () => {
    for (let i = 0; i < 20; i++) {
      const result = parseLinks(`[[link${i}]]`)
      expect(result).toHaveLength(1)
      expect(result[0].path).toBe(`link${i}`)
    }
  })
})

// ============================================
// Embed link parsing
// ============================================
describe('parseLinks - embeds', () => {
  it('parses embed link with isEmbed=true', () => {
    const result = parseLinks('![[foo::bar]]')
    expect(result).toHaveLength(1)
    expect(result[0].isEmbed).toBe(true)
    expect(result[0].path).toBe('foo::bar')
    expect(result[0].raw).toBe('![[foo::bar]]')
    expect(result[0].start).toBe(0)
    expect(result[0].end).toBe(13)
  })

  it('parses regular link with isEmbed=false', () => {
    const result = parseLinks('[[foo]]')
    expect(result).toHaveLength(1)
    expect(result[0].isEmbed).toBe(false)
  })

  it('parses mixed links and embeds', () => {
    const result = parseLinks('link [[a]] embed ![[b]] link [[c]]')
    expect(result).toHaveLength(3)
    expect(result[0].isEmbed).toBe(false)
    expect(result[0].path).toBe('a')
    expect(result[1].isEmbed).toBe(true)
    expect(result[1].path).toBe('b')
    expect(result[2].isEmbed).toBe(false)
    expect(result[2].path).toBe('c')
  })

  it('embed in middle of text has correct start/end', () => {
    const result = parseLinks('hello ![[img]] world')
    expect(result).toHaveLength(1)
    expect(result[0].start).toBe(6)
    expect(result[0].end).toBe(14)
    expect(result[0].isEmbed).toBe(true)
  })
})

// ============================================
// segmentContent
// ============================================
describe('segmentContent', () => {
  it('returns empty array for empty string', () => {
    expect(segmentContent('')).toEqual([])
  })

  it('returns single text segment for plain text', () => {
    const result = segmentContent('hello world')
    expect(result).toEqual([{ type: 'text', value: 'hello world' }])
  })

  it('returns single link segment for link-only content', () => {
    const result = segmentContent('[[foo]]')
    expect(result).toHaveLength(1)
    expect(result[0].type).toBe('link')
    expect(result[0].value).toBe('foo')
    expect(result[0].link).toBeDefined()
  })

  it('splits text-link-text into 3 segments', () => {
    const result = segmentContent('hello [[foo]] world')
    expect(result).toHaveLength(3)
    expect(result[0]).toEqual({ type: 'text', value: 'hello ' })
    expect(result[1].type).toBe('link')
    expect(result[1].value).toBe('foo')
    expect(result[2]).toEqual({ type: 'text', value: ' world' })
  })

  it('returns 2 link segments for adjacent links', () => {
    const result = segmentContent('[[a]][[b]]')
    expect(result).toHaveLength(2)
    expect(result[0].type).toBe('link')
    expect(result[0].value).toBe('a')
    expect(result[1].type).toBe('link')
    expect(result[1].value).toBe('b')
  })

  it('returns link then text for link at start', () => {
    const result = segmentContent('[[foo]] bar')
    expect(result).toHaveLength(2)
    expect(result[0].type).toBe('link')
    expect(result[0].value).toBe('foo')
    expect(result[1]).toEqual({ type: 'text', value: ' bar' })
  })

  it('returns text then link for link at end', () => {
    const result = segmentContent('bar [[foo]]')
    expect(result).toHaveLength(2)
    expect(result[0]).toEqual({ type: 'text', value: 'bar ' })
    expect(result[1].type).toBe('link')
    expect(result[1].value).toBe('foo')
  })

  it('returns embed segment for ![[...]]', () => {
    const result = segmentContent('see ![[img]]')
    expect(result).toHaveLength(2)
    expect(result[0]).toEqual({ type: 'text', value: 'see ' })
    expect(result[1].type).toBe('embed')
    expect(result[1].value).toBe('img')
  })

  it('handles mixed links and embeds', () => {
    const result = segmentContent('[[a]] then ![[b]]')
    expect(result).toHaveLength(3)
    expect(result[0].type).toBe('link')
    expect(result[1]).toEqual({ type: 'text', value: ' then ' })
    expect(result[2].type).toBe('embed')
  })
})

// ============================================
// Property tests (fast-check)
// ============================================
describe('content - property tests', () => {
  it('parseLinks: content.slice(start, end) === raw', () => {
    const arbWord = fc.stringOf(fc.char().filter(c => c !== '[' && c !== ']' && c !== '\n'), { minLength: 1, maxLength: 10 })
    const arbPlain = fc.stringOf(fc.char().filter(c => c !== '[' && c !== ']'), { minLength: 0, maxLength: 20 })
    const arbContent = fc.tuple(
      fc.array(fc.tuple(arbPlain, arbWord), { minLength: 1, maxLength: 5 }),
      arbPlain
    ).map(([pairs, suffix]) => {
      let content = ''
      for (const [text, word] of pairs) {
        content += text + `[[${word}]]`
      }
      return content + suffix
    })

    fc.assert(fc.property(arbContent, (content) => {
      const links = parseLinks(content)
      for (const link of links) {
        expect(content.slice(link.start, link.end)).toBe(link.raw)
      }
    }))
  })

  it('segmentContent: concatenation reproduces content', () => {
    fc.assert(fc.property(fc.string({ minLength: 0, maxLength: 100 }), (content) => {
      const segments = segmentContent(content)
      const reconstructed = segments.map(s => s.type === 'link' ? s.link!.raw : s.value).join('')
      expect(reconstructed).toBe(content)
    }))
  })

  it('TS regex cannot parse ] inside quoted segments (Rust can)', () => {
    const content = '[[foo::"bar]baz"]]'
    const tsLinks = parseLinks(content)
    // TS regex [^\]]+ stops at first ] inside the quotes, so the entire
    // link pattern fails to match. The Rust state-machine parser handles
    // this correctly and would return 1 link with path 'foo::"bar]baz"'.
    // Document the divergence: TS returns 0 links, Rust returns 1.
    expect(tsLinks.length).toBe(0)
  })
})
