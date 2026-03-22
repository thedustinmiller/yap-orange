# Import and Export

The Import/Export panel lets you transfer block subtrees as JSON files. You can export a namespace and its descendants, then import that data into another location or another yap-orange instance.

<!-- TODO: screenshot of import/export dialog -->

## Exporting

### Steps

1. Navigate to the block you want to export (it becomes the active namespace in the outliner).
2. Open the **Import/Export** panel tab.
3. The Export section shows the current namespace path.
4. (Optional) Use the **Include properties** checklist to select which property keys to include in the export. The `name` key is always included. Internal keys (prefixed with `_`) are shown in a separate section.
5. Click **Export**.
6. A file save dialog appears. Choose where to save the JSON file.
7. The status line shows the count of exported nodes and edges.

### Export Format

The export produces a `yap-tree-v1` JSON file containing:

- **nodes** -- the block subtree with content, properties, and hierarchy information.
- **edges** -- any semantic edges between nodes in the exported subtree.

## Importing

### Steps

1. Navigate to the **parent block** where you want the imported data to appear as children.
2. Open the **Import/Export** panel tab.
3. In the Import section, click **Choose file...** and select a previously exported JSON file.
4. Choose an import mode:
   - **Merge** -- skips blocks that already exist (matched by content). Avoids duplicates.
   - **Copy** -- creates fresh UUIDs for everything. Always creates new blocks.
5. (Merge mode only) Choose a **match strategy**:
   - **Auto** -- the server picks the best strategy.
   - **Content only** -- matches blocks by their content identity.
   - **Content + structure** -- matches by content and parent-child relationships (Merkle hash).
   - **Full topology** -- matches by the full subtree topology.
   - **Legacy (v1 compat)** -- uses the v1 export hash for backward compatibility.
6. (Merge mode only) Optionally enable **Link to existing content globally** -- when enabled, the import searches the entire database (not just the target subtree) for matching content to hard-link instead of creating new blocks.
7. Click **Import**.
8. The status line shows results: `Created X, Skipped Y` (and `Linked Z` if global linking was used).

### After Import

After a successful import, the outliner automatically reloads the target block's children so the imported blocks appear immediately. The file selection is cleared, ready for another import.

## Requirements

- **Export** requires an active namespace -- you must be navigated to a block (not at the root level).
- **Import** also requires an active namespace -- the imported blocks become children of the currently navigated block.
- Both operations are disabled (buttons grayed out) when no block is selected.

## Error Handling

If an export or import fails, the status line shows the error message prefixed with "Error:". Common issues:

- Network errors if the server is unreachable.
- Invalid JSON if the import file is corrupted or not a valid yap-tree export.
- Permission errors if the target block cannot be modified.
