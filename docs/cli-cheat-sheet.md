## CLI Cheat Sheet

### Blocks

```bash
# Create a block
yap block create "My note content" --namespace research::ml --name attention

# Create with content from stdin
echo "Long content here" | yap block create - --namespace notes --name idea

# Create with a content type and structured properties
yap block create "Alice Johnson" --namespace people --name alice \
  --type person --prop '{"email":"alice@example.com","role":"engineer"}'

# Get a block (shows rendered content)
yap block get <block-id>

# List blocks under a namespace
yap block list --namespace research

# Search by name or path
yap block list --search attention

# List by content type
yap block list --content-type person

# Update content
yap block update <block-id> --content "Updated text"

# Update properties
yap block update <block-id> --prop '{"status":"done"}'

# Rename
yap block update <block-id> --name new-name

# Move to a different parent
yap block move <block-id> --parent <parent-block-id>

# Move to root
yap block move <block-id> --parent root

# Soft delete
yap block delete <block-id>

# Restore deleted block
yap block restore <block-id>

# List orphaned blocks (parent was deleted)
yap block list --orphans

# Render a namespace as a markdown document
yap block tree research::ml
yap block tree research::ml --depth 2
```

### Namespaces

```bash
# Create namespace (creates parent namespaces automatically)
yap ns create research::ml::transformers

# List all namespaces
yap ns list

# Show tree view
yap ns tree research
yap ns tree              # All namespaces
```

### Atoms (content objects)

```bash
# Get rendered atom content
yap atom get <lineage-id>

# Get raw storage format (template + links array)
yap atom get <lineage-id> --raw

# Show what links to this atom
yap atom backlinks <lineage-id>

# Show graph neighborhood (links + edges)
yap atom graph <lineage-id>
```

### Edges (semantic relationships)

```bash
# Create a semantic edge between two atoms
yap edge create <from-lineage-id> <to-lineage-id> references
yap edge create <from-lineage-id> <to-lineage-id> inspired-by

# List edges for an atom
yap edge list <lineage-id>

# Delete an edge
yap edge delete <edge-id>
```

### Links

```bash
# Resolve a wiki-link path to an atom ID
yap link resolve research::ml::attention

# Resolve a relative link from a given namespace
yap link resolve "./sibling" --from research::ml
```

### Search

```bash
# Search blocks by name or namespace path
yap search attention
yap search "ml::"
```

### Export / Import

```bash
# Export a subtree to JSON
yap export research::ml --output ml-backup.json
yap export research::ml > ml-backup.json

# Import a subtree under a parent
yap import ml-backup.json --parent projects
yap import ml-backup.json --parent projects --mode copy   # Fresh UUIDs
yap import ml-backup.json --parent projects --mode merge  # Deduplicate (default)
```

### Database

```bash
yap db migrate   # Run pending migrations
yap db status    # Show migration status
yap db reset     # Drop and recreate (with confirmation prompt)
```

## Custom Types (Schemas)

Define structured data types (Person, Project, Task, etc.) and attach them to blocks.

### Define a schema

```bash
# Create a schema under types::<name>
yap schema create person --fields '[
  {"name":"email","type":"string"},
  {"name":"role","type":"enum","options":["engineer","manager","designer"]},
  {"name":"team","type":"string"}
]'

yap schema list                        # Show all schemas
yap schema get person                  # Show field definitions
yap schema resolve person              # Resolve with namespace walk-up
yap schema resolve person --from projects::backend   # Local override lookup
```

Schemas are stored as regular blocks under `types::<name>` with `content_type = "schema"`. You can also define local schemas at `<namespace>::types::<name>` that shadow the global one.

### Create typed entries

```bash
# Via CLI
yap block create "Alice Johnson" \
  --namespace people \
  --name alice \
  --type person \
  --prop '{"email":"alice@example.com","role":"engineer","team":"backend"}'

# List by type
yap block list --content-type person
```
