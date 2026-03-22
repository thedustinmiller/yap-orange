# Block Operations

Blocks are the primary unit of content in yap-orange. Each block has a name, lives in a namespace (determined by its parent chain), and points to an atom that holds its content. The `yap block` subcommand handles all block CRUD operations.

## Create a Block

```bash
yap block create "Some content here" --namespace research --name "my note"
```

Required flags:
- `--namespace` -- the parent namespace path (use empty string `""` for root)
- `--name` -- the block's name within that namespace

Optional flags:
- `--type` -- content type (default: `text`)
- `--prop` -- properties as a JSON object string

The first positional argument is the content. Omit it or pass `-` to read from stdin:

```bash
# Read content from stdin
echo "Content from a pipe" | yap block create - --namespace notes --name piped

# Pipe detection (no content argument, stdin is not a TTY)
cat article.md | yap block create --namespace docs --name article
```

### Typed blocks with properties

Create a block with a custom content type and structured properties:

```bash
yap block create "Alice Smith" \
  --namespace people \
  --name alice \
  --type person \
  --prop '{"email": "alice@example.com", "role": "engineer"}'
```

### Output

Human-readable output shows the new block ID, lineage ID, and namespace. Use `--json` for the full response:

```bash
yap --json block create "hello" --namespace notes --name greeting
```

```json
{
  "block_id": "019414a0-...",
  "lineage_id": "019414a0-...",
  "namespace": "notes",
  "name": "greeting"
}
```

## Get a Block

Fetch a single block by its block ID:

```bash
yap block get 019414a0-b1c2-7def-8000-000000000001
```

The output includes the block ID, lineage ID, namespace, name, content type, position, parent ID (if any), and the rendered content.

## List Blocks

List blocks with various filters:

```bash
# All blocks in a namespace
yap block list --namespace research

# Search by name or path
yap block list --search "transformers"

# Filter by content type
yap block list --content-type person

# Show orphaned blocks (no parent, not a root)
yap block list --orphans
```

You can also use the top-level `search` shortcut:

```bash
yap search "transformers"
```

The list output shows each block's namespace path, a type indicator, and a truncated content preview. The total count is printed at the end.

## Update a Block

Update a block's content, name, position, or properties:

```bash
# Update content
yap block update 019414a0-... --content "Updated content"

# Update name
yap block update 019414a0-... --name "new-name"

# Update properties
yap block update 019414a0-... --prop '{"email": "newemail@example.com"}'

# Update multiple fields at once
yap block update 019414a0-... --content "New text" --name "renamed" --prop '{"key": "val"}'

# Read new content from stdin
echo "piped update" | yap block update 019414a0-... --content -
```

Content and property updates modify the atom (creating a new immutable snapshot). Name and position updates modify the block metadata directly.

## Move a Block

Move a block to a different parent, changing its namespace:

```bash
# Move under another block
yap block move 019414a0-... --parent 019414a0-bbbb-...

# Move to root level (no parent)
yap block move 019414a0-... --parent root

# Move and set position within the new parent
yap block move 019414a0-... --parent 019414a0-bbbb-... --position 8000
```

The `--parent` flag accepts a block UUID or the literal string `root` to make it a top-level block.

## Delete and Restore

Blocks use soft deletion. Deleted blocks are hidden from normal queries but can be restored:

```bash
# Soft delete
yap block delete 019414a0-...

# Restore a single block
yap block restore 019414a0-...

# Restore a block and all its deleted descendants
yap block restore 019414a0-... --recursive
```

## Tree Rendering

Render a namespace subtree as a markdown document with headers:

```bash
# Render the research namespace as markdown
yap block tree research

# Limit depth
yap block tree research --depth 2
```

This outputs the block hierarchy using markdown heading levels (`#`, `##`, `###`, etc.), with each block's content below its heading. This is useful for exporting a readable document from your notes.
