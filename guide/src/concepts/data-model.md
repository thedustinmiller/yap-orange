# The Four-Table Data Model

yap-orange stores all notes in just four database tables. This might seem surprising — most note-taking tools use a single "notes" table with columns for title, body, folder, and so on. yap-orange splits these concerns into separate tables, each with a single responsibility. The payoff is a system where links never break, edits never destroy history, and content can appear in multiple places without duplication.

The design is inspired by how Unix filesystems work. If you understand inodes and directory entries, the yap-orange model will feel familiar. If you don't, that's fine — we'll build the analogy as we go.

## The Filesystem Analogy

In a Unix filesystem:

- **Data blocks** hold the raw bytes of a file. They're immutable chunks on disk.
- **Inodes** are the stable identity of a file. An inode tracks which data blocks belong to it, plus metadata like timestamps. When you edit a file, the inode gets updated to point at new data blocks — but the inode number itself never changes.
- **Directory entries** map a human-readable name (like `notes.txt`) to an inode number. The same inode can appear under different names in different directories (hard links).
- **Symlinks** are named pointers from one location to another.

yap-orange maps this directly:

| Filesystem | yap-orange | Role |
|------------|-----------|------|
| Data blocks | **Atoms** | Immutable content snapshots |
| Inodes | **Lineages** | Stable identity, mutable pointer to current content |
| Directory entries | **Blocks** | Named position in the hierarchy |
| Symlinks | **Edges** | Named relationships between content |

Let's look at each in detail.

## Atoms: Immutable Content Snapshots

An atom is a frozen snapshot of content at a moment in time. When you write a note and hit save, yap-orange creates an atom. When you edit that note and save again, it creates a *new* atom — the original is never touched. This append-only design means you always have full edit history, and concurrent readers never see half-written data.

```sql
CREATE TABLE atoms (
    id              UUID PRIMARY KEY,
    content_type    TEXT NOT NULL DEFAULT '',
    content_template TEXT NOT NULL DEFAULT '',
    links           UUID[] NOT NULL DEFAULT '{}',
    properties      JSONB NOT NULL DEFAULT '{}',
    content_hash    TEXT NOT NULL DEFAULT '',
    predecessor_id  UUID REFERENCES atoms(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Key fields

- **`content_template`** — The note's text, but with wiki links replaced by numbered placeholders like `{0}`, `{1}`. The actual link targets live in the `links` array. (The [Content Storage](./content-storage.md) chapter explains why.)
- **`links`** — An ordered array of lineage UUIDs corresponding to the `{0}`, `{1}`, ... placeholders in the template.
- **`predecessor_id`** — Points to the previous atom in the edit chain. The very first version has `predecessor_id = NULL`. This forms a linked list of versions you can walk backwards through.
- **`content_hash`** — A SHA-256 digest of the content type, template, and sorted link IDs. Two atoms with identical content produce the same hash, enabling deduplication during import.
- **`properties`** — Arbitrary JSON metadata. Used by typed content (schemas, settings) to store structured fields.
- **`content_type`** — Categorizes the atom: plain text, a schema definition, a setting, a typed instance like "person", etc.

### What "immutable" means in practice

Atom rows are **never updated and never deleted**. The table is append-only. This has real consequences:

- **Edit history is free.** Every save creates a new row, so the full revision chain is always available by following `predecessor_id` links backward.
- **Content-addressable dedup.** Because atoms are immutable, their `content_hash` is stable. If you import content that already exists, the system can detect the duplicate and reuse the existing atom.
- **No write conflicts.** Reads never block writes and vice versa. There's nothing to lock because existing rows never change.

The tradeoff is storage growth — every edit adds a row. In practice, text notes are small and UUIDv7 primary keys compress well. For a personal knowledge base, this is a non-issue.

## Lineages: Stable Identity

If atoms are frozen snapshots, how do you refer to "the current version" of a note? That's what lineages are for. A lineage is a mutable pointer that always tracks the latest atom for a piece of content.

```sql
CREATE TABLE lineages (
    id          UUID PRIMARY KEY,
    current_id  UUID NOT NULL REFERENCES atoms(id),
    version     INTEGER NOT NULL DEFAULT 1,
    deleted_at  TIMESTAMPTZ,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Key fields

- **`current_id`** — Points to the atom that represents the "live" content right now. This is the only field that changes on edits.
- **`version`** — Increments with each edit. Useful for conflict detection and display.
- **`deleted_at`** — Soft delete timestamp. When non-NULL, the content is considered deleted but is still recoverable. Soft deletes live on lineages, not on atoms (atoms are never modified).

### The edit cycle

When you edit a note:

1. A new atom is created with `predecessor_id` pointing to the old atom.
2. The lineage's `current_id` is updated to point at the new atom.
3. The lineage's `version` increments.

The lineage ID itself never changes. It equals the UUID of the very first atom created for that content. This is the critical property: **every link in the system targets a lineage ID**, so links remain valid no matter how many edits occur.

### Why lineages matter for links

Consider a link from note A to note B. The link stores B's lineage ID. When B is edited, a new atom is created and B's lineage pointer swings to it — but the lineage ID hasn't changed, so A's link still works. When B is moved to a different location in the hierarchy, the lineage ID still hasn't changed, so A's link still works. This is how yap-orange achieves zero link rot.

## Blocks: The Hierarchy

Atoms hold content, lineages give content stable identity, but neither says anything about *where* content lives in your note hierarchy. That's the job of blocks.

A block is a directory entry: it places a lineage at a specific position in the tree, with a human-readable name and an ordering among siblings.

```sql
CREATE TABLE blocks (
    id          UUID PRIMARY KEY,
    lineage_id  UUID NOT NULL REFERENCES lineages(id),
    parent_id   UUID REFERENCES blocks(id),
    name        TEXT NOT NULL,
    position    TEXT NOT NULL DEFAULT '80',
    deleted_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Key fields

- **`lineage_id`** — Which content this block shows. Multiple blocks can reference the same lineage, meaning the same content appears in multiple places in the tree (like hard links in a filesystem).
- **`parent_id`** — Self-referential foreign key defining the tree structure. A NULL `parent_id` means this is a root block.
- **`name`** — The human-readable name displayed in the outliner. Together with the parent chain, it forms the namespace path.
- **`position`** — A fractional index string used for lexicographic ordering among siblings. This allows inserting a block between any two existing blocks without renumbering all the positions.
- **`deleted_at`** — Soft delete, independent of the lineage's soft delete.

### Namespace paths are computed, not stored

The path `research::ml::attention` is not stored anywhere. It is derived at read time by walking the `parent_id` chain from the block up to the root:

```
block "attention" (parent_id → block "ml")
  → block "ml" (parent_id → block "research")
    → block "research" (parent_id → NULL, root)
```

Join the names with `::` separators and you get `research::ml::attention`.

This means that **moving a block to a new parent instantly changes its path** — and every link that targets its lineage will display the new path on next read. There's no stale path data to update.

### Multiple blocks, one lineage

Because blocks reference lineages rather than containing content directly, you can have the same content appear at multiple places in your tree. Block A under `projects::current` and Block B under `archive::2024` can both point to the same lineage. Editing the content through either block updates the shared lineage, and both locations see the change.

### Ordering with fractional indexes

The `position` field is a string that supports lexicographic comparison. To insert between position `"40"` and `"80"`, you compute a midpoint like `"6080"`. This avoids the classic problem of integer-based ordering where inserting between items 3 and 4 requires renumbering everything after the insertion point.

### Uniqueness constraints

Two important constraints prevent naming collisions:

```sql
-- No two children of the same parent can share a name
CREATE UNIQUE INDEX idx_blocks_unique_parent_name
    ON blocks (parent_id, name) WHERE deleted_at IS NULL;

-- No two root blocks can share a name
CREATE UNIQUE INDEX idx_blocks_unique_root_name
    ON blocks (name) WHERE parent_id IS NULL AND deleted_at IS NULL;
```

Both constraints exclude soft-deleted blocks, so deleting a block frees up its name.

## Edges: Semantic Relationships

Links embedded in content (wiki links like `[[research::ml::attention]]`) capture one kind of relationship: "this note references that note." But what about relationships that aren't inline references? Things like "this note was inspired by that one," "this task depends on that task," or "this concept contradicts that one."

Edges are explicit, typed relationships between lineages that exist outside of any note's content.

```sql
CREATE TABLE edges (
    id              UUID PRIMARY KEY,
    from_lineage_id UUID NOT NULL REFERENCES lineages(id),
    to_lineage_id   UUID NOT NULL REFERENCES lineages(id),
    edge_type       TEXT NOT NULL,
    properties      JSONB NOT NULL DEFAULT '{}',
    deleted_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Key fields

- **`from_lineage_id`** / **`to_lineage_id`** — The two endpoints. Like content links, edges target lineages, not blocks or paths. They survive moves and edits.
- **`edge_type`** — A free-text label: `"references"`, `"inspired-by"`, `"depends-on"`, `"blocks"`, or any string you define.
- **`properties`** — Arbitrary JSON metadata on the relationship (weight, notes, timestamps, etc.).
- **`deleted_at`** — Soft delete.

### Uniqueness

A unique constraint prevents duplicate edges:

```sql
UNIQUE (from_lineage_id, to_lineage_id, edge_type) WHERE deleted_at IS NULL
```

You can have multiple edges between the same pair of lineages as long as they have different types (e.g., A `references` B and A `depends-on` B are both allowed). Soft-deleting an edge frees up the slot for a new edge of the same type.

### Edges vs. inline links

Both edges and inline wiki links connect lineages, but they serve different purposes:

| | Inline links | Edges |
|---|---|---|
| **Where they live** | Inside atom content | Separate table |
| **Created by** | Writing `[[path]]` in the editor | Explicit action (UI, CLI, API) |
| **Typed** | No (all are "references") | Yes (`edge_type` field) |
| **Visible in content** | Yes, rendered as clickable links | No, shown in graph/panel views |
| **Bidirectional query** | Via backlink search on `links` array | Direct query on `from`/`to` columns |

In practice, most connections are inline links. Edges are for when you want to express a relationship that doesn't belong in any particular note's text — organizational metadata, dependency graphs, concept maps, and similar structures.

## How It All Fits Together

Here's a concrete example. You have a note called "attention" under `research::ml`. You write:

```
The attention mechanism was introduced in {0}. See also {1}.
```

In the database, this is:

1. **An atom** with the content template above, `links = [lineage-A, lineage-B]`, and a `predecessor_id` pointing to the previous version (or NULL if this is the first edit).
2. **A lineage** whose `current_id` points to this atom. This lineage has existed since the note was first created; only its pointer has changed.
3. **A block** named `"attention"` with `parent_id` pointing to the `"ml"` block, and `lineage_id` pointing to the lineage from step 2.
4. **An edge** of type `"inspired-by"` from this lineage to some other lineage — representing a relationship that isn't part of the note's text.

Now if you move the block from `research::ml::attention` to `papers::transformers::attention`:

- The block's `parent_id` changes (it now points to the `"transformers"` block).
- The lineage, atom, and edge are completely untouched.
- Every link targeting this lineage still works. The displayed path updates automatically.

This separation of concerns — immutable content, stable identity, flexible positioning, and explicit relationships — is what makes the four-table model work.
