# Seed Data

On first run, yap-orange auto-imports default content so new users see an interactive tutorial instead of an empty workspace. This document covers the architecture, configuration, and how to create custom seeds.

## How It Works

1. **Compile-time embedding** — `crates/yap-core/src/seed.rs` embeds `fixtures/tutorial.json` via `include_str!()`. All backends (server, desktop, WASM) link against yap-core, so all three get seed data with zero runtime configuration.

2. **First-run guard** — `bootstrap()` calls `get_root_blocks()` before doing anything. If the database has no root blocks (first run), it imports seed trees. If roots already exist, seeding is skipped entirely for fast startup.

3. **Idempotent merge** — Seed import uses `ImportOptions::seed_defaults()` (Merge mode + ContentIdentity matching). If for some reason seeding runs on a populated database, it deduplicates by content hash and skips existing nodes.

## Per-Frontend Behavior

### Server (`cargo run -p yap-server`)

Controlled by the `YAP_SEED_FILE` environment variable:

| Value | Behavior |
|-------|----------|
| *(not set)* | Uses built-in tutorial (compiled into binary) |
| `fixtures/tutorial.json` | Loads and imports that file |
| `fixtures/custom.json` | Loads any valid export JSON |
| `none` or `""` | No seeds — production mode |

The seed file is read once at startup. It must be a valid yap export JSON (single tree or array of trees).

### Desktop (Tauri)

Always uses the built-in tutorial seeds. No configuration — the desktop app is designed as a self-contained experience.

### WASM SPA (Browser)

Uses built-in tutorial seeds. The WASM `init()` function calls `bootstrap()` with `default_seed_trees()`.

**Factory Reset**: available in Settings (only in WASM mode). Clears all data, re-runs migrations, and re-bootstraps with seed data — restoring the database to its initial state.

## Tutorial Content

The built-in tutorial (`fixtures/tutorial.json`) is a ~27-node export tree:

```
tutorial/
  welcome                  — overview with links to all sections
  hierarchy/               — blocks, namespaces, creating, editing, nesting
  linking/                 — wiki-link syntax, resolution, backlinks, examples
  types/                   — custom types, type registry, creating schemas
  graph/                   — edges, creating edges, edge types
  organizing/              — drag-drop, indenting, keyboard shortcuts, context menu, tips
```

Features demonstrated:
- **Internal links** — cross-references between sections (15+ wiki-links)
- **Edges** — 4 semantic relationships (related-to, extends)
- **Content** — Markdown with live wiki-link examples readers can click

## Creating Custom Seed Content

1. **Export from a running instance**:
   ```bash
   # Export a subtree
   yap export my-namespace -o seed.json

   # Or use the API directly
   curl http://localhost:3000/api/blocks/<id>/export > seed.json
   ```

2. **Use as server seed**:
   ```bash
   YAP_SEED_FILE=seed.json cargo run -p yap-server
   ```

3. **Bundle into the binary** (for distribution):
   - Place the file at `fixtures/tutorial.json` (or add a new file)
   - Update `crates/yap-core/src/seed.rs` to `include_str!` the new file
   - Rebuild all targets

### Seed File Format

Seed files use the standard yap export format (`yap-tree-v1` or `yap-tree-v2`). The `parse_seed_json()` function accepts either a single `ExportTree` object or a JSON array of them:

```json
// Single tree
{ "format": "yap-tree-v1", "source_namespace": "...", "nodes": [...], "edges": [...] }

// Multiple trees
[
  { "format": "yap-tree-v1", "source_namespace": "a", "nodes": [...], "edges": [...] },
  { "format": "yap-tree-v1", "source_namespace": "b", "nodes": [...], "edges": [...] }
]
```

## Key Files

| File | Purpose |
|------|---------|
| `fixtures/tutorial.json` | Built-in tutorial content (~27 nodes) |
| `crates/yap-core/src/seed.rs` | `default_seed_trees()`, `parse_seed_json()` |
| `crates/yap-core/src/bootstrap.rs` | `bootstrap()` with first-run guard |
| `crates/yap-server/src/main.rs` | `load_seed_trees()` with `YAP_SEED_FILE` |
| `crates/yap-server-wasm/src/lib.rs` | `init()` + `factory_reset()` with seeds |
| `web/src/lib/views/SettingsView.svelte` | Factory Reset button (WASM only) |
