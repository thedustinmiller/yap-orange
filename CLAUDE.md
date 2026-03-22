# CLAUDE.md

## Project Overview

**yap-orange** is a Rust-based note-taking system with hierarchical organization and graph linking. It uses an inode-like architecture with four tables:

- **Atoms** - Immutable content snapshots (append-only, like filesystem inodes)
- **Lineages** - Mutable pointers to current atom snapshot (stable identity for links)
- **Blocks** - References to lineages in the hierarchy (like directory entries)
- **Edges** - Non-hierarchical semantic relationships between lineages

Links target lineage IDs, not paths or block positions, so content survives moves and edits without link rot.

## Tech Stack

- **Server:** Rust + Axum 0.8 + Tokio
- **Database:** PostgreSQL 16 + SQLx 0.8
- **Desktop:** Tauri 2 + pg-embed (embedded Postgres)
- **Browser SPA:** In-WASM Axum + SQLite (sqlite-wasm-rs + OPFS persistence)
- **CLI:** Clap 4 (derive macros) + reqwest 0.12 (HTTP client)
- **IDs:** UUIDv7 (time-sortable)
- **Frontend:** Svelte 5 + CodeMirror 6 + xyflow + Dockview (`web/`)

## Development Commands

```bash
# Start PostgreSQL database
docker compose up -d

# Run migrations
cargo run -p yap-cli -- db migrate

# Run all tests
cargo test

# Run tests for specific crate
cargo test -p yap-core

# Start server
cargo run -p yap-server

# Start web UI (separate terminal, runs at http://localhost:5173)
cd web && npm install && npm run dev

# CLI help
cargo run -p yap-cli -- --help

# Check compilation
cargo check --workspace

# Dev tasks (xtask)
cargo xtask db setup       # Run migrations
cargo xtask db reseed      # Clear + seed sample data
cargo xtask run server     # Start API server
cargo xtask run web        # Start Vite dev server
cargo xtask run desktop    # Start Tauri desktop app
cargo xtask check all      # cargo check + npm typecheck

# Testing
cargo xtask test all                   # Run all tests (Rust + web)
cargo xtask test rust                  # Run all Rust tests (workspace)
cargo xtask test rust -- -p yap-core   # Run specific crate tests
cargo xtask test web                   # Run all web tests (vitest + playwright)
cargo xtask test unit                  # Run web unit tests only (vitest)
cargo xtask test e2e                   # Run web e2e tests only (playwright)

# Building
cargo xtask build server               # Build server (debug)
cargo xtask build server --release     # Build server (release)
cargo xtask build cli                  # Build CLI (debug)
cargo xtask build cli --release        # Build CLI (release)
cargo xtask build web                  # Build web frontend (vite build)
cargo xtask build wasm                 # Build WASM SPA module
cargo xtask build desktop              # Build Tauri desktop app (release)
cargo xtask build desktop --debug      # Build Tauri desktop app (debug)
cargo xtask build all                  # Build everything
cargo xtask build all --release        # Build everything (release)
```

### WASM / Browser SPA

```bash
# Build WASM for browser SPA mode
cd crates/yap-server-wasm && wasm-pack build --target web --out-dir ../../web/public/wasm --out-name yap_server_wasm

# Run SPA mode (no backend server needed)
cd web && npm run dev
# Open http://localhost:5173 — auto-detects no server and starts WASM worker

# Check WASM crates compile
cd crates/yap-store-wasm && cargo check --target wasm32-unknown-unknown
cd crates/yap-server-wasm && cargo check --target wasm32-unknown-unknown
```

## Workspace Structure

```
crates/
├── yap-core/       # Shared library: models, Store trait, link parsing, export logic
├── yap-store-pg/   # PostgreSQL implementation of the Store trait (SQLx queries)
├── yap-server/     # HTTP API server (Axum) — also a library exposing build_router()
├── yap-desktop/    # Tauri 2 desktop app: embeds pg-embed + Axum, no external deps
├── yap-store-wasm/ # SQLite Store impl for WASM (sqlite-wasm-rs FFI, excluded from workspace)
├── yap-server-wasm/# WASM entry point: init() + handle_request() (cdylib, excluded from workspace)
└── yap-cli/        # Command-line interface (Clap) — talks to server via HTTP
web/                # Svelte 5 frontend (Dockview layout, xyflow graph, CodeMirror editor)
guide/              # User guide (mdBook)
xtask/              # Development task runner (db, run, check commands)
migrations/         # PostgreSQL migration files
fixtures/           # Sample data for import testing
docs/               # Technical reference (architecture, API, export format)
```

## Desktop Development

```bash
# Run the desktop app (starts embedded Postgres + Axum + Tauri window)
cd crates/yap-desktop && cargo tauri dev

# First run downloads the Postgres binary (~50 MB) to ~/.cache/pg-embed/
# Data is persisted to ~/.local/share/yap-orange/ (Linux)
```

## Data Model

- **atoms**: `id`, `content_type`, `content_template`, `links[]` (lineage IDs), `properties`, `content_hash`, `predecessor_id`, `created_at`
- **lineages**: `id`, `current_id` (→ atoms), `version`, `deleted_at`, `updated_at`
- **blocks**: `id`, `lineage_id`, `parent_id`, `name`, `position` (fractional index), `deleted_at`, `created_at`
- **edges**: `id`, `from_lineage_id`, `to_lineage_id`, `edge_type`, `properties`, `deleted_at`, `created_at`

Atoms are immutable (append-only). Soft deletes live on lineages and blocks. Namespace paths are computed dynamically by walking the `parent_id` chain — not stored.

## Link Syntax

Wiki-link style: `[[namespace::path::to::"block name"]]`

- `[[foo::bar]]` - Absolute path
- `[[./foo]]` - Child of current namespace
- `[[../foo]]` - Sibling
- `[["name with::colons"]]` - Quoted segments

## Entry Creation

An **entry** is a block of a custom type. Create one by typing `@typeName{"field":"value"}` as the entire content of a new block. The frontend parses this, sets `content_type` and `properties`, and clears the text. The server never sees the `@` syntax — it receives `content_type` + `properties` directly. CLI equivalent: `--type typeName --prop '{"field":"value"}'`.

## Content Types

- `content` — default, markdown text with wiki-links (rendered via CodeMirror/ContentRenderer)
- `raw_text` — literal monospace text, no markdown or link processing
- `schema` — type definition (rendered by SchemaView, nav/edit mode)
- `setting` — UI settings (rendered by SettingsView)
- `type_registry` — schema list manager (rendered by TypeRegistryView)
- `todo` — task with status/description/time_ranges (rendered by TodoView, nav/edit mode)
- Any other string — custom type, rendered by EntryView (schema-driven form, nav/edit mode) or a registered custom view

All custom views support nav mode (compact inline summary) and edit mode (full form). OutlinerNode passes `{isEditing}` as a prop to all views. Enter/click activates edit mode; Escape saves and exits.

## Settings

"Settings" in this project always refers to the `settings::ui` block — a regular block in the hierarchy at namespace `settings`, name `ui`, with `content_type = "setting"`. All UI preferences are stored as key-value pairs in this block's `properties`. This means settings are persisted the same way as any other content: through the atom/lineage system.

The settings store (`web/src/lib/settingsStore.svelte.ts`) provides `getSetting(key)` / `setSetting(key, value)` for reading/writing. The settings UI (`web/src/lib/views/SettingsView.svelte`) renders an inline form when navigating to the settings block.

Current settings keys:
- `theme` — dark, light, system
- `font_size` — editor font size (10-24)
- `editor_line_numbers` — show line numbers in editor
- `dev_mode` — enables debug info bar and debug log panel
- `default_namespace` — default namespace for new blocks
- `max_expand_depth` — max depth for auto-expand and expand-all (0 = unlimited)
- `last_location` — (internal) last navigated block
- `outliner_expanded` — (internal) expanded block IDs in outliner
- `sidebar_expanded` — (internal) expanded block IDs in sidebar

## Environment

Copy `.env.example` to `.env`. Key variables:
- `DATABASE_URL` - PostgreSQL connection string
- `SERVER_HOST` / `SERVER_PORT` - API server binding
- `YAP_SERVER_URL` - Server URL for CLI (default: `http://localhost:3000`)
- `YAP_SEED_FILE` - Seed data for first-run import (see Seed Data below)
- `RUST_LOG` - Logging levels

## Seed Data

On first run (empty database), all three frontends auto-import a built-in tutorial tree (`fixtures/tutorial.json`, ~27 nodes) covering hierarchy, linking, types, graph, and organizing features. The tutorial appears as the `tutorial` namespace in the sidebar.

The seed module (`crates/yap-core/src/seed.rs`) embeds the fixture at compile time via `include_str!`. The `bootstrap()` function checks if the database has any root blocks; if empty, it imports the seed trees using idempotent merge (ContentIdentity matching). Subsequent startups skip seeding.

**Server mode** — controlled by `YAP_SEED_FILE` env var:
- Not set: uses built-in tutorial (default)
- File path: loads and imports that JSON file
- `"none"` or empty: no seeds (production mode)

**Desktop mode** — always uses built-in tutorial seeds.

**WASM SPA mode** — uses built-in tutorial. Factory Reset (Settings panel) clears all data and re-imports seeds.

**Creating custom seed content**: export a subtree from a running instance (`yap export <path> -o seed.json`), then set `YAP_SEED_FILE=seed.json`.

## Keyboard Shortcuts

### Panel Navigation
- `Alt+1..5` — Toggle focus: Navigator, Bookmarks, Links, Properties, Graph
- `Ctrl+K` — Quick Switcher (MRU panel list, arrow keys to cycle, Enter to select)

### Outliner (navigate mode)
- `Arrow Up/Down` — Move selection (`Shift` extends)
- `Arrow Right/Left` — Expand/collapse or move to child/parent
- `Enter` — Edit selected block
- `Tab/Shift+Tab` — Indent/outdent
- `Delete/Backspace` — Delete selected

### Editor (edit mode)
- `Escape` / `Cmd+Enter` — Save and exit
- `Enter` — New sibling block
- `Shift+Enter` — Newline
- `[[` — Wiki link completion

### Custom Views (edit mode)
- `Escape` — Save and exit to nav mode
- `Tab/Shift+Tab` — Cycle between inputs within the view
- `Enter` or click enters edit mode (same as regular blocks)

## Accessibility

The frontend follows WCAG 2.1 AA. Zero `svelte-ignore a11y_*` suppressions in the codebase. All interactive elements are native `<button>` with ARIA labels. Key patterns: tree view (sidebar, outliner), menu (context menu), dialog (quick switcher, delete modal), status (toasts). See `docs/architecture.md` for the full ARIA pattern reference.
