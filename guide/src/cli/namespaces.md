# Namespace Operations

In yap-orange, namespaces are just blocks. A namespace path like `research::ml::transformers` is computed dynamically by walking the `parent_id` chain -- there is no separate namespace table. The `yap ns` subcommand provides convenience operations for creating and viewing the namespace hierarchy.

## Create a Namespace

```bash
yap ns create research::ml::transformers
```

This creates a block named `transformers` under the `research::ml` parent. If the parent path does not exist yet, the server creates intermediate blocks automatically.

Creating a root-level namespace:

```bash
yap ns create projects
```

The output shows the created namespace path and the new block ID:

```
Created namespace: research::ml::transformers
Block ID: 019414a0-b1c2-7def-8000-000000000001
```

## List Namespaces

List all root-level blocks (top-level namespaces):

```bash
yap ns list
```

Output:

```
research
projects
notes

3 namespace(s)
```

This returns blocks with no parent (`parent_id IS NULL`), which are the roots of the hierarchy.

## Tree View

Display the full namespace hierarchy as a tree:

```bash
# Show entire tree
yap ns tree

# Show tree rooted at a specific namespace
yap ns tree research
```

Output:

```
├── ml
│   ├── transformers (note)
│   ├── diffusion (note)
│   └── rl
│       └── q-learning (note)
└── biology
    └── genomics (note)
```

The tree uses box-drawing characters to show the hierarchy. Each node shows its name and a type indicator (`(note)` for blocks with content, `(task)` for task-type blocks, no indicator for empty container blocks).

## Namespaces vs. Blocks

Since namespaces are blocks, all block operations also work on namespaces:

```bash
# Get details about a namespace block
yap block get 019414a0-...

# Move a namespace (and all its children) under a different parent
yap block move 019414a0-... --parent 019414a0-bbbb-...

# Delete a namespace
yap block delete 019414a0-...

# List children of a namespace
yap block list --namespace research::ml
```

The key insight is that `yap ns create` is a convenience wrapper. Under the hood it calls the same block creation endpoint, with an empty content body and the parent namespace derived from the path.
