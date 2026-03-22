# The Export Format

yap-orange uses a portable JSON format called `yap-tree-v1` for exporting and importing subtrees of content. This chapter explains the format's structure, the reasoning behind its design choices, and how the import process works.

## Why a Custom Format?

A yap-orange subtree isn't just a collection of files. It's a graph of linked content with hierarchy, ordering, typed metadata, and semantic relationships. Markdown files in a folder could capture the text, but they'd lose the links (which are lineage UUIDs internally), the edges, the properties, the fractional ordering, and the content type system. The `yap-tree-v1` format preserves all of this in a single self-contained JSON document.

The format is designed for three use cases:

1. **Moving content between databases.** Export from your desktop app, import into a shared server, or vice versa.
2. **Seeding fresh installations.** Ship sample data as `.json` fixtures that can be imported on first run.
3. **Tool interoperability.** External tools can produce `yap-tree-v1` JSON to feed content into yap-orange without needing database access.

## Top-Level Structure

```json
{
  "format": "yap-tree-v1",
  "exported_at": "2026-03-05T02:57:46Z",
  "source_namespace": "sample::recipes",
  "nodes": [ ... ],
  "edges": [ ... ]
}
```

| Field | Purpose |
|-------|---------|
| `format` | Version identifier. Always `"yap-tree-v1"` for this format. |
| `exported_at` | ISO 8601 timestamp of when the export was created. |
| `source_namespace` | The namespace path of the root block in the source database. Informational — not used during import. |
| `nodes` | The exported blocks, in breadth-first order. |
| `edges` | Semantic relationships between nodes in the export. |

## Nodes

Each node represents one block and its associated content (lineage + current atom). Nodes are ordered breadth-first: the root appears first, then all its children, then all grandchildren, and so on. This guarantees that a node's parent always appears earlier in the array than the node itself.

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

### Node Fields

**`local_id`** — A monotonic integer assigned during export, starting at 0 for the root. This is *not* the database UUID. Local IDs exist so that nodes can reference each other within the export file without exposing or depending on database-specific UUIDs. This makes the format portable across database instances.

**`name`** — The block's name (the last segment of its namespace path). Combined with the parent chain within the export, you can reconstruct the full path.

**`content_type`** — The atom's content type: `"content"` (default), `"raw_text"`, `"schema"`, `"setting"`, `"type_registry"`, `"todo"`, or any user-defined type name like `"person"`.

**`content_template`** — The atom's content with `{N}` placeholders where links appear. This is the same template format used in the database (see [Content Storage](./content-storage.md)).

**`internal_links`** — Links whose targets are *within* this export. Each entry maps a placeholder index to a local ID:
```json
{ "placeholder_index": 0, "target_local_id": 4 }
```
This means `{0}` in the template links to the node with `local_id: 4` in this same export file.

**`external_links`** — Links whose targets are *outside* this export. Since the target isn't included in the file, it's identified by its namespace path in the source database:
```json
{ "placeholder_index": 1, "target_path": "ingredients::pantry::olive-oil" }
```
During import, the system will try to resolve this path in the target database. If the path exists there, the link is connected. If not, the link becomes unresolved.

**`properties`** — The atom's JSONB properties, passed through as-is. This carries typed field values (for custom type instances), schema definitions, settings, and any other metadata.

**`export_hash`** — A SHA-256 digest used for deduplication during import. More on this below.

**`parent_local_id`** — The `local_id` of this node's parent within the export, or `null` for the root node. This reconstructs the tree hierarchy without database UUIDs.

**`position`** — The fractional index string that determines ordering among siblings. Preserved during export so that import recreates the same sibling order.

**`children_local_ids`** — An ordered list of this node's children's `local_id` values. This is redundant with `parent_local_id` (you could reconstruct it) but makes the tree structure immediately visible when reading the JSON.

### Link Splitting: Internal vs. External

In the database, the atom's `links` array is a flat list of lineage UUIDs. The export process classifies each link:

- If the target lineage belongs to a block within the exported subtree, it becomes an **internal link** with a `target_local_id`.
- If the target lineage is outside the subtree, it becomes an **external link** with a `target_path` (the resolved namespace path in the source database).

This split is important because internal links can be reliably reconnected during import (the target is right there in the same file), while external links are best-effort (the target path might not exist in the destination database).

## Edges

Edges in the export represent semantic relationships (not inline links) between nodes.

```json
{
  "from_local_id": 8,
  "to_local_id": 5,
  "edge_type": "inspired-by",
  "properties": {}
}
```

**Only edges where both endpoints are within the exported subtree are included.** If an edge connects a node inside the subtree to one outside it, that edge is silently omitted. This keeps the export self-contained — you won't have dangling edge references.

Edge fields use `local_id` values, not database UUIDs, following the same portability principle as node references.

## The Export Hash

Each node carries an `export_hash` — a SHA-256 digest that enables content-addressed deduplication across database instances.

The hash is computed as:

```
SHA256(content_type + "\x00" + content_template + "\x00" + sorted_internal_local_ids_as_u32_le_bytes)
```

There are two critical design choices here:

**Local IDs, not UUIDs.** The hash uses the integer `local_id` values assigned during export, not the database UUIDs. This means the same content exported from two different databases (where the UUIDs differ) will produce the same hash. The hash is about *what the content is*, not *where it came from*.

**Stability across databases.** Because the hash depends only on content and structural relationships (via local IDs), it serves as a universal fingerprint. Two independent exports of equivalent content will yield matching hashes, enabling the merge import mode to detect duplicates.

During import, the hash is stored in the created atom's `properties` under the key `_import_hash`. This allows subsequent imports to find existing content by hash rather than re-importing it.

## Import Modes

The export format supports two import modes that determine how content is handled when it might already exist in the target database.

### Merge Mode (Default)

Merge mode is designed for the common case: importing content that might partially overlap with what's already in the database. For each node in the export:

1. Compute the export hash from the node's content.
2. Search for an existing atom whose `properties->>'_import_hash'` matches.
3. **If found:** Reuse the existing lineage. No new atom or lineage is created. The block may still be created if it doesn't exist at the target location.
4. **If not found:** Create a new lineage and atom, storing the hash as `_import_hash` in properties.

External links are resolved against the target database by namespace path. If the path `ingredients::pantry::olive-oil` exists in the target database, the link is connected to that lineage. If it doesn't exist, the link slot gets the nil UUID (`00000000-0000-0000-0000-000000000000`), which the UI renders as an unresolved link.

Merge mode is idempotent for content: importing the same file twice won't create duplicates. It's safe to use for syncing content between databases or re-importing updated exports.

### Copy Mode

Copy mode creates everything fresh, ignoring any existing content:

- All nodes get new UUIDs regardless of whether identical content exists.
- External links always become nil UUIDs (no resolution attempted).
- No `_import_hash` is stored.

Use copy mode when you want an independent duplicate of a subtree — for example, forking a template to create a new project from a standard structure. The copied content has no link back to the original.

### Import Results

After import, the system reports what happened:

```json
{
  "created": 5,
  "merged": 2,
  "skipped": 0
}
```

- **`created`** — New lineages and atoms created.
- **`merged`** — Nodes that matched existing content and were reused.
- **`skipped`** — Nodes that couldn't be imported (errors).

## Using Export and Import

### CLI

```bash
# Export a subtree by namespace path
yap export research::ml --output ml-export.json

# Export by block ID
yap export <block-id> --output subtree.json

# Import under a parent (merge mode, default)
yap import ml-export.json --parent projects

# Import as an independent copy
yap import ml-export.json --parent projects --mode copy
```

### HTTP API

```bash
# Export
curl http://localhost:3000/api/blocks/<id>/export > subtree.json

# Import
curl -X POST http://localhost:3000/api/blocks/<parent-id>/import \
  -H "Content-Type: application/json" \
  -d @subtree.json
```

## Design Tradeoffs

A few deliberate tradeoffs are worth understanding:

**Cross-subtree edges are dropped.** If node A inside the subtree has an edge to node B outside it, that edge is not exported. The alternative would be to export it as an "external edge" with a path reference (similar to external links), but edges are less common than content links and the complexity wasn't justified. If you need the full edge graph, export a larger subtree that includes both endpoints.

**External links are best-effort.** The `target_path` in an external link is the path *at export time*. If the target block has moved in the source database since the export, the path is stale. And if the target doesn't exist in the destination database at all, the link becomes unresolved. This is an acceptable tradeoff: external links are inherently fragile because the export can't control what exists in the destination.

**BFS ordering is required, not optional.** Importers depend on parents appearing before children in the node array. A randomly-ordered node list would require a topological sort during import. BFS order makes the importer simpler and the format easier to read by hand (the tree structure is apparent from scanning top to bottom).

**Local IDs are dense integers starting at 0.** They're not random or sparse. This makes the export compact, the hash computation straightforward, and the format easy to debug by reading the JSON directly.
