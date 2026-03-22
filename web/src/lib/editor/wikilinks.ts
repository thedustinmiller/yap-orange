/**
 * CM6 ViewPlugin for cursor-aware wiki link decorations.
 *
 * When the cursor is AWAY from a [[link]], renders a styled clickable span
 * (Obsidian-style live preview — brackets hidden, just the path).
 *
 * When the cursor is INSIDE a [[link]], shows the raw [[path]] text with
 * a colored mark decoration so the user can edit the path directly.
 */
import {
  ViewPlugin,
  Decoration,
  WidgetType,
  type EditorView,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view'
import { RangeSetBuilder } from '@codemirror/state'
import type { Extension } from '@codemirror/state'

const LINK_RE = /\[\[([^\]]+)\]\]/g

class WikiLinkWidget extends WidgetType {
  constructor(
    readonly path: string,
    readonly onNavigate: (path: string) => void,
  ) {
    super()
  }

  toDOM(): HTMLElement {
    const span = document.createElement('span')
    span.className = 'cm-wiki-link'
    span.textContent = this.path
    span.title = `Navigate to ${this.path}`
    span.addEventListener('click', (e) => {
      e.preventDefault()
      e.stopPropagation()
      this.onNavigate(this.path)
    })
    return span
  }

  eq(other: WikiLinkWidget): boolean {
    return this.path === other.path
  }

  ignoreEvent(): boolean {
    return false
  }
}

/** Mark decoration for when cursor is inside the link — colors the raw text */
const linkMark = Decoration.mark({ class: 'cm-wiki-link-editing' })

function buildDecorations(
  view: EditorView,
  onNavigate: (path: string) => void,
): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>()
  const doc = view.state.doc

  // Collect cursor ranges
  const cursors = view.state.selection.ranges

  for (let i = 1; i <= doc.lines; i++) {
    const line = doc.line(i)
    let match: RegExpExecArray | null
    LINK_RE.lastIndex = 0

    while ((match = LINK_RE.exec(line.text)) !== null) {
      const from = line.from + match.index
      const to = from + match[0].length
      const path = match[1]

      // Check if any cursor overlaps this link range
      const cursorInside = cursors.some(
        (r) => r.from < to && r.to > from,
      )

      if (cursorInside) {
        // Cursor inside: show raw text with mark styling
        builder.add(from, to, linkMark)
      } else {
        // Cursor away: replace with styled widget
        builder.add(
          from,
          to,
          Decoration.replace({
            widget: new WikiLinkWidget(path, onNavigate),
          }),
        )
      }
    }
  }

  return builder.finish()
}

/**
 * Returns a CM6 extension that provides cursor-aware wiki link decorations.
 *
 * @param onNavigate - Called when user clicks a wiki link widget
 */
export function wikiLinks(onNavigate: (path: string) => void): Extension {
  return ViewPlugin.define(
    (view) => ({
      decorations: buildDecorations(view, onNavigate),
      update(update: ViewUpdate) {
        if (
          update.docChanged ||
          update.selectionSet ||
          update.viewportChanged
        ) {
          this.decorations = buildDecorations(update.view, onNavigate)
        }
      },
    }),
    {
      decorations: (v) => v.decorations,
    },
  )
}
