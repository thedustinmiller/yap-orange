# Export / Import Format

yap-orange uses a portable JSON format (`yap-tree-v1`) for exporting and importing block subtrees. This enables moving content between databases, seeding fresh installations with sample data, and building tools that produce yap-compatible content.

## Format Overview

```json
{
  "format": "yap-tree-v1",
  "exported_at": "2026-03-05T02:57:46Z",
  "source_namespace": "sample::recipes",
  "nodes": [ ... ],
  "edges": [ ... ]
}
```

Nodes are in BFS order (parents always appear before their children). The root node has `parent_local_id: null`.

## Node Structure

```json
{
  "local_id": 0,
  "name": "recipes",
  "content_type": "content",
  "content_template": "A collection of {0} recipes.",
  "internal_links": [
    { "placeholder_index": 0, "target_local_id": 1 }
  ],
  "external_links": [
    { "placeholder_index": 1, "target_path": "ingredients::pantry::olive-oil" }
  ],
  "properties": {},
  "export_hash": "e6e111d...",
  "parent_local_id": null,
  "position": "80",
  "children_local_ids": [1, 2]
}
```

### Fields

| Field | Description |
|-------|-------------|
| `local_id` | Monotonic integer within this export (0 = root) |
| `name` | Block name (last segment of namespace path) |
| `content_type` | `"content"` (default), `"raw_text"`, `"schema"`, `"setting"`, `"type_registry"`, `"todo"`, or any user-defined type name |
| `content_template` | Text with `{N}` placeholders for links |
| `internal_links` | Links whose targets are within this export |
| `external_links` | Links whose targets are outside this export |
| `properties` | JSONB key-value metadata |
| `export_hash` | SHA-256 for deduplication (see below) |
| `parent_local_id` | `local_id` of parent node, or `null` for root |
| `position` | Fractional index string for ordering |
| `children_local_ids` | Ordered list of child `local_id` values |

### Link Splitting

The `links` array from the atom is split into two lists:

**`internal_links`** — targets within the exported subtree:
```json
{ "placeholder_index": 0, "target_local_id": 4 }
```
The `target_local_id` refers to another node in this same export.

**`external_links`** — targets outside the exported subtree:
```json
{ "placeholder_index": 1, "target_path": "ingredients::olive-oil" }
```
The `target_path` is the namespace path in the source database. During import, it will be resolved in the target database.

## Edge Structure

```json
{
  "from_local_id": 8,
  "to_local_id": 5,
  "edge_type": "inspired-by",
  "properties": {}
}
```

Only edges where **both** endpoints are within the exported subtree are included. Cross-subtree edges are not exported.

## Export Hash

The `export_hash` is a SHA-256 computed as:

```
SHA256(content_type || "\x00" || content_template || "\x00" || sorted_internal_local_ids_as_u32_le_bytes)
```

This hash is stable across database instances because it uses `local_id` integers instead of UUIDs. It enables content-addressed deduplication during `merge` imports.

On import, the hash is stored as `_import_hash` in the atom's `properties` JSONB.

## Import Modes

### `merge` (default)

Before creating a node, the importer:

1. Computes the same hash from the export node's content
2. Looks for an existing atom with `properties->>'_import_hash' = hash`
3. If found: reuses the existing lineage (no new rows created)
4. If not found: creates a new lineage+atom pair

External links are resolved against the target database by namespace path. If the path doesn't exist in the target, the link placeholder becomes `Uuid::nil()` (zero UUID).

### `copy`

Creates all nodes with fresh UUIDs regardless of existing content. External links always become `Uuid::nil()`.

Use `copy` mode when you want an independent duplicate of the subtree (e.g., forking a template).

## Import Result

```json
{
  "created": 5,
  "merged": 2,
  "skipped": 0
}
```

## CLI Usage

```bash
# Export
yap export research::ml --output ml-export.json
yap export <block-id> --output subtree.json

# Import
yap import ml-export.json --parent projects
yap import ml-export.json --parent projects --mode copy
yap import ml-export.json --parent projects --mode merge
```

## HTTP API Usage

```bash
# Export via HTTP
curl http://localhost:3000/api/blocks/<id>/export > subtree.json

# Import via HTTP
curl -X POST http://localhost:3000/api/blocks/<parent-id>/import \
  -H "Content-Type: application/json" \
  -d @subtree.json
```

## Example

See `fixtures/sample-recipes.json` for a complete example with internal links and edges.

```bash
# Load the sample data
yap import fixtures/sample-recipes.json --parent root_block_id

# Or create a root namespace first
yap ns create sample
yap import fixtures/sample-recipes.json --parent $(yap link resolve sample --json | jq -r .block_id)
```
