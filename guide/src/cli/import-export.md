# Import and Export

yap-orange supports exporting block subtrees to a portable JSON format and importing them back. This is useful for backups, migrating data between instances, sharing content, and seeding new databases.

## Export

Export a subtree rooted at a namespace path or block ID:

```bash
# Export by namespace path (writes to file)
yap export research::ml --output ml-backup.json

# Export by block ID
yap export 019414a0-b1c2-7def-8000-000000000001 --output backup.json

# Export to stdout (no --output flag)
yap export research::ml
```

The `target` argument accepts either a namespace path (resolved via link resolution) or a UUID.

### Output format

The export file is a JSON object containing:
- `nodes` -- an array of block/atom snapshots with content, properties, and link placeholders
- `edges` -- an array of semantic edges within the subtree
- `source_namespace` -- the original namespace path of the exported root

When writing to a file, the CLI prints a summary:

```
Exported to ml-backup.json
  12 nodes, 3 edges
  Source: research::ml
```

### Filtering properties

Use `--include-keys` to export only specific property keys (comma-separated):

```bash
yap export research::ml --output ml.json --include-keys "email,role"
```

By default, all non-underscore-prefixed property keys are included.

## Import

Import a previously exported subtree under a parent block:

```bash
yap import ml-backup.json --parent projects
```

The `--parent` flag accepts a namespace path or block UUID specifying where to graft the imported tree.

### Import modes

The `--mode` flag controls how the import handles blocks that already exist:

```bash
# Merge mode (default): deduplicates by content hash
yap import data.json --parent projects --mode merge

# Copy mode: always creates fresh UUIDs, no deduplication
yap import data.json --parent projects --mode copy
```

**Merge mode** compares imported nodes against existing blocks. If a match is found (same content hash), the import skips creating a duplicate and reuses the existing block. This is the default and is safe for repeated imports.

**Copy mode** ignores any existing content and creates new blocks with fresh UUIDs for every node in the import file.

### Match strategy

In merge mode, `--match-by` controls how existing blocks are matched:

```bash
yap import data.json --parent projects --mode merge --match-by content_identity
```

Available strategies:
- `export_hash` -- match by the export hash stored in node metadata
- `content_identity` -- match by content hash and name
- `merkle` -- match using the Merkle tree hash
- `topology` -- match by structural position in the tree

### Global linking

Use `--global-link` to search the entire database (not just the target parent) for matching content to hard-link against:

```bash
yap import data.json --parent projects --mode merge --global-link
```

This is useful when importing content that may already exist elsewhere in your hierarchy.

### Import output

The CLI prints a summary after import:

```
Import complete.
  Created: 8
  Skipped: 4
  Root block: 019414a0-ffff-7def-8000-000000000010
  Edges created: 3
```

Additional diagnostics are shown if there were issues:
- **Linked** -- blocks that were hard-linked to existing content (with `--global-link`)
- **Unresolved external links** -- wiki-links pointing to paths outside the imported subtree that could not be resolved
- **Failed edges** -- edges that could not be created (e.g., target not found)

## Practical Examples

### Backup and restore

```bash
# Backup everything under "research"
yap export research --output research-backup.json

# Restore into a new instance (after db migrate)
yap import research-backup.json --parent ""
```

### Migrate between instances

```bash
# Export from instance A
YAP_SERVER_URL=http://server-a:3000 yap export projects --output projects.json

# Import into instance B
YAP_SERVER_URL=http://server-b:3000 yap import projects.json --parent ""
```

### Seed sample data

```bash
# Import a fixture file under a test namespace
yap import fixtures/sample-data.json --parent test-data --mode copy
```

### Idempotent sync

```bash
# Safe to run repeatedly -- merge mode skips duplicates
yap import shared-notes.json --parent team --mode merge
```
