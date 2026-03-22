# Content Storage and Links

One of the most important design decisions in yap-orange is how note content and wiki links are stored. The editor shows human-readable link syntax like `[[research::ml::attention]]`, but the database stores something quite different. This chapter explains the translation between these two representations, why it works this way, and what it means for the durability of your links.

## What You See vs. What's Stored

In the editor, you write wiki links inline with your text:

```
See [[research::ml::attention]] for the details.
Also related to [[./sibling-note]].
```

This is the *display representation* — designed for humans. But yap-orange doesn't store this string directly. Instead, it splits the content into two parts:

**Template:**
```
See {0} for the details.
Also related to {1}.
```

**Links array:**
```json
["019x-lineage-id-for-attention", "019x-lineage-id-for-sibling"]
```

Each `{N}` placeholder in the template corresponds to the lineage UUID at index N in the links array. The template is stored in the atom's `content_template` field; the lineage UUIDs go in the `links` field.

## Why Separate Templates from Links?

This design exists to solve a fundamental problem: **paths change, but links shouldn't break.**

Consider a note that contains `[[research::ml::attention]]`. If you stored that path literally, what happens when someone moves the "attention" block to `papers::transformers::attention`? You'd need to find and update every note that mentioned the old path. Miss one, and you have a broken link. In a large knowledge base with thousands of cross-references, this is a recipe for link rot.

yap-orange avoids this entirely. The stored link is a lineage UUID — the stable identity of the content, which never changes regardless of where the block sits in the hierarchy. When the note is loaded for display, the system resolves each lineage UUID to its current namespace path and reconstructs the `[[path]]` syntax. If the block moved, the displayed path automatically reflects the new location. No links break. No bulk updates needed.

This is the same principle behind how the [data model](./data-model.md) separates blocks (position) from lineages (identity). Content storage is where that separation pays its biggest dividend.

## Serialization: Editor to Storage

When you save a note, the system transforms the editor's display text into the storage representation. Here is the step-by-step process:

### Step 1: Parse wiki links

The content is scanned for `[[...]]` patterns. The parser is a hand-written state machine (not a regular expression) because wiki link syntax has edge cases that regexes handle poorly — quoted segments containing `::`, escaped quotes, and relative path prefixes.

From this input:

```
See [[research::ml::attention]] for the details.
Also related to [[./sibling-note]].
```

The parser extracts two links:
1. `research::ml::attention` (absolute path)
2. `./sibling-note` (relative path — child of current block's namespace)

### Step 2: Resolve paths to lineage IDs

Each extracted path is resolved against the block hierarchy. Resolution walks the `blocks` table:

- **Absolute paths** (`research::ml::attention`): Start at root blocks, walk down matching names segment by segment.
- **Relative paths** (`./sibling-note`): Resolve relative to the current block's position in the tree. `./` means "child of my namespace," `../` means "sibling."
- **Quoted segments** (`[["name::with::colons"]]`): The double-quote syntax prevents `::` from being interpreted as a path separator.

Each resolved block yields its `lineage_id`. If a path can't be resolved — the target doesn't exist — the link is left as literal text in the template rather than being converted to a placeholder. This means unresolved links are visible in the stored content and can be re-resolved later if the target is created.

### Step 3: Replace links with placeholders

Each resolved link is replaced with a numbered placeholder:

```
See {0} for the details.
Also related to {1}.
```

The lineage UUIDs are collected in order:

```json
["019abc-lineage-for-attention", "019def-lineage-for-sibling"]
```

### Step 4: Create a new atom

The template and links array are written to a new atom row. The atom's `predecessor_id` points to the previous atom (the one the lineage was pointing to before this edit). The lineage's `current_id` is then updated to point at this new atom, and its `version` increments.

This is the immutability guarantee in action: the old atom is never modified. The edit history is preserved as a chain of atoms linked by `predecessor_id`.

## Deserialization: Storage to Editor

When you open a note for editing, the reverse transformation occurs.

### Step 1: Load template and links

The current atom is fetched via the lineage's `current_id`. This gives us the `content_template` and `links` array.

### Step 2: Resolve lineage IDs to paths

For each lineage UUID in the `links` array:

1. Find the block(s) that reference this lineage (via `blocks.lineage_id`).
2. Pick the appropriate block. (If the same lineage appears in multiple places, the system resolves to the most contextually relevant one.)
3. Walk the block's `parent_id` chain up to the root, collecting names.
4. Join them with `::` to form the namespace path.

For example, lineage UUID `019abc...` might resolve to block "attention" → parent "ml" → parent "research" → root, yielding the path `research::ml::attention`.

### Step 3: Replace placeholders with wiki links

Each `{N}` in the template is replaced with `[[resolved-path]]`:

```
See [[research::ml::attention]] for the details.
Also related to [[./sibling-note]].
```

This is what the editor displays. If a lineage UUID can't be resolved (the lineage was deleted, or all its blocks were removed), the placeholder can be rendered differently in the UI to indicate a broken reference.

## Link Syntax Reference

The wiki link parser supports several path forms:

| Syntax | Meaning |
|--------|---------|
| `[[foo::bar]]` | Absolute path from root |
| `[[./child]]` | Child of current block's namespace |
| `[[../sibling]]` | Sibling (parent's child) |
| `[[..]]` | Parent namespace |
| `[[/foo::bar]]` | Explicit absolute (same as no prefix) |
| `[["name::with::colons"]]` | Quoted segment — `::` treated as literal |
| `[[foo::\"bar\"]]` | Escaped quote inside a segment |

Relative paths are resolved at serialization time. The stored link is always a lineage UUID, so the relative/absolute distinction is a convenience for the editor — it doesn't affect storage.

## Content Hashing

Every atom has a `content_hash` field containing a SHA-256 digest. The hash is computed over:

1. The `content_type` string
2. A null byte separator (`\x00`)
3. The `content_template` string
4. A null byte separator
5. The lineage UUIDs from the `links` array, sorted and concatenated

Because atoms are immutable, the hash is computed once at creation and never changes. Two atoms with identical content type, template text, and link targets will produce the same hash.

This enables **content-addressable deduplication**. During import, the system can check whether an atom with the same hash already exists before creating a new one. In `merge` import mode, this prevents duplicate content from piling up when you import the same export file multiple times.

The hash deliberately includes the link target UUIDs, not the displayed paths. This means the hash is sensitive to *what* you're linking to, not *where* it currently lives in the tree. Two notes with identical text but linking to different targets will have different hashes.

## Unresolved Links

Not every link can be resolved. The target might not exist yet, or the path might be misspelled. yap-orange handles this gracefully:

- **During serialization (save):** If a `[[path]]` can't be resolved to a lineage, it's kept as literal text in the template. It won't appear in the `links` array. The next time you edit the note, it will still show up as `[[path]]` in the editor — giving you the chance to fix the path or create the target.

- **During deserialization (load):** If a lineage UUID in the `links` array can't be resolved to a path (the lineage or all its blocks were deleted), the UI can render the placeholder differently to indicate a broken reference.

This means links degrade gracefully. A misspelled path doesn't corrupt anything — it just stays as text until it's fixed. A deleted target shows a visual indicator rather than silently disappearing.

## A Complete Example

Let's trace through a full cycle.

**You write in the editor:**
```
The transformer architecture uses [[research::ml::attention]]
mechanisms. For implementation, see [[projects::code::transformer-lib]].
```

**Serialization (save):**

1. Parser finds two links: `research::ml::attention` and `projects::code::transformer-lib`.
2. Path resolution yields lineage IDs: `019a-aaaa-...` and `019b-bbbb-...`.
3. Template becomes: `The transformer architecture uses {0}\nmechanisms. For implementation, see {1}.`
4. Links array: `["019a-aaaa-...", "019b-bbbb-..."]`
5. New atom created with these values. Lineage updated to point at it.

**Someone moves the "attention" block to `papers::transformers::attention`.**

**Deserialization (load) — some time later:**

1. Template loaded: `The transformer architecture uses {0}\nmechanisms. For implementation, see {1}.`
2. Links loaded: `["019a-aaaa-...", "019b-bbbb-..."]`
3. Lineage `019a-aaaa-...` now resolves to `papers::transformers::attention` (new location).
4. Lineage `019b-bbbb-...` still resolves to `projects::code::transformer-lib`.
5. Editor displays:
```
The transformer architecture uses [[papers::transformers::attention]]
mechanisms. For implementation, see [[projects::code::transformer-lib]].
```

The link updated itself. No migration script, no search-and-replace, no broken links. The content template never changed — only the runtime resolution of the lineage UUID to its current path.
