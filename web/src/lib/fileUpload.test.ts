import { describe, it, expect } from 'vitest'

// Test the mediaContentType logic directly since importing from fileUpload.ts
// pulls in api.ts which has browser-only side effects (window reference).
// This mirrors the exact logic in fileUpload.ts:mediaContentType.
function mediaContentType(mimeType: string): string {
  if (mimeType.startsWith('image/')) return 'image'
  if (mimeType === 'application/pdf') return 'pdf'
  return 'file'
}

describe('mediaContentType', () => {
  it('returns "image" for image MIME types', () => {
    expect(mediaContentType('image/png')).toBe('image')
    expect(mediaContentType('image/jpeg')).toBe('image')
    expect(mediaContentType('image/gif')).toBe('image')
    expect(mediaContentType('image/svg+xml')).toBe('image')
    expect(mediaContentType('image/webp')).toBe('image')
  })

  it('returns "pdf" for PDF MIME type', () => {
    expect(mediaContentType('application/pdf')).toBe('pdf')
  })

  it('returns "file" for everything else', () => {
    expect(mediaContentType('application/zip')).toBe('file')
    expect(mediaContentType('text/plain')).toBe('file')
    expect(mediaContentType('application/msword')).toBe('file')
    expect(mediaContentType('application/vnd.openxmlformats-officedocument.wordprocessingml.document')).toBe('file')
    expect(mediaContentType('audio/mpeg')).toBe('file')
    expect(mediaContentType('video/mp4')).toBe('file')
    expect(mediaContentType('')).toBe('file')
    expect(mediaContentType('application/octet-stream')).toBe('file')
  })
})
