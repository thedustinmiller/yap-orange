import { describe, it, expect } from 'vitest'
import { parseTypeCommand } from './typeCommand'

// ============================================
// Valid commands
// ============================================
describe('parseTypeCommand - valid commands', () => {
  it('parses @person{"name":"Alice"} correctly', () => {
    const result = parseTypeCommand('@person{"name":"Alice"}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('person')
    expect(result!.values).toEqual({ name: 'Alice' })
  })

  it('parses command with multiple fields', () => {
    const result = parseTypeCommand('@contact{"name":"Bob","email":"bob@example.com","age":30}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('contact')
    expect(result!.values).toEqual({ name: 'Bob', email: 'bob@example.com', age: 30 })
  })

  it('handles empty values: @person{}', () => {
    const result = parseTypeCommand('@person{}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('person')
    expect(result!.values).toEqual({})
  })

  it('handles boolean and number values', () => {
    const result = parseTypeCommand('@task{"done":false,"priority":1}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('task')
    expect(result!.values).toEqual({ done: false, priority: 1 })
  })

  it('handles null values in fields', () => {
    const result = parseTypeCommand('@item{"description":null}')
    expect(result).not.toBeNull()
    expect(result!.values).toEqual({ description: null })
  })

  it('handles nested objects in values', () => {
    const result = parseTypeCommand('@config{"settings":{"theme":"dark","size":12}}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('config')
    expect(result!.values).toEqual({ settings: { theme: 'dark', size: 12 } })
  })

  it('handles nested arrays in values', () => {
    const result = parseTypeCommand('@item{"tags":["a","b","c"]}')
    expect(result).not.toBeNull()
    expect(result!.values).toEqual({ tags: ['a', 'b', 'c'] })
  })

  it('trims surrounding whitespace', () => {
    const result = parseTypeCommand('  @person{"name":"Alice"}  ')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('person')
    expect(result!.values).toEqual({ name: 'Alice' })
  })

  it('trims leading/trailing newlines', () => {
    const result = parseTypeCommand('\n@todo{"title":"Buy milk"}\n')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('todo')
    expect(result!.values).toEqual({ title: 'Buy milk' })
  })

  it('type names can contain underscores', () => {
    const result = parseTypeCommand('@my_type{"field":"value"}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('my_type')
  })

  it('type names can contain digits', () => {
    const result = parseTypeCommand('@type2{"field":"value"}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('type2')
  })
})

// ============================================
// Case sensitivity
// ============================================
describe('parseTypeCommand - case sensitivity', () => {
  it('preserves lowercase type name', () => {
    const result = parseTypeCommand('@person{"name":"Alice"}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('person')
  })

  it('preserves uppercase type name', () => {
    const result = parseTypeCommand('@PERSON{"name":"Alice"}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('PERSON')
  })

  it('preserves mixed case type name', () => {
    const result = parseTypeCommand('@MyType{"name":"Alice"}')
    expect(result).not.toBeNull()
    expect(result!.typeName).toBe('MyType')
  })
})

// ============================================
// Returns null for non-commands
// ============================================
describe('parseTypeCommand - returns null for non-commands', () => {
  it('returns null for empty string', () => {
    expect(parseTypeCommand('')).toBeNull()
  })

  it('returns null for regular text content', () => {
    expect(parseTypeCommand('hello world')).toBeNull()
  })

  it('returns null for plain text with @ symbol', () => {
    expect(parseTypeCommand('email me at user@example.com')).toBeNull()
  })

  it('returns null if content has text before the command', () => {
    expect(parseTypeCommand('create @person{"name":"Alice"}')).toBeNull()
  })

  it('returns null if content has text after the command', () => {
    expect(parseTypeCommand('@person{"name":"Alice"} done')).toBeNull()
  })

  it('returns null for wiki link syntax', () => {
    expect(parseTypeCommand('[[some::path]]')).toBeNull()
  })

  it('returns null for content without braces', () => {
    expect(parseTypeCommand('@person')).toBeNull()
  })

  it('returns null for @ without type name', () => {
    expect(parseTypeCommand('@{"name":"Alice"}')).toBeNull()
  })

  it('returns null for type name with hyphens (non-word chars)', () => {
    expect(parseTypeCommand('@my-type{"name":"Alice"}')).toBeNull()
  })

  it('returns null for type name with spaces', () => {
    expect(parseTypeCommand('@my type{"name":"Alice"}')).toBeNull()
  })
})

// ============================================
// Returns null for invalid JSON
// ============================================
describe('parseTypeCommand - invalid JSON', () => {
  it('returns null for @type{invalid json}', () => {
    expect(parseTypeCommand('@person{invalid json}')).toBeNull()
  })

  it('returns null for @type{not: valid}', () => {
    expect(parseTypeCommand('@person{not: valid}')).toBeNull()
  })

  it('returns null for unclosed string in JSON', () => {
    expect(parseTypeCommand('@person{"name":"Alice}')).toBeNull()
  })

  it('returns null for trailing comma in JSON', () => {
    expect(parseTypeCommand('@person{"name":"Alice",}')).toBeNull()
  })

  it('returns null when JSON body is an array', () => {
    // @type{[1,2,3]} => match[2] = "[1,2,3]", JSON.parse("{[1,2,3]}") is invalid JSON
    expect(parseTypeCommand('@type{[1,2,3]}')).toBeNull()
  })

  it('returns null for mismatched braces', () => {
    expect(parseTypeCommand('@type{"a":{"b":"c"}')).toBeNull()
  })
})

// ============================================
// Return shape
// ============================================
describe('parseTypeCommand - return shape', () => {
  it('returns object with typeName and values keys only', () => {
    const result = parseTypeCommand('@note{"text":"hello"}')
    expect(result).not.toBeNull()
    expect(Object.keys(result!)).toEqual(['typeName', 'values'])
  })

  it('values is a plain object', () => {
    const result = parseTypeCommand('@note{"text":"hello"}')
    expect(result).not.toBeNull()
    expect(typeof result!.values).toBe('object')
    expect(result!.values).not.toBeNull()
    expect(Array.isArray(result!.values)).toBe(false)
  })
})
