# Keyboard Shortcuts

yap-orange is designed for keyboard-driven workflows. This page is the complete shortcut reference.

## Panel Navigation

### Alt+N Panel Toggle

Switch between panels without the mouse using JetBrains-style shortcuts:

| Shortcut | Panel |
|----------|-------|
| `Alt+1` | Navigator (sidebar) |
| `Alt+2` | Bookmarks |
| `Alt+3` | Links (backlinks/outlinks/edges) |
| `Alt+4` | Properties |
| `Alt+5` | Graph |

**Toggle behavior:**
- If the panel is **hidden** (was closed), it is re-added and focused.
- If the panel is **visible but not focused**, it receives focus.
- If the panel is **already focused**, focus returns to the last active outliner.

This three-state toggle means you can glance at a panel and bounce back to your editing flow with the same shortcut.

### Quick Switcher (Ctrl+K)

Press **Ctrl+K** to open the Quick Switcher -- a popup overlay listing all panels in most-recently-used (MRU) order.

- **Arrow Up/Down** or repeated **Ctrl+K** to cycle through the list.
- **Enter** to switch to the highlighted panel.
- **Escape** to dismiss without switching.

The MRU order means your most recently used panels appear at the top, making it fast to toggle between two panels.

## Outliner Navigation Mode

When the outliner is focused and you are **not** editing a block (mode shows `NAV` in the status bar):

| Key | Action |
|-----|--------|
| `Arrow Down` | Select next visible block |
| `Arrow Up` | Select previous visible block |
| `Arrow Right` | Expand selected block, or move to first child if already expanded |
| `Arrow Left` | Collapse selected block, or move to parent if already collapsed |
| `Enter` | Enter edit mode on the selected block |
| `Escape` | Clear selection |
| `Tab` | Indent selected block(s) under their previous sibling |
| `Shift+Tab` | Outdent selected block(s) up one level |
| `Delete` / `Backspace` | Delete selected block(s) |

### Multi-Select

| Input | Action |
|-------|--------|
| `Shift+Arrow Down` | Extend selection downward |
| `Shift+Arrow Up` | Extend selection upward |
| `Ctrl/Cmd+Click` | Toggle a block in/out of the selection |
| `Shift+Click` | Select a contiguous range from anchor to clicked block |

Multi-selected blocks can be indented, outdented, or deleted together.

## Editor Mode

When editing a block (mode shows `EDIT` in the status bar):

| Key | Action |
|-----|--------|
| `Escape` | Save content and return to NAV mode |
| `Ctrl/Cmd+Enter` | Save content and return to NAV mode |
| `Enter` | Save and create a new sibling block below |
| `Shift+Enter` | Insert a newline (regular line break) |
| `Tab` | Save and indent the block |
| `Shift+Tab` | Save and outdent the block |
| `Arrow Up` (on first line) | Save and edit the previous block |
| `Arrow Down` (on last line) | Save and edit the next block |

### Autocomplete Triggers

| Trigger | What It Opens |
|---------|--------------|
| `[[` | Wiki link completion (search blocks by namespace path) |

## Tips

- **Quick panel glance**: Press `Alt+3` to check backlinks, read what you need, press `Alt+3` again to jump back to the outliner.
- **Rapid outlining**: In edit mode, Enter-type-Enter-type builds a flat list. Then switch to NAV mode and use Tab to nest items.
- **Panel cycling**: If you use Links and Properties frequently, `Ctrl+K` + `Enter` is faster than reaching for the mouse.
