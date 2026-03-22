# Search, Links, Atoms, and Edges

This chapter covers the commands for searching blocks, resolving wiki-link paths, inspecting atoms, and managing semantic edges.

## Search

Search blocks by name or namespace path:

```bash
yap search "transformers"
```

Results show each matching block's namespace, type indicator, and a content preview:

```
research::ml::transformers  Attention is all you need - foundational pap...
notes::reading::transformers-survey  Survey of transformer architectur...

2 result(s)
```

This is equivalent to `yap block list --search "transformers"`.

## Link Resolution

Resolve a wiki-link path to its lineage and block IDs:

```bash
yap link resolve "research::ml::transformers"
```

Output:

```
Lineage: 019414a0-aaaa-7def-8000-000000000001
Block:   019414a0-bbbb-7def-8000-000000000002
Namespace: research::ml::transformers
```

### Relative paths

Use `--from` to resolve relative links from a context namespace:

```bash
# Resolve a sibling link (../) from within research::ml
yap link resolve "../biology" --from research::ml

# Resolve a child link (./) from within research
yap link resolve "./ml::transformers" --from research
```

This is the same resolution logic that the editor uses when you write `[[../biology]]` inside a block in the `research::ml` namespace.

## Atom Commands

Atoms are immutable content snapshots. Every block points (via its lineage) to a current atom. The `yap atom` commands let you inspect atoms directly.

### Get an atom

View the rendered content of an atom (with links resolved to paths):

```bash
yap atom get 019414a0-aaaa-7def-8000-000000000001
```

Output:

```
ID: 019414a0-aaaa-7def-8000-000000000001
Type: text
Content:
  This references [[research::ml::transformers]]
```

### Raw atom view

Use `--raw` to see the underlying template with placeholder indices and the links array:

```bash
yap atom get 019414a0-aaaa-7def-8000-000000000001 --raw
```

Output:

```
ID: 019414a0-aaaa-7def-8000-000000000001
Type: text
Template:
  This references [[{0}]]
Links:
  0: 019414a0-cccc-7def-8000-000000000003
```

The template uses `{0}`, `{1}`, etc. as placeholders that index into the `links` array. Each link entry is a lineage UUID. This is how yap-orange maintains link integrity -- links point to stable lineage IDs, not paths.

### Backlinks

See which atoms link to a given lineage:

```bash
yap atom backlinks 019414a0-aaaa-7def-8000-000000000001
```

Output:

```
Atoms linking to this:
  notes::daily::2025-01-15  Reviewed [[research::ml::transformers]] t...
  projects::paper  See [[research::ml::transformers]] for backgrou...

2 backlink(s)
```

### Graph neighborhood

View the full neighborhood of an atom -- its content, outlinks, backlinks, and edges in both directions:

```bash
yap atom graph 019414a0-aaaa-7def-8000-000000000001
```

Output:

```
Atom:
  ID: 019414a0-aaaa-7def-8000-000000000001
  Type: text
  Content: Attention is all you need - foundational paper on...

Outlinks (2):
  -> research::ml::attention  Self-attention mechanism described in...
  -> research::ml::positional-encoding  Sinusoidal encoding scheme...

Backlinks (1):
  <- notes::daily::2025-01-15  Reviewed transformers paper today...

Edges (1 out, 0 in):
  --related_to-> 019414a0-dddd-7def-8000-000000000004
```

## Edge Commands

Edges are non-hierarchical semantic relationships between lineages. Unlike wiki-links (which are embedded in content), edges are first-class records with a type label and optional properties.

### Create an edge

```bash
yap edge create \
  019414a0-aaaa-7def-8000-000000000001 \
  019414a0-bbbb-7def-8000-000000000002 \
  "related_to"
```

Arguments are positional: `<from-lineage-id> <to-lineage-id> <edge-type>`.

Output:

```
Created edge: 019414a0-eeee-7def-8000-000000000005
019414a0-aaaa-... --related_to-> 019414a0-bbbb-...
```

### List edges

List all edges (both incoming and outgoing) for a lineage:

```bash
yap edge list 019414a0-aaaa-7def-8000-000000000001
```

Output:

```
Outgoing:
  related_to -> 019414a0-bbbb-... (019414a0-eeee-...)

Incoming:
  (none)
```

The edge ID is shown in parentheses after each entry.

### Delete an edge

Delete an edge by its edge ID:

```bash
yap edge delete 019414a0-eeee-7def-8000-000000000005
```
