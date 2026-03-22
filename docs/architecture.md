# Architecture

## Overview

yap-orange uses a four-table database model inspired by filesystem inodes. The key insight is separating *content* from *position*: notes can appear in multiple places, and links target stable identity objects rather than mutable paths.

There are three deployment modes:

### Web / development mode

```
┌─────────────────────────────────────────┐
│        Svelte 5 Web UI (port 5173)      │
│  Dockview layout, xyflow graph, CodeMirror editor  │
└─────────────────┬───────────────────────┘
                  │ HTTP / JSON
┌─────────────────▼───────────────────────┐
│       Rust Server - Axum 0.8 (port 3000) │
│           yap-core library               │
└─────────────────┬───────────────────────┘
                  │ SQLx
┌─────────────────▼───────────────────────┐
│     PostgreSQL 16 (Docker, port 5432)   │
└─────────────────────────────────────────┘
```

### Desktop mode (`yap-desktop`)

```
┌─────────────────────────────────────────┐
│         Tauri 2 webview window          │
│      (same Svelte 5 frontend build)     │
└─────────────────┬───────────────────────┘
                  │ HTTP / JSON (random port, localhost only)
┌─────────────────▼───────────────────────┐
│     Axum server (yap-server router)     │
│      embedded in yap-desktop process    │
└─────────────────┬───────────────────────┘
                  │ SQLx
┌─────────────────▼───────────────────────┐
│   PostgreSQL (pg-embed, random port)    │
│   binary auto-downloaded on first run   │
│   data persisted in platform data dir   │
└─────────────────────────────────────────┘
```

The desktop build is a single binary with no external dependencies. On first launch pg-embed downloads the PostgreSQL binary (~50 MB) and caches it in `~/.cache/pg-embed/`. Database files are kept in the platform data directory (`~/.local/share/yap-orange` on Linux, `~/Library/Application Support/yap-orange` on macOS).

### Browser SPA mode (WASM)

```
┌─────────────────────────────────────────┐
│        Svelte 5 Web UI (same build)     │
│     api.ts detects WASM mode            │
└─────────────────┬───────────────────────┘
                  │ postMessage
┌─────────────────▼───────────────────────┐
│   Dedicated Web Worker (wasm-worker.js) │
│     WASM binary (yap-server-wasm)       │
│     Axum Router + SQLite (in-process)   │
│     OPFS persistence (sahpool VFS)      │
└─────────────────────────────────────────┘
```

The SPA build runs the entire API server inside the browser. The Axum router, request handlers, and SQLite database all compile to WebAssembly and run inside a Dedicated Web Worker. Browser storage uses the Origin Private File System (OPFS) via the `sahpool` VFS, giving persistent SQLite storage that survives page reloads. No backend server, Docker, or external database is needed.

**How it works:**

- `initWasmWorker()` in `sw-register.ts` checks if a backend server is available (`GET /health` with 1s timeout). If not, it spawns a Dedicated Worker (`wasm-worker.js`).
- The worker loads `yap_server_wasm.wasm`, installs the OPFS VFS, opens SQLite, runs migrations, bootstraps meta-schema and settings, and builds the Axum router.
- `api.ts` checks `isWasmMode()` — if true, requests are sent via `postMessage` to the worker instead of HTTP `fetch()`. The worker routes them through the Axum router and returns `{status, headers, body}` JSON.
- Static assets are still served by the web server (Vite dev or any static host).

**Key constraint:** `createSyncAccessHandle` (needed by OPFS sahpool VFS) is only available in Dedicated Web Workers — not Service Workers or the main thread. This is why the WASM engine runs in a Worker, not directly in the page.

**Limitations:** Single-tab only (sahpool VFS supports one connection). Multi-tab would require SharedWorker (future work). Data lives in OPFS — not accessible from other origins.

The CLI (`yap`) talks to the server over HTTP, not directly to the database. All business logic lives in `yap-core`, shared between server and CLI.

---

## Data Model

### Four Tables

```
atoms ──── immutable content snapshots (append-only)
lineages ── mutable pointer to current atom snapshot
blocks ──── hierarchy entries (directory-entry-like)
edges ────  non-hierarchical relationships
```

### Atoms — Immutable Content Snapshots

```sql
CREATE TABLE atoms (
    id              UUID PRIMARY KEY,
    content_type    TEXT NOT NULL DEFAULT 'content',
    content_template TEXT NOT NULL DEFAULT '',
    links           UUID[] NOT NULL DEFAULT '{}',
    properties      JSONB NOT NULL DEFAULT '{}',
    content_hash    TEXT NOT NULL DEFAULT '',
    predecessor_id  UUID REFERENCES atoms(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Atoms are **append-only**. Each edit creates a new atom row with `predecessor_id` pointing to the previous version. You never update or delete an atom row. This gives you:

- Full edit history
- Content-addressable deduplication via `content_hash`
- Safe concurrent reads (no dirty reads or update conflicts)

The `content_template` stores text with `{0}`, `{1}` placeholders where wiki links appear:

```
content_template: "See {0} for the details."
links: ["550e8400-e29b-41d4-a716-446655440001"]
```

The `links` array holds lineage IDs (not atom IDs) in placeholder order. This matters: links always point to the stable lineage identity, so they survive content edits.

### Lineages — Mutable Identity Pointers

```sql
CREATE TABLE lineages (
    id          UUID PRIMARY KEY,
    current_id  UUID NOT NULL REFERENCES atoms(id),
    version     INTEGER NOT NULL DEFAULT 1,
    deleted_at  TIMESTAMPTZ,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

A lineage is a **mutable pointer** to whichever atom snapshot is currently "live". When you edit content:

1. A new atom is created with `predecessor_id = old_atom.id`
2. `lineages.current_id` is updated to point at the new atom
3. `lineages.version` increments

The lineage ID equals the first atom's UUID (set at creation time), so existing references never change even as the content evolves. Soft deletes live on lineages, not atoms.

### Blocks — Hierarchy Entries

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

Blocks place lineages in the hierarchy. Multiple blocks can reference the same lineage (the same content appears in multiple locations). The `parent_id` self-reference defines the tree structure. NULL `parent_id` means a root block.

`position` is a fractional index string for lexicographic ordering — this allows inserting between any two positions without renumbering.

The "namespace path" (`research::ml::attention`) is computed by walking the `parent_id` chain and joining block names with `::`. It is not stored; it is derived on read.

### Edges — Semantic Relationships

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

Edges are explicit semantic relationships between lineages that are not inline content links and not parent-child hierarchy. Examples: `references`, `inspired-by`, `depends-on`, `blocks`.

There's a unique constraint on `(from_lineage_id, to_lineage_id, edge_type)` where `deleted_at IS NULL`, preventing duplicate edges of the same type between the same pair.

---

## Content Storage Model

### Editor View vs. Storage

The editor shows wiki-link syntax:

```
See [[research::ml::attention]] for the details.
Also related to [[./sibling-note]].
```

Storage separates template from references:

```
content_template: "See {0} for the details.\nAlso related to {1}."
links: [
    "019x-lineage-id-for-attention",
    "019x-lineage-id-for-sibling"
]
```

### Why This Matters

**Links survive moves.** Blocks can be moved to different namespaces. The displayed path `[[research::ml::attention]]` is derived dynamically from the current block location. The stored link is a lineage ID. If you move the `attention` block to `projects::papers::attention`, all links that referenced it will show the new path on the next read — no link rot.

**Immutable atoms enable deduplication.** The `content_hash` is a SHA-256 of the content type, template, and sorted link lineage IDs. Two identical notes get the same hash, enabling dedup during import (`merge` mode).

### Serialization (Editor → Storage)

When saving:

1. Parse the content for `[[...]]` wiki links
2. Resolve each link path to a lineage ID via the `blocks` table
3. Replace each `[[path]]` with `{N}` placeholder
4. Store lineage IDs in order in the `links` array
5. Create a new atom with `predecessor_id = current_atom_id`
6. Update lineage to point at the new atom

Unresolved links (path not found) are kept as literal text; they show up differently in the UI.

### Deserialization (Storage → Editor)

When loading:

1. Load `content_template` and `links` array from the current atom
2. For each lineage ID in `links`:
   - Find the block(s) referencing it
   - Walk block's `parent_id` chain to reconstruct the namespace path
   - Format as `[[namespace::path::name]]`
3. Replace `{N}` placeholders with formatted wiki links
4. Return rendered content

---

## Link Syntax

```
[[namespace::path::to::"block name"]]
```

The parser is a hand-written state machine (not regex) to handle quoted segments, escape sequences, and relative paths correctly.

| Syntax | Resolution |
|--------|-----------|
| `[[foo::bar]]` | Absolute path from root |
| `[[./child]]` | Child of current block's namespace |
| `[[../sibling]]` | Sibling (one level up + down) |
| `[[..]]` | Parent namespace |
| `[[/foo::bar]]` | Explicit absolute (same as no prefix) |
| `[["name::with::colons"]]` | Quoted segment (treats `::` as literal) |
| `[[foo::\"bar\"]]` | Escaped quote inside segment |

---

## Custom Types

The custom types system lets you define structured schemas and create typed entries (blocks with a custom content type).

### Content types

Every block has a `content_type` that determines how it is rendered:

| Content Type | Description |
|-------------|-------------|
| `content` | Default. Markdown text with wiki-links |
| `raw_text` | Literal monospace rendering, no markdown processing |
| `schema` | Type definition (rendered by SchemaView) |
| `setting` | UI settings (rendered by SettingsView) |
| `type_registry` | Schema manager (rendered by TypeRegistryView) |
| `todo` | Task with status cycling (rendered by TodoView) |
| *any other string* | User-defined type, rendered by EntryView or a custom view |

### How schemas are stored

A schema is an ordinary block with `content_type = "schema"`. Its `properties` JSONB field holds the field definitions:

```json
{
  "fields": [
    {"name": "email", "type": "string"},
    {"name": "role", "type": "enum", "options": ["engineer", "manager"]},
    {"name": "team", "type": "string", "required": true}
  ]
}
```

Schema field types: `string`, `number`, `boolean`, `date`, `enum`, `ref`, `text`.

The `ref` field type creates a reference to another typed entry, with `target_type` specifying the expected type name.

Schemas live under `types::<name>` in the namespace tree (e.g. `types::person`). Local overrides at `<namespace>::types::<name>` shadow the global one — resolution walks up the namespace hierarchy until a match is found.

### Entries (typed blocks)

An entry is a regular block with a custom content type. Its atom has:

- `content_type` — the schema name (e.g. `"person"`)
- `properties` — field values plus schema version pinning:

```json
{
  "_schema_atom_id": "<atom-id-of-schema-at-creation-time>",
  "email": "alice@example.com",
  "role": "engineer"
}
```

The `_schema_atom_id` pins the entry to the specific schema version it was created against.

### Two-tier view system

Entries are rendered by a two-tier view system:

1. **Custom views** — registered in `typeViewRegistry.ts`, these override the default rendering for specific content types (e.g., `TodoView` for `todo`, `SettingsView` for `setting`).
2. **EntryView** — the generic fallback that auto-generates a schema-driven form from the entry's schema fields. Each field type has a dedicated field view component in `views/fields/` (StringField, NumberField, BooleanField, DateField, EnumField, TextField, RefField).

Entries store all data in `properties` — there is no freeform content on typed blocks (except via `text` fields defined in the schema).

### Nav/edit mode

All custom views (EntryView, TodoView, SchemaView) support two display modes, matching the same Enter/Escape pattern as regular content blocks:

- **Nav mode** (default) — compact, inline summary for scanning:
  - EntryView: `name: Alice · email: alice@ex.com`
  - TodoView: `[checkbox] STATUS description... time`
  - SchemaView: `name:string · email:string · birthday:date`
- **Edit mode** — full form, activated by Enter or click:
  - Tab/Shift+Tab cycles between inputs within the view
  - Escape saves and exits back to nav mode
  - First input is auto-focused on entry

OutlinerNode passes `{isEditing}` as a prop to all custom and entry views. Views accept `isEditing` as an optional boolean prop (default `false`). In nav mode, views do not block click propagation, so clicking a custom view block enters edit mode the same way clicking a regular content block does.

### The `@type{...}` creation command

In the editor, type `@type{...}` to create a new typed entry. This is a one-shot client-side command, not persistent syntax:

```
@person{"name":"Alice","email":"alice@example.com"}
```

On save, the `typeCommand.ts` parser extracts the command, sets `content_type = "person"` and the field values as `properties` on the block itself. The server never sees the `@type{...}` syntax — it only receives the resulting content_type and properties.

### Core/server is dumb

The core and server layers are type-unaware. They store `content_type` and `properties` as opaque values with no validation. All type logic (schema resolution, field rendering, `@type{...}` parsing) lives in the frontend.

### DB index

Migration `002_content_type_index.sql` adds a B-tree index on `atoms(content_type)` for efficient type-filtered queries.

### Atom snapshot endpoint

`GET /api/atoms/snapshot/:atom_id` returns a specific atom snapshot by its atom ID (not lineage ID). This is used to retrieve pinned schema versions via `_schema_atom_id`. Backed by the `get_atom_by_id()` Store method.

---

## Export / Import

The `yap-tree-v1` format exports a subtree as a self-contained JSON bundle. See [export-format.md](./export-format.md) for the full specification.

Two import modes:

- **merge** (default): Deduplicates nodes via `_import_hash` stored in atom properties. If an identical node already exists in the target database, it is reused. External links (pointing outside the exported subtree) are resolved against the target database.
- **copy**: Creates all nodes with fresh UUIDs. External links become `Uuid::nil()` (the zero UUID), visually marked as unresolved.

---

## Crate Structure

### `yap-core`

Shared library. Contains:

- `models.rs` — `Atom`, `Lineage`, `Block`, `Edge` structs and DTOs
- `store.rs` — `Store` trait: backend-agnostic interface (~35 required methods)
- `links.rs` — Wiki link parser and path resolver
- `content.rs` — Serialize/deserialize content (editor ↔ storage)
- `export.rs` — Export/import subtree logic
- `hash.rs` — Content hash computation (SHA-256)
- `error.rs` — Error types

### `yap-store-pg`

PostgreSQL backend implementing the `Store` trait from `yap-core`. All SQLx queries live here. Error mapping (e.g., unique violation → `Error::Conflict`) is handled in this crate.

### `yap-store-sqlite`

Native SQLite backend implementing the `Store` trait from `yap-core`. Uses `sqlx` with the SQLite driver. Serves two purposes: a lightweight backend for local development and testing (no Docker required), and the shared SQL implementation that `yap-store-wasm` adapts for WASM targets. The two crates use the same SQL queries.

### `yap-store-tests`

Cross-backend test suite. Defines tests via a `store_tests!` declarative macro that expands the same test body once for each registered backend. Tests cover CRUD, hierarchy traversal, move safety (cycle detection), edges, and export/import. SQLite tests run without `DATABASE_URL`; PostgreSQL variants are conditionally ignored unless a database is available.

### `yap-bench`

Performance benchmarks for `Store` operations (CRUD, hierarchy traversal, backlink queries). The benchmark runner is exposed via the `yap-server` `bench` feature flag, which adds a `POST /api/debug/benchmarks` endpoint that accepts a `BenchmarkConfig` and returns results.

### `yap-server`

Axum HTTP server. Exposes a `build_router(AppState) -> Router` library function so that `yap-desktop` can embed the exact same API without duplication. The standalone binary reads `DATABASE_URL` from the environment and binds on `SERVER_HOST:SERVER_PORT` (defaults: `0.0.0.0:3000`).

Optional `openapi` feature flag enables utoipa-generated OpenAPI spec and Swagger UI (served at `/swagger-ui`).

### `yap-cli`

Clap CLI. The `client.rs` module is a typed HTTP client wrapping `reqwest`. All commands call the server; the CLI does not touch the database directly.

### `yap-store-wasm`

SQLite backend for WASM targets. Implements the `Store` trait using `sqlite-wasm-rs` (raw FFI bindings to SQLite compiled to WASM) instead of `sqlx`. Uses the exact same SQL queries as `yap-store-sqlite`. The `WasmDb` wrapper in `db.rs` provides a safe API over the unsafe `sqlite3_*` FFI calls. Only compiles for `wasm32-unknown-unknown`.

### `yap-server-wasm`

WASM entry point (`cdylib`). Exports `init()` and `handle_request(method, url, body)` via `wasm-bindgen`. Initializes the OPFS VFS, opens SQLite, runs migrations, builds the Axum router, and routes requests. Built with `wasm-pack build crates/yap-server-wasm --target web`.

### `yap-desktop`

Tauri 2 desktop application. Acts as an orchestrator:

1. Starts embedded PostgreSQL via **pg-embed 1.0** on a random port
2. Connects `PgStore`, runs SQLx migrations, seeds the meta-schema
3. Starts the Axum server (`yap-server::build_router`) on a second random port
4. Opens a Tauri webview; exposes a `get_server_port` IPC command so the frontend can discover the server URL at runtime
5. On window close: signals Axum shutdown, drops the pg-embed handle to stop Postgres

The frontend detects the Tauri environment via `window.__TAURI_INTERNALS__` and calls `initApi()` once before mounting — this sets the `BASE_URL` for all API calls. The same frontend build works in both browser and desktop contexts.

---

## Indexes

Key indexes on the schema:

```sql
-- Fast backlink queries: "who links to lineage X?"
CREATE INDEX idx_atoms_links ON atoms USING GIN (links);

-- Fast property queries
CREATE INDEX idx_atoms_properties ON atoms USING GIN (properties);

-- Dedup by content hash
CREATE INDEX idx_atoms_content_hash ON atoms (content_hash);

-- Version history traversal
CREATE INDEX idx_atoms_predecessor ON atoms (predecessor_id);

-- Block tree traversal (children of a parent)
CREATE INDEX idx_blocks_parent_id ON blocks (parent_id) WHERE deleted_at IS NULL;

-- All blocks referencing a lineage
CREATE INDEX idx_blocks_lineage_id ON blocks (lineage_id) WHERE deleted_at IS NULL;

-- Unique name within a parent (enforces namespace uniqueness)
CREATE UNIQUE INDEX idx_blocks_unique_parent_name ON blocks (parent_id, name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_blocks_unique_root_name ON blocks (name) WHERE parent_id IS NULL AND deleted_at IS NULL;
```

---

## Technology Stack

| Layer | Technology |
|-------|-----------|
| Database (dev) | PostgreSQL 16 (Docker Compose) |
| Database (desktop) | PostgreSQL via pg-embed 1.0 (embedded, auto-downloaded) |
| Database (browser) | SQLite via sqlite-wasm-rs (OPFS persistence) |
| WASM glue | wasm-bindgen + wasm-pack |
| Server | Rust + Axum 0.8 + Tokio |
| Core Library | `yap-core` (shared between server and CLI) |
| DB queries | SQLx 0.8 (compile-time checked) |
| IDs | UUIDv7 (time-sortable) |
| Position ordering | Fractional index strings |
| Frontend | Svelte 5 |
| UI layout | Dockview (resizable panels) |
| Graph view | @xyflow/svelte |
| Editor | CodeMirror 6 |
| Desktop shell | Tauri 2 |
| CLI | Clap 4 (derive macros) |
| HTTP client (CLI) | reqwest 0.12 |

---

## URL Routing

The frontend uses hash-based routing (`/#/...`) to persist navigation state across page reloads and enable browser back/forward navigation.

### Tier 1: Pipe-Delimited Paths (current)

Single outliner (backward compatible):
```
/#/journal::2026
/#/block/<UUID>
/#/                    — home / root blocks
```

Multiple outliners use pipe-delimited segments, sorted by outliner ID for stability:
```
/#/journal::2026|projects::yap-orange|research::ml
```

Special segments:
- `~` — home (root blocks view). Example: `/#/~|projects::yap-orange`
- `block/<UUID>` — direct block ID reference (works per-segment)

The URL encodes only *where* each outliner is navigated — not which one is active. Active tab state is persisted separately in dockview's layout settings. This means switching focus between outliner tabs does not rewrite the URL; only actual navigation changes it. Single-outliner URLs contain no pipes, preserving backward compatibility.

If the URL contains more paths than existing outliners, additional outliner tabs are created next to the current active outliner. If the URL contains fewer paths, extra outliners are unaffected (they keep their current location).

**Implementation:** `router.svelte.ts` handles serialization (via `getAllOutlinerPaths()` from `outlinerStore`, sorted by ID) and deserialization (via `openOutlinersFromPaths()` from `dockviewActions`). The `appState` route pusher callback goes through `pushRoute()` which automatically serializes all outliners when more than one exists.

### Tier 2: Layout Blob (future extension point)

For full dockview layout persistence in the URL (panel sizes, arrangement, scroll positions), a query parameter approach is planned:

```
/#/journal::2026|projects::yap-orange?layout=<deflate+base64url>
```

**Size budget:** Typical dockview layout JSON is ~1.6 KB raw. With deflate + base64url encoding this compresses to ~636 characters, well within browser URL limits (~2,000 safe, ~8,000 max).

**Potential use cases:**
- **Workspace bookmarks** — blocks with `content_type = "workspace"` that store a full layout URL in properties, allowing one-click workspace restore
- **Static site sharing** — shareable URLs that reconstruct the exact panel arrangement
- **Yjs collaboration** — room initialization from a layout URL so collaborators see the same workspace

This is documented as a design decision; implementation is deferred until a concrete use case requires it.

---

## Keyboard Shortcuts

### Global Panel Shortcuts

JetBrains-style `Alt+N` shortcuts toggle panel focus. These are handled by a global `keydown` listener in `DockLayout.svelte`.

| Shortcut | Panel |
|----------|-------|
| `Alt+1` | Navigator (sidebar) |
| `Alt+2` | Bookmarks |
| `Alt+3` | Links (backlinks) |
| `Alt+4` | Properties |
| `Alt+5` | Graph |

**Toggle behavior:** If the panel is hidden, it is added and focused. If visible but not focused, it receives focus. If already focused, focus returns to the last active outliner.

### Quick Switcher

`Ctrl+K` opens the Quick Switcher — an overlay listing all panels in most-recently-used (MRU) order.

- Arrow keys or repeated `Ctrl+K` to cycle through the list
- `Enter` to activate the selected panel
- `Escape` to cancel

**Implementation:** `QuickSwitcher.svelte` renders as a modal overlay with `role="dialog"` and listbox semantics. MRU state is managed by `panelHistory.svelte.ts` (module-level `$state` singleton).

### Outliner Navigation (when focused)

| Key | Action |
|-----|--------|
| `Arrow Up/Down` | Move selection |
| `Shift+Arrow Up/Down` | Extend selection |
| `Arrow Right` | Expand node / move to first child |
| `Arrow Left` | Collapse node / move to parent |
| `Enter` | Enter edit mode |
| `Escape` | Clear selection |
| `Tab` | Indent selected block(s) |
| `Shift+Tab` | Outdent selected block(s) |
| `Delete/Backspace` | Delete selected block(s) |

### Editor Mode (when editing a block)

| Key | Action |
|-----|--------|
| `Escape` / `Cmd+Enter` | Save and exit edit mode |
| `Enter` | Create new sibling block below |
| `Shift+Enter` | Insert newline |
| `Tab` | Indent block |
| `Shift+Tab` | Outdent block |
| `Arrow Up` (at line 1) | Save and move to previous block |
| `Arrow Down` (at last line) | Save and move to next block |
| `[[` | Wiki link autocomplete |

### Custom View Edit Mode (when editing a typed block)

| Key | Action |
|-----|--------|
| `Escape` | Save and exit to nav mode |
| `Tab` / `Shift+Tab` | Cycle between inputs within the view |

---

## Accessibility

The frontend follows WCAG 2.1 AA guidelines.

### Color Contrast

All text meets a minimum 4.5:1 contrast ratio against its background. The `--text-muted` color (`#7a85b8`) achieves ~4.7:1 on `--bg-primary` (`#1a1b26`).

### Landmarks and Structure

- The app root uses a `<main>` landmark
- Document `<title>` is set via `<svelte:head>`
- Toast notifications are wrapped in an `aria-live="polite"` region

### ARIA Patterns

| Component | Pattern | Key Attributes |
|-----------|---------|---------------|
| Sidebar tree | Tree view | `role="tree"`, `role="treeitem"`, `role="group"`, `aria-expanded` |
| Outliner | Application + Tree | `role="application"` (container), `role="tree"` (content), `role="treeitem"` with `aria-expanded`, `aria-selected`, `aria-level` |
| Context menu | Menu | `role="menu"`, `role="menuitem"` |
| Quick Switcher | Dialog + Listbox | `role="dialog"`, `role="listbox"`, `role="option"`, `aria-selected` |
| Delete modal | Dialog | `role="dialog"`, `aria-modal="true"`, focus trap |
| Toast | Status + Alert | `role="status"` (container), `role="alert"` (each toast) |
| CodeMirror editors | Input | `aria-label` via `EditorView.contentAttributes` |
| Schema fields | Form | `aria-label` on all inputs, selects, checkboxes |

### Interactive Elements

All interactive elements use native `<button>` elements (not `<span>` or `<div>` with click handlers). This ensures:
- Keyboard focusability without manual `tabindex`
- Enter/Space activation by default
- Screen reader announces the element as a button

Where a container needs click handling but also contains child buttons (e.g., bookmark items), the container uses `role="button"` with `tabindex="0"` and explicit keyboard handlers to avoid nested `<button>` elements.

Non-interactive containers that use `onclick` for event handling (e.g., `stopPropagation`) are annotated with `role="presentation"`.

### Suppressed Warnings

The codebase has **zero** `svelte-ignore a11y_*` suppressions. All accessibility warnings have been resolved at the source.
