# Future Considerations

Potential problems and solutions to revisit as the project evolves.

---

## Link parser: switch to nom if grammar grows

**Status:** Watch
**Trigger:** When the link syntax expands beyond simple `[[segments]]` — e.g., display text (`[[target|label]]`), anchors (`[[page#heading]]`), transclusion, or inline query syntax.

**Context:** The current link parser is a hand-rolled char-by-char state machine (~120 lines, 7 states). It works for the current grammar, but property tests revealed a class of bug where state interactions (e.g., `FoundFirstColon` consuming a `]` meant for `FoundFirstClose`) caused format/parse asymmetry. The fix was straightforward (quote `:` in `format_link`), but the underlying issue — that each new character class adds states that interact with every other state — scales poorly.

**Why nom:** Parser combinators make the grammar declarative. Each production rule (quoted segment, `::` separator, `]]` terminator) is an independent combinator. New syntax composes additively without cross-state interaction. Lookahead (`not`, `peek`) makes disambiguation explicit.

**Why not now:**
- The grammar is ~6 production rules — nom's boilerplate would rival the logic
- Scanning for `[[` in freeform text is awkward in nom (outer scan loop stays manual)
- Char-based position tracking (needed for CodeMirror) requires `nom_locate` or post-hoc byte→char conversion
- New dependency + learning curve for contributors
- The current parser + property tests cover the existing grammar reliably

**If we switch:** Use nom 8+. The outer "find `[[` in text" loop stays hand-written; nom handles everything between `[[` and `]]`. Use `nom_locate` for span tracking. Keep `format_link` and `process_relative_path` as-is (they're independent of the parser implementation).

---
