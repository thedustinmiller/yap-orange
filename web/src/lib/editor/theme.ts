/**
 * CM6 theme for yap-orange — Tokyo Night dark theme using CSS variables.
 * Designed for inline block editing (no line numbers, no fold gutters).
 */
import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags } from '@lezer/highlight'
import type { Extension } from '@codemirror/state'

const editorTheme = EditorView.theme(
  {
    '&': {
      backgroundColor: 'var(--bg-input)',
      color: 'var(--text-primary)',
      fontSize: '13px',
      fontFamily: 'inherit',
      lineHeight: '1.5',
    },
    '&.cm-focused': {
      outline: 'none',
    },
    '.cm-content': {
      caretColor: 'var(--accent-bright)',
      padding: '4px 2px',
      fontFamily: 'inherit',
    },
    '.cm-cursor, .cm-dropCursor': {
      borderLeftColor: 'var(--accent-bright)',
      borderLeftWidth: '2px',
    },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
      background: 'var(--accent-glow) !important',
    },
    '.cm-activeLine': {
      backgroundColor: 'transparent',
    },
    '.cm-selectionMatch': {
      backgroundColor: 'rgba(122, 162, 247, 0.15)',
    },
    '.cm-scroller': {
      overflow: 'auto',
    },
    // Autocomplete tooltip styling
    '.cm-tooltip': {
      backgroundColor: 'var(--bg-secondary)',
      border: '1px solid var(--border-color)',
      borderRadius: '4px',
      boxShadow: '0 4px 12px rgba(0, 0, 0, 0.3)',
    },
    '.cm-tooltip-autocomplete': {
      '& > ul': {
        fontFamily: 'var(--font-mono)',
        fontSize: '12px',
      },
      '& > ul > li': {
        padding: '4px 8px',
      },
      '& > ul > li[aria-selected]': {
        backgroundColor: 'var(--bg-active)',
        color: 'var(--text-primary)',
      },
    },
  },
  { dark: true },
)

const highlightStyle = HighlightStyle.define([
  { tag: tags.heading1, color: '#ff9e64', fontWeight: 'bold', fontSize: '1.3em' },
  { tag: tags.heading2, color: '#ff9e64', fontWeight: 'bold', fontSize: '1.15em' },
  { tag: tags.heading3, color: '#ff9e64', fontWeight: 'bold' },
  { tag: [tags.heading4, tags.heading5, tags.heading6], color: '#e0af68', fontWeight: 'bold' },
  { tag: tags.strong, color: '#c0caf5', fontWeight: 'bold' },
  { tag: tags.emphasis, color: '#c0caf5', fontStyle: 'italic' },
  { tag: tags.strikethrough, textDecoration: 'line-through', color: '#565f89' },
  { tag: tags.link, color: 'var(--link-color)', textDecoration: 'underline' },
  { tag: tags.url, color: 'var(--link-color)' },
  { tag: [tags.processingInstruction, tags.monospace], color: '#9ece6a', fontFamily: 'var(--font-mono)' },
  { tag: tags.quote, color: '#565f89', fontStyle: 'italic' },
  { tag: tags.list, color: '#bb9af7' },
  { tag: tags.meta, color: '#565f89' },
  { tag: tags.contentSeparator, color: '#565f89' },
])

export const yapTheme: Extension = [editorTheme, syntaxHighlighting(highlightStyle)]
