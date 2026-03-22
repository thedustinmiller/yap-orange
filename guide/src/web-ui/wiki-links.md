# Wiki Links

Wiki links connect blocks to each other using `[[double bracket]]` syntax. Links target lineage IDs internally, so they survive moves and edits without breaking.

## Creating a Wiki Link

1. Enter edit mode on a block (click its content or press Enter while selected).
2. Type `[[` to trigger the autocomplete popup.
3. Start typing to filter blocks by namespace path. The autocomplete fetches matching blocks from the API with debouncing.
4. Select a result with arrow keys and press Enter, or click it.
5. The completion inserts the full `[[namespace::path]]` syntax.

You can also type the full link manually: `[[research::ml::attention]]`.

<!-- TODO: screenshot of wiki-link autocomplete -->

## Link Syntax

### Absolute Paths

Reference any block by its full namespace path:

```
[[research::ml::attention]]
[[projects::yap-orange::docs]]
```

### Relative Paths

Reference blocks relative to the current location:

```
[[./child]]      -- child of current namespace
[[../sibling]]   -- sibling (child of parent)
[[..]]           -- parent namespace
```

### Quoted Segments

When a block name contains `::` (the path separator), wrap it in double quotes:

```
[["name with::colons"]]
[[research::"paper: attention is all you need"]]
```

## How Links Are Stored

When you save a block, links in the content are resolved to lineage IDs by the server. The stored atom contains lineage ID placeholders, not namespace paths. When content is read back, the server renders the current namespace paths for each lineage ID.

This means:
- **Moving a linked block** does not break the link. The lineage ID stays the same.
- **Renaming a linked block** does not break the link. The rendered path updates automatically.
- **Deleting a linked block** leaves a broken link (the lineage ID no longer resolves).

## Live Preview Decorations

The editor uses cursor-aware decorations for wiki links, similar to Obsidian's live preview:

- **Cursor away from a link** -- the `[[brackets]]` are hidden and the link path is rendered as a styled, clickable span. Clicking it navigates to the target block.
- **Cursor inside a link** -- the raw `[[path]]` syntax is shown with colored text, so you can edit the path directly.

This behavior is implemented as a CM6 `ViewPlugin` that rebuilds decorations whenever the document, selection, or viewport changes.

## Clicking a Wiki Link

When you click a rendered wiki link (in either edit mode or display mode):

1. If in edit mode, the current block's content is saved first.
2. The link path is resolved via the API.
3. If the target block exists, the outliner navigates to it.
4. If the target block does not exist, a new block is created at that path and then navigated to.

## Wiki Links in the Links Panel

The Links panel (right side) shows all link relationships for the currently selected block:

- **Links to** -- blocks that the current block's content links to (outlinks).
- **Linked from** -- blocks whose content links to the current block (backlinks).
- Each entry shows the namespace path and a content snippet. Click an entry to navigate to that block.
