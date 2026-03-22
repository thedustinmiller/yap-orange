# Editing Blocks

Blocks are the fundamental unit of content in yap-orange. Each block has a name, optional text content, and optional properties. This chapter covers how to create and edit blocks in the web UI.

## Creating Blocks

There are two ways to create a new block:

### Quick-Create (Outliner Header)

1. Click the **+** button in the outliner header.
2. A text input appears. Type a name for the new block.
3. Press **Enter** to create it. Press **Escape** to cancel.

The block is created as a child of the current namespace (the block you are centered on in the outliner). If you are at the root level, it becomes a root block.

### From Edit Mode (Enter Key)

While editing a block's content:

1. Press **Enter** to save the current block and create a new sibling block below it.
2. The new block is automatically named with a timestamp (e.g., `note-20260315-143022`).
3. The editor jumps to the new block in edit mode.

The new block is positioned between the current block and its next sibling using fractional indexing, so existing ordering is preserved.

## Edit Mode vs Display Mode

Each block in the outliner has two visual states:

### Display Mode (Default)

- Content is rendered through `ContentRenderer`, which processes wiki links into styled, clickable elements.
- Wiki links appear as colored text that you can click to navigate.
- Blocks with no content show their name in italic muted text.

### Edit Mode

- Click on a block's content area to enter edit mode.
- A CodeMirror 6 editor appears with a highlighted border.
- The editor supports markdown syntax highlighting and wiki-link autocomplete.
- Wiki links show as styled text when your cursor is away from them, but reveal the raw `[[path]]` syntax when your cursor moves inside.

<!-- TODO: screenshot of editor in action -->

## Saving Content

Content is saved in several ways:

- **Escape** -- saves content and returns to navigation mode.
- **Cmd/Ctrl+Enter** -- saves content and returns to navigation mode.
- **Blur** (clicking away) -- auto-saves content without changing modes.
- **Enter** -- saves current content and creates a new sibling block.
- **Arrow Up/Down at boundary** -- saves and moves to adjacent block when the cursor is on the first or last line.
- **Tab / Shift+Tab** -- saves content and indents/outdents the block.

All save paths go through the same handler, so content is never lost when transitioning between blocks or modes.

## Editor Keybindings

| Key | Action |
|-----|--------|
| `Escape` | Save and return to navigation mode |
| `Cmd/Ctrl+Enter` | Save and return to navigation mode |
| `Enter` | Save and create new sibling block below |
| `Shift+Enter` | Insert a newline (multi-line content) |
| `Tab` | Save and indent block (reparent under previous sibling) |
| `Shift+Tab` | Save and outdent block (reparent under grandparent) |
| `Arrow Up` (at first line) | Save and move to previous block |
| `Arrow Down` (at last line) | Save and move to next block |
| `[[` | Open wiki-link autocomplete |

## Custom View Editing

Blocks with custom content types (entries, todos, schemas) follow the same edit pattern as regular content blocks:

- **Click** or press **Enter** to enter edit mode.
- **Escape** saves and exits back to nav mode.

In edit mode, custom views show a full form instead of their compact nav mode summary. Use **Tab** and **Shift+Tab** to cycle between inputs within the form. The first input is auto-focused when entering edit mode.

This means the outliner editing flow is consistent across all block types — regular content blocks and typed blocks share the same Enter/Escape interaction model.

## Block Icons

Each block in the outliner shows a contextual icon:

- Custom type icon (from the type view registry) for blocks with registered content types
- Folder icon for blocks with no text content (namespace/container blocks)
- Clipboard icon for blocks that have both content and children
- Checkbox icon for todo blocks
- Document icon for regular content blocks
