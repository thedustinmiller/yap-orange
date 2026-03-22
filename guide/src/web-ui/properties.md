# Properties Panel

The Properties panel provides a JSON editor for viewing and editing the metadata (properties) attached to a block's atom.

<!-- TODO: screenshot of properties panel -->

## Viewing Properties

The Properties panel automatically displays properties for whichever block is active:

1. If a block is **selected** in the outliner and has properties, those are shown.
2. Otherwise, the **currently navigated block** (the one the outliner is centered on) is shown if it has properties.
3. If neither has properties, the panel shows "No properties on this block" or "Select a block to see properties."

## Editing Properties

The editor is a full CodeMirror 6 instance configured for JSON syntax highlighting. To edit:

1. Click into the Properties panel editor.
2. Modify the JSON. The editor validates your input as you type.
3. Click away (blur) to save. The properties are persisted to the server automatically.

The editor supports standard features: undo/redo, line wrapping, and monospace font.

## Status Indicators

The Properties panel header shows several indicators:

| Indicator | Meaning |
|-----------|---------|
| **Key count badge** | A colored badge showing the number of top-level keys in the properties object. |
| **Orange dot** | The properties have been modified but not yet saved (dirty state). |
| **"saving..."** | A save is currently in progress. |
| **"invalid JSON"** | The current editor content is not valid JSON. The editor border turns red. Properties will not be saved until the JSON is valid. |

## Automatic Block Switching

When you select a different block or navigate to a different namespace, the Properties panel:

1. Flushes any pending save for the previous block (fire-and-forget).
2. Loads the new block's properties into the editor.
3. Resets the dirty and error states.

This means you can freely navigate between blocks without losing unsaved property changes.

## Common Use Cases

- **Typed blocks** -- schemas, settings, and custom-typed blocks store their structured data in properties. The Properties panel lets you see and edit the raw JSON.
- **Schema fields** -- a schema block's `fields` array lives in properties. While the Schema View provides a friendly grid editor, you can also edit fields directly as JSON here.
- **Debugging** -- inspect any block's metadata when something looks wrong. Dev mode blocks, edge properties, and import metadata are all visible here.
