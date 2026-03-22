# Custom Types

Custom types let you define structured data schemas and create typed entries (blocks with a custom content type). Schemas live under the `types::` namespace and define the fields that entries of that type will have.

## The Type Registry

The `types` root block is a special namespace that acts as a type registry. It uses a custom view (`TypeRegistryView`) that lists all schema children with field summaries and provides a form to create new types.

To view the type registry:

1. Navigate to the `types` block in the sidebar.
2. The outliner shows the Type Registry View instead of the normal editor.
3. Existing schemas are listed with their field counts.
4. Use the "New type name..." input at the bottom to create a new schema.

## Creating a Schema

1. Navigate to the `types` block.
2. Type a name in the "New type name..." field and press Enter.
3. A new schema block is created under `types::` with `content_type = "schema"`.
4. Click the new schema to view its field editor.

## Editing Schema Fields

When you navigate to a schema block (e.g., `types::person`), the Schema View replaces the normal editor. It shows a grid with columns:

| Column | Description |
|--------|-------------|
| **Name** | The field name (e.g., `email`, `age`) |
| **Type** | Field type: `string`, `number`, `boolean`, `date`, `enum`, `ref`, `text` |
| **Options / Target** | For `enum` type: comma-separated values. For `ref` type: the target type name. |
| **Req** | Whether the field is required (checkbox) |
| **Delete** | Remove the field (x button) |

<!-- TODO: screenshot of schema editor -->

### Steps to Add a Field

1. Click **+ Add field** at the bottom of the field list.
2. Enter a field name.
3. Select a type from the dropdown.
4. If the type is `enum`, enter comma-separated options (e.g., `active, inactive, archived`).
5. If the type is `ref`, enter the target type name.
6. Check the Required box if needed.

Changes auto-save after a 500ms debounce. Fields are stored in the schema block's `properties.fields` array.

## Creating Entries with `@type{...}`

The `@type{...}` command lets you create a typed entry by setting a block's content type and properties in one step. This is a one-shot client-side command — it is not persistent syntax.

### Using the `@type{...}` Command

1. Enter edit mode on a block.
2. Type the command with the type name and field values as JSON:
   ```
   @person{"name":"Alice","email":"alice@example.com"}
   ```
3. Save the block.

### What Happens on Save

When you save a block containing an `@type{...}` command:

1. The `typeCommand.ts` parser extracts the command from the content.
2. The block's `content_type` is set to the type name (e.g., `person`).
3. The JSON field values are stored in the block's `properties`.
4. The `_schema_atom_id` is set to pin the entry to the current schema version.
5. The server receives the block with the resulting content_type and properties — it never sees the `@type{...}` syntax.

### Entries and EntryView

Entries (blocks with a custom content type) are rendered by a two-tier view system:

1. **Custom views** take priority if one is registered for the content type (e.g., `TodoView` for `todo`).
2. **EntryView** is the generic fallback — it auto-generates a schema-driven form with field views for each schema field (StringField, NumberField, BooleanField, DateField, EnumField, TextField, RefField).

Entries store all data in `properties`. There is no freeform content on typed blocks (except via `text` fields defined in the schema).

### Nav Mode and Edit Mode

All custom views (EntryView, TodoView, SchemaView) have two display modes that follow the same Enter/Escape pattern as regular content blocks:

**Nav mode** (default) shows a compact, inline summary for scanning:

- **EntryView**: `name: Alice · email: alice@ex.com`
- **TodoView**: `[checkbox] STATUS description... time`
- **SchemaView**: `name:string · email:string · birthday:date`

**Edit mode** shows the full form, activated by pressing Enter or clicking the block:

- Tab / Shift+Tab cycles between inputs within the view.
- Escape saves and exits back to nav mode.
- The first input is auto-focused when entering edit mode.

Clicking a custom view block in the outliner enters edit mode just like clicking a regular content block — custom views do not block click propagation in nav mode.

## Content Types

Every block has a `content_type` that determines how it is rendered:

| Content Type | Description |
|-------------|-------------|
| `content` | Default. Markdown text with wiki-links |
| `raw_text` | Literal monospace rendering, no markdown |
| `schema` | Type definition (SchemaView) |
| `setting` | UI settings (SettingsView) |
| `type_registry` | Schema manager (TypeRegistryView) |
| `todo` | Task with status cycling (TodoView) |
| *any other string* | User-defined type, rendered by EntryView or custom view |

## Type View Registry

The type view system is data-driven and extensible. The registry at `web/src/lib/views/typeViewRegistry.ts` maps `content_type` strings to view definitions:

| Content Type | View Component | Icon |
|-------------|---------------|------|
| `setting` | SettingsView | Gear |
| `schema` | SchemaView | Hexagon |
| `type_registry` | TypeRegistryView | Hexagon |
| `todo` | TodoView | Checkbox |
| *other* | EntryView | (from registry or default) |

When a block's `content_type` has a registered view, the outliner renders that custom component instead of the standard BlockEditor/ContentRenderer pair. The component receives the `TreeNode` and `isEditing` boolean as props.

New content types can be added by inserting entries into the `VIEW_DEFINITIONS` record. Each entry specifies a lazy-loaded Svelte component, an icon string, and a human-readable label. Views should accept `isEditing` as an optional boolean prop and render a compact nav mode summary when `false`, and a full editing form when `true`.
