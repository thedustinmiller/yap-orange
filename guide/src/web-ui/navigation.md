# Navigation

The web UI provides multiple ways to navigate the block hierarchy: the sidebar tree, the outliner, keyboard shortcuts, breadcrumbs, URL routing, and drag-and-drop reordering.

## Sidebar

The sidebar (left panel, titled "Blocks") shows the top-level namespace tree.

- **Click a block** to navigate into it. The outliner centers on that block and shows its children.
- **Click the triangle** next to a block to expand/collapse its children in the sidebar. Children are loaded lazily on first expand.
- **Child count badge** -- blocks with children show a count badge.
- **Home button** -- click the house icon in the sidebar header to return to the root view (all top-level blocks).
- **Delete** -- hover over a block and click the X to delete it and all its children. A confirmation dialog appears first. Deleted blocks can be recovered from the Orphans view.

<!-- TODO: screenshot of sidebar + outliner -->

## Outliner

The outliner (center panel) shows the hierarchical view of the currently navigated block and its descendants.

### Centering on a Block

When you navigate to a block (via sidebar click, wiki link, or URL), that block becomes the "virtual root" of the outliner. It appears at the top with its children indented below. This is called "centering" or "center perspective."

To center on any block visible in the outliner, click the **target icon** that appears on hover at the right edge of a block row.

### Breadcrumbs

The outliner header shows a breadcrumb trail from the root to the current block. Click any breadcrumb segment to navigate to that ancestor. Click the `~` at the start to return home.

### Expanding and Collapsing

- Click the **triangle** to the left of a block to expand or collapse it.
- Blocks with no text content (namespace/container blocks) auto-expand on load.
- If a block was expanded in a previous session (state is persisted), it auto-loads its children on mount.

### Expand All / Collapse All

The outliner header has two buttons:

- **Expand all** -- expands all blocks recursively, loading children as needed. Respects the `max_expand_depth` setting (default 0 = unlimited).
- **Collapse all** -- collapses every expanded block in the outliner.

## Keyboard Navigation

### Panel Switching

You can navigate between panels without the mouse:

| Shortcut | Action |
|----------|--------|
| `Alt+1` through `Alt+5` | Toggle focus on Navigator, Bookmarks, Links, Properties, Graph |
| `Ctrl+K` | Open the Quick Switcher (MRU panel list) |

See [Keyboard Shortcuts](./keyboard-shortcuts.md) for the full reference.

### Outliner Navigation

When in navigation mode (not editing), the outliner supports full keyboard control:

| Key | Action |
|-----|--------|
| `Arrow Down` | Select next visible block |
| `Arrow Up` | Select previous visible block |
| `Arrow Right` | Expand selected block, or move into first child if already expanded |
| `Arrow Left` | Collapse selected block, or move to parent if already collapsed |
| `Enter` | Enter edit mode on selected block |
| `Escape` | Clear selection |
| `Shift+Arrow Down` | Extend selection downward (multi-select) |
| `Shift+Arrow Up` | Extend selection upward (multi-select) |
| `Tab` | Indent selected block(s) -- reparent under previous sibling |
| `Shift+Tab` | Outdent selected block(s) -- reparent under grandparent |
| `Delete` / `Backspace` | Delete selected block(s) |

### Multi-Select

- **Ctrl/Cmd+Click** toggles a block in/out of the selection.
- **Shift+Click** selects a contiguous range from the selection anchor to the clicked block.
- **Shift+Arrow** extends the selection in the given direction.
- Multi-selected blocks can be indented/outdented together with Tab/Shift+Tab.

## URL Routing

The web UI uses hash-based URLs for navigation:

| URL | Destination |
|-----|-------------|
| `/#/` | Home (root blocks) |
| `/#/research::ml::attention` | Navigate to namespace path |
| `/#/block/<UUID>` | Navigate to block by ID |

Browser back/forward buttons work as expected. The URL updates automatically when you navigate.

## Drag and Drop

Blocks in the outliner can be reordered and reparented by dragging:

1. **Drag** a block by clicking and holding anywhere on its row.
2. As you drag over other blocks, drop zone indicators appear:
   - **Top quarter** -- drop above (reorder as sibling before target).
   - **Bottom quarter** -- drop below (reorder as sibling after target).
   - **Middle half** -- drop inside (reparent as child of target).
3. **Release** to complete the move.

Drag-and-drop respects the hierarchy:
- You cannot drop a block onto itself.
- You cannot drop a block onto one of its own descendants (which would create a cycle).
- When dragging a multi-selected set of blocks, all selected blocks move together.

After a drop, the outliner reloads both the old and new parent blocks to reflect the change.
