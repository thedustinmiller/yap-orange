# HTTP API Reference
Last updated 2026-03-22

This document maps the API surface and analogies between clients.


## Client Parity

| Endpoint | CLI Command | Web (`api.ts`) | Status |
|---|---|---|---|
| `GET /health` | `yap health` | `api.health()` | All |
| `GET /api/atoms/:id` | `yap atom get <id> --raw` | `atoms.get(id)` | All |
| `GET /api/atoms/:id/rendered` | `yap atom get <id>` | `atoms.getRendered(id)` | All |
| `PUT /api/atoms/:id` | `yap block update <id> --content` | `atoms.update(id, data)` | All |
| `GET /api/atoms/:id/backlinks` | `yap atom backlinks <id>` | `atoms.backlinks(id)` | All |
| `GET /api/atoms/:id/references` | `yap atom references <id>` | `atoms.references(id)` | All |
| `GET /api/atoms/:id/graph` | `yap atom graph <id>` | `atoms.graph(id)` | All |
| `GET /api/atoms/:id/edges` | `yap edge list <id>` | — | CLI only |
| `GET /api/atoms/snapshot/:atom_id` | — | `atoms.snapshot(id)` | Web + API |
| `POST /api/blocks` | `yap block create` | `blocks.create(data)` | All |
| `GET /api/blocks` | `yap block list` | `blocks.list(params)` | All |
| `GET /api/blocks/:id` | `yap block get <id>` | `blocks.get(id)` | All |
| `PUT /api/blocks/:id` | `yap block update <id>` | `blocks.update(id, data)` | All |
| `DELETE /api/blocks/:id` | `yap block delete <id>` | `blocks.delete(id)` | All |
| `DELETE /api/blocks/:id/recursive` | `yap block delete <id> --recursive` | `blocks.deleteRecursive(id)` | All |
| `GET /api/blocks/:id/children` | — | `blocks.children(id)` | Web + API |
| `GET /api/blocks/:id/property-keys` | `yap block property-keys <id>` | `importExport.propertyKeys(id)` | All |
| `POST /api/blocks/:id/restore` | `yap block restore <id>` | `blocks.restore(id)` | All |
| `POST /api/blocks/:id/restore-recursive` | `yap block restore <id> --recursive` | — | CLI + API |
| `POST /api/blocks/:id/move` | `yap block move <id>` | `blocks.move(id, data)` | All |
| `GET /api/blocks/roots` | `yap ns list` | `roots.list()` | All |
| `GET /api/blocks/orphans` | `yap block list --orphans` | `blocks.orphans()` | All |
| `GET /api/blocks/:id/export` | `yap export <target>` | `importExport.export(id)` | All |
| `POST /api/blocks/:id/import` | `yap import <file>` | `importExport.import(id, tree)` | All |
| `POST /api/import` | `yap import <file> --root` | `importExport.importAtRoot(tree)` | All |
| `POST /api/edges` | `yap edge create` | `edges.create(data)` | All |
| `DELETE /api/edges/:id` | `yap edge delete <id>` | `edges.delete(id)` | All |
| `GET /api/schemas` | `yap schema list` | `schemas.list()` | All |
| `POST /api/schemas/resolve` | `yap schema resolve <name>` | `schemas.resolve(name)` | All |
| `POST /api/graph/subtree` | `yap graph subtree <ids...>` | `graph.subtree(ids)` | All |
| `POST /api/resolve` | `yap link resolve <path>` | `resolve(path)` | All |
| `GET /api/debug/logs` | `yap debug logs` | `debug.logs(since)` | All |
| `POST /api/debug/benchmarks` | `yap debug benchmarks` | — | CLI + API |


---

## Health

### `GET /health`

```json
{"status": "ok", "database": "connected"}
```

---

## Atoms

Atoms are content objects. Use the lineage ID (not the atom snapshot ID) in all requests.

### `GET /api/atoms/:lineage_id`

Returns the raw atom (template + links array).

**Response**
```json
{
  "id": "<atom-snapshot-uuid>",
  "lineage_id": "<lineage-uuid>",
  "content_type": "content",
  "content_template": "See {0} for details.",
  "links": ["<lineage-uuid-1>"],
  "properties": {},
  "content_hash": "abc123...",
  "predecessor_id": null,
  "created_at": "2026-03-04T10:00:00Z"
}
```

### `GET /api/atoms/:lineage_id/rendered`

Returns the atom with wiki links resolved from `{N}` placeholders back to `[[path]]` syntax.

**Response**
```json
{
  "id": "<atom-snapshot-uuid>",
  "lineage_id": "<lineage-uuid>",
  "content_type": "content",
  "content": "See [[research::ml::attention]] for details.",
  "properties": {},
  "created_at": "2026-03-04T10:00:00Z"
}
```

### `PUT /api/atoms/:lineage_id`

Update atom content. Creates a new immutable atom snapshot; the lineage pointer advances.

**Request**
```json
{
  "content": "Updated content with [[new::link]]",
  "content_type": "content",
  "properties": {}
}
```

**Response** — same shape as `GET /api/atoms/:id`

### `GET /api/atoms/:lineage_id/backlinks`

Returns all lineages whose content template links to this lineage.

**Response**
```json
[
  {
    "lineage_id": "<uuid>",
    "content": "See [[research::ml::attention]] here.",
    "content_type": "content",
    "namespace": "projects::notes::overview"
  }
]
```

### `GET /api/atoms/:lineage_id/references`

Returns semantic edges pointing to this lineage (from the `edges` table).

**Response** — array of `EdgeResponse` (same as edge endpoints)

### `GET /api/atoms/:lineage_id/graph`

Returns the full graph neighborhood: the atom, its backlinks, outlinks, and edges.

**Response**
```json
{
  "atom": { ... },
  "backlinks": [ ... ],
  "outlinks": [ ... ],
  "edges": {
    "outgoing": [ ... ],
    "incoming": [ ... ]
  }
}
```

### `GET /api/atoms/:lineage_id/edges`

Returns all edges where this lineage is either the source or target.

### `GET /api/atoms/snapshot/:atom_id`

Returns a specific atom snapshot by its atom ID (not lineage ID). Used to retrieve pinned schema versions via `_schema_atom_id` stored in entry properties.

**Response**
```json
{
  "id": "<atom-uuid>",
  "content_type": "schema",
  "content_template": "",
  "links": [],
  "properties": {"fields": [...]},
  "content_hash": "abc123...",
  "predecessor_id": null,
  "created_at": "2026-03-04T10:00:00Z"
}
```

---

## Blocks

Blocks place lineages in the hierarchy. A block has a name and a parent, which together define its namespace path.

### `POST /api/blocks`

Create a block (and its content lineage/atom). Creates parent namespace blocks automatically.

**Request**
```json
{
  "namespace": "research::ml",
  "name": "attention",
  "content": "The attention mechanism...",
  "content_type": "content",
  "properties": {}
}
```

The `namespace` is the **parent** path. The block will be created at `research::ml::attention`.

**Response**
```json
{
  "block_id": "<uuid>",
  "lineage_id": "<uuid>",
  "namespace": "research::ml::attention",
  "name": "attention"
}
```

### `GET /api/blocks?namespace=X&search=Y&lineage_id=Z&content_type=T`

List blocks. All query parameters are optional and can be combined.

- `namespace` — prefix match on computed namespace path
- `search` — text search on block name and namespace
- `lineage_id` — find all blocks referencing a specific lineage
- `content_type` — filter by atom content type (e.g. `person`, `schema`, `todo`)

**Response** — array of `BlockResponse` objects

### `GET /api/blocks/orphans`

List blocks whose parent has been soft-deleted (parent_id points to a deleted block).

### `GET /api/blocks/:id`

Get a single block with its rendered content.

**Response**
```json
{
  "id": "<uuid>",
  "lineage_id": "<uuid>",
  "parent_id": "<uuid>",
  "namespace": "research::ml::attention",
  "name": "attention",
  "position": "80",
  "content": "The attention mechanism...",
  "content_type": "content",
  "properties": {},
  "created_at": "2026-03-04T10:00:00Z"
}
```

### `PUT /api/blocks/:id`

Update block metadata (name and/or position). Does not update content.

**Request**
```json
{
  "name": "new-name",
  "position": "8180"
}
```

### `DELETE /api/blocks/:id`

Soft-delete a block. Children become orphans (their `parent_id` still points to the deleted block).

### `DELETE /api/blocks/:id/recursive`

Recursively soft-deletes a block and all of its descendants.

Returns `204 No Content` on success.

### `GET /api/blocks/:id/children`

Get direct children of a block.

### `GET /api/blocks/:id/property-keys`

Returns all distinct property keys used anywhere in the subtree rooted at this block (across all descendant atoms).

**Response**
```json
["priority", "status", "due_date"]
```

**Response** — array of `BlockResponse`

### `POST /api/blocks/:id/restore`

Restore a soft-deleted block.

### `POST /api/blocks/:id/restore-recursive`

Recursively restores a soft-deleted block and all of its descendants.

**Response**
```json
{"restored": 5}
```

The `restored` value is the count of blocks that were restored.

### `POST /api/blocks/:id/move`

Move a block to a new parent (or to root).

**Request**
```json
{
  "parent_id": "<uuid>",
  "position": "8080"
}
```

Use `"parent_id": null` to move to root.

### `GET /api/blocks/:id/export`

Export the block subtree to portable JSON format.

**Response** — `ExportTree` object (see [export-format.md](./export-format.md))

### `POST /api/blocks/:id/import`

Import a subtree under this block.

**Request** — `ExportTree` object with an optional `mode` field:
```json
{
  "format": "yap-tree-v1",
  "mode": "merge",
  ...
}
```

**Response**
```json
{
  "created": 5,
  "merged": 2,
  "skipped": 0
}
```

### `POST /api/import`

Import a subtree at root level (no parent block).

**Request** — same as `POST /api/blocks/:id/import`

**Response**
```json
{
  "created": 5,
  "merged": 2,
  "skipped": 0
}
```

---

## Schemas (Custom Types)

Schemas define structured data types. They are stored as blocks under `types::<name>` with `content_type = "schema"` and field definitions in `properties.fields`.

### `GET /api/schemas`

List all schema definitions.

**Response**
```json
[
  {
    "block_id": "<uuid>",
    "lineage_id": "<uuid>",
    "namespace": "types::person",
    "name": "person",
    "version": 1,
    "fields": [
      {"name": "email", "type": "string"},
      {"name": "role", "type": "enum", "options": ["engineer", "manager"]}
    ],
    "content": "Schema: person"
  }
]
```

### `POST /api/schemas/resolve`

Resolve a type name to its schema, using namespace walk-up. Given a type name and an optional context namespace, searches:

1. `<context>::types::<name>`
2. `<parent>::types::<name>` (walking up)
3. `types::<name>` (global fallback)

Returns the first match.

**Request**
```json
{
  "type_name": "person",
  "from_namespace": "projects::backend"
}
```

**Response** — same shape as a single item from `GET /api/schemas`

Returns `404` if no schema is found.

---

## Edges

### `POST /api/edges`

Create a semantic edge between two lineages.

**Request**
```json
{
  "from_lineage_id": "<uuid>",
  "to_lineage_id": "<uuid>",
  "edge_type": "references",
  "properties": {}
}
```

Returns `409 Conflict` if an edge of the same type already exists between these lineages.

**Response**
```json
{
  "id": "<uuid>",
  "from_lineage_id": "<uuid>",
  "to_lineage_id": "<uuid>",
  "edge_type": "references",
  "properties": {},
  "created_at": "2026-03-04T10:00:00Z"
}
```

### `DELETE /api/edges/:id`

Soft-delete an edge.

---

## Utility

### `POST /api/graph/subtree`

Returns graph data (content links and semantic edges) for a given set of lineage IDs. Used by the graph panel to render connections within a visible subgraph.

**Request**
```json
{
  "lineage_ids": ["<uuid-1>", "<uuid-2>", "..."]
}
```

**Response**
```json
{
  "content_links": [
    {"from_lineage_id": "<uuid>", "to_lineage_id": "<uuid>"}
  ],
  "edges": [ ... ]
}
```

Returns `400 Bad Request` if more lineage IDs are provided than the implementation limit allows.

### `GET /api/blocks/roots`

List all root-level blocks (blocks with no parent).

**Response** — array of:
```json
{
  "id": "<block-uuid>",
  "namespace": "research",
  "name": "research",
  "lineage_id": "<uuid>",
  "parent_id": null,
  "position": "80"
}
```

### `GET /api/debug/logs`

Returns recent server log entries from the tracing ring buffer.

**Query parameters:**
- `since` (optional) — only return entries with id > this value (for polling)

**Response** — array of:
```json
{
  "id": 42,
  "timestamp": "2026-03-14T16:51:32.334Z",
  "level": "DEBUG",
  "target": "tower_http::trace::on_response",
  "message": "finished processing request latency=7 ms status=200"
}
```

### `POST /api/debug/benchmarks` *(feature-gated: `bench`)*

Runs performance benchmarks against the current Store backend. Only available when the server is compiled with the `bench` feature flag.

**Request** — `BenchmarkConfig` (see `yap-bench` crate)

**Response** — `BenchmarkResults` with timing data per operation.

### `POST /api/resolve`

Resolve a wiki-link path to a lineage and block.

**Request**
```json
{
  "path": "research::ml::attention",
  "from_namespace": "projects::notes"
}
```

`from_namespace` is required for relative paths (`./`, `../`).

**Response**
```json
{
  "lineage_id": "<uuid>",
  "block_id": "<uuid>",
  "namespace": "research::ml::attention"
}
```

---

## Error Responses

| Status | When |
|--------|------|
| `400 Bad Request` | Invalid JSON, invalid UUID, invalid input |
| `404 Not Found` | Resource doesn't exist or is soft-deleted |
| `409 Conflict` | Duplicate edge, duplicate block name in namespace |
| `500 Internal Server Error` | Database error |

All errors return:
```json
{"error": "Human-readable message"}
```
