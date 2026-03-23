import { describe, it, expect } from 'vitest'
import {
  hasCustomView,
  getViewDefinition,
  getViewIcon,
} from './typeViewRegistry'

describe('typeViewRegistry', () => {
  describe('hasCustomView', () => {
    it('returns true for built-in types', () => {
      expect(hasCustomView('setting')).toBe(true)
      expect(hasCustomView('schema')).toBe(true)
      expect(hasCustomView('type_registry')).toBe(true)
      expect(hasCustomView('todo')).toBe(true)
    })

    it('returns true for media types', () => {
      expect(hasCustomView('image')).toBe(true)
      expect(hasCustomView('pdf')).toBe(true)
      expect(hasCustomView('file')).toBe(true)
    })

    it('returns false for unregistered types', () => {
      expect(hasCustomView('content')).toBe(false)
      expect(hasCustomView('raw_text')).toBe(false)
      expect(hasCustomView('nonexistent')).toBe(false)
    })

    it('returns false for null/undefined', () => {
      expect(hasCustomView(null)).toBe(false)
      expect(hasCustomView(undefined)).toBe(false)
      expect(hasCustomView('')).toBe(false)
    })
  })

  describe('getViewDefinition', () => {
    it('returns definition with load, icon, and label for media types', () => {
      const imageDef = getViewDefinition('image')
      expect(imageDef).toBeDefined()
      expect(imageDef!.icon).toBeTruthy()
      expect(imageDef!.label).toBe('Image')
      expect(typeof imageDef!.load).toBe('function')

      const pdfDef = getViewDefinition('pdf')
      expect(pdfDef).toBeDefined()
      expect(pdfDef!.label).toBe('PDF')

      const fileDef = getViewDefinition('file')
      expect(fileDef).toBeDefined()
      expect(fileDef!.label).toBe('File')
    })

    it('returns undefined for unregistered types', () => {
      expect(getViewDefinition('content')).toBeUndefined()
      expect(getViewDefinition('nonexistent')).toBeUndefined()
      expect(getViewDefinition(null)).toBeUndefined()
    })
  })

  describe('getViewIcon', () => {
    it('returns icons for media types', () => {
      expect(getViewIcon('image')).toBeTruthy()
      expect(getViewIcon('pdf')).toBeTruthy()
      expect(getViewIcon('file')).toBeTruthy()
    })

    it('returns undefined for unregistered types', () => {
      expect(getViewIcon('content')).toBeUndefined()
      expect(getViewIcon(null)).toBeUndefined()
    })
  })
})
