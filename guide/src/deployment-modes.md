# Deployment Modes

yap-orange is a single system that runs in three different configurations. Every mode uses the same Svelte 5 frontend, the same Axum router, and the same `Store` trait from `yap-core`. The difference is where the database lives and how the pieces connect.

This chapter explains each mode, why it exists, and when to choose it.

## The shared architecture

Before diving into the modes, it helps to understand what they have in common. All three run the same stack:

- **Frontend** -- Svelte 5 with CodeMirror 6, xyflow, and Dockview
- **API layer** -- Axum 0.8 router (the same `build_router()` function)
- **Storage** -- An implementation of the `Store` trait (PostgreSQL or SQLite)

The frontend does not know or care which mode it is running in. It sends HTTP-shaped requests and gets JSON responses. The only variation is the transport: real HTTP requests (web and desktop modes) or `postMessage` to a Web Worker (WASM mode). This is handled transparently by `api.ts`.

---

## Mode 1: Web + Server

```
Browser (port 5173)  ──HTTP──▶  Axum server (port 3000)  ──SQLx──▶  PostgreSQL 16 (port 5432)
```

### How it works

You run PostgreSQL in Docker, start the Axum server as a native binary, and serve the Svelte frontend through Vite (in development) or as static files (in production). The frontend talks to the server over plain HTTP.

### When to choose it

- **Development.** Hot-reloading on both frontend and backend, full access to database tooling, and the fastest iteration loop.
- **Multi-user setups.** This is the only mode where multiple people can share the same database. Run the server on a shared machine or VPS and point browsers at it.
- **Production deployments.** PostgreSQL gives you mature backup tooling, replication, and the best performance for large datasets.

### Requirements

| Dependency | Purpose |
|---|---|
| Docker (or a PostgreSQL 16 install) | Database |
| Rust toolchain | Build and run the server |
| Node.js + npm | Build and serve the frontend |

### Quick start

```bash
docker compose up -d          # Start PostgreSQL
cargo run -p yap-cli -- db migrate   # Run migrations
cargo run -p yap-server       # Start API server (port 3000)
cd web && npm install && npm run dev  # Start frontend (port 5173)
```

Or, using the task runner:

```bash
cargo xtask db setup
cargo xtask run server   # In one terminal
cargo xtask run web      # In another
```

### Where data lives

In the PostgreSQL database. By default, Docker Compose maps a named volume so data survives container restarts. The connection string is configured in `.env` via `DATABASE_URL`.

---

## Mode 2: Desktop App (Tauri 2)

```
Tauri 2 webview  ──HTTP (localhost, random port)──▶  Axum server (in-process)  ──SQLx──▶  pg-embed (random port)
```

### How it works

The desktop app is a single Tauri 2 binary. When you launch it, three things happen inside the same process:

1. **pg-embed** starts an embedded PostgreSQL instance on a random port. On the first run, it downloads the Postgres binary (~50 MB) to `~/.cache/pg-embed/` and caches it for future launches.
2. The **Axum server** starts on a random localhost port, connected to that embedded Postgres.
3. A **Tauri webview** opens the Svelte frontend. The frontend detects the Tauri environment via `window.__TAURI_INTERNALS__`, calls `initApi()`, and discovers the server port through Tauri's IPC mechanism.

From that point on, the frontend works exactly like the web mode -- it just happens to be talking to a server inside the same process.

### When to choose it

- **Single-user, zero-config.** No Docker, no terminal commands. Launch the app and start taking notes.
- **Offline use.** Everything runs locally. No network connection needed after the first-run download.
- **Desktop integration.** Native window management, system tray, file system access -- all the things a Tauri app provides.

### Requirements

None, after the initial binary is built or installed. The first launch handles the Postgres download automatically.

For development:

```bash
cd crates/yap-desktop && cargo tauri dev
```

### Where data lives

| Platform | Path |
|---|---|
| Linux | `~/.local/share/yap-orange/` |
| macOS | `~/Library/Application Support/yap-orange/` |
| Windows | `%APPDATA%\yap-orange\` |

The pg-embed binary cache is stored separately at `~/.cache/pg-embed/`.

---

## Mode 3: Browser SPA (WASM)

```
Svelte 5 UI  ──postMessage──▶  Dedicated Web Worker  ──WASM──▶  Axum router + SQLite (sqlite-wasm-rs)  ──▶  OPFS
```

### How it works

The entire Axum server, including the router and a SQLite-backed `Store` implementation, is compiled to WebAssembly. When the frontend loads, it tries to reach the backend with a `GET /health` request. If that request times out (meaning no server is running), the frontend automatically spawns a Dedicated Web Worker that loads the WASM binary.

Once the worker is running, `api.ts` detects WASM mode via `isWasmMode()` and routes all API calls through `postMessage` instead of `fetch`. The worker receives each message, runs it through the Axum router, and posts the response back. From the frontend's perspective, the API behaves identically.

SQLite storage uses the `sqlite-wasm-rs` crate with the `sahpool` VFS, which persists data to the browser's Origin Private File System (OPFS).

### When to choose it

- **Zero-install demo.** Share a URL and the recipient can use the full application immediately. No server, no downloads, no accounts.
- **Trying it out.** If you want to evaluate yap-orange without committing to any infrastructure, this is the fastest path.
- **Offline-first, no native app.** Works in any modern browser with OPFS support.

### Limitations

- **Single-tab only.** The `sahpool` VFS acquires an exclusive lock on the OPFS files. Opening a second tab will fail to connect to the database.
- **Browser-origin-scoped data.** Your notes live in the browser's storage for that specific origin. Clearing site data deletes everything. Use export to back up your work.
- **Performance.** SQLite in WASM is slower than native PostgreSQL, especially for large datasets. Fine for hundreds or a few thousand notes; may feel sluggish beyond that.
- **No CLI access.** The CLI needs an HTTP server to talk to. In WASM mode there is no server listening on a port.

### Requirements

A modern browser with OPFS support (Chrome, Edge, Firefox, Safari 16.4+). Nothing else.

### Building the WASM target

```bash
cd crates/yap-server-wasm
wasm-pack build --target web --out-dir ../../web/public/wasm --out-name yap_server_wasm
```

Then start the frontend normally:

```bash
cd web && npm run dev
```

If no server is running on port 3000, the frontend will automatically fall back to WASM mode.

---

## Feature comparison

| Feature | Web + Server | Desktop | Browser SPA |
|---|---|---|---|
| Multi-user | Yes | No | No |
| Works offline | No | Yes | Yes |
| CLI access | Yes | Yes | No |
| Database | PostgreSQL 16 | PostgreSQL (embedded) | SQLite (WASM) |
| Performance (large datasets) | Best | Good | Acceptable |
| Install required | Docker + Rust + Node | Single binary | None |
| Data location | Postgres volume | Local app data directory | Browser OPFS |
| Cross-device sync | Via shared server | Export/import | Export/import |
| First-run download | Docker image pull | ~50 MB Postgres binary | ~5 MB WASM binary |

---

## Data portability

All three modes support the same export and import format. You can export your notes from the browser SPA, import them into a desktop app, then later import them into a shared server -- or any other combination. The data model is identical; only the storage engine differs.

```bash
# Export from server mode via CLI
yap export --output notes.json

# Import into any mode via the web UI or CLI
yap import --input notes.json
```

See the [Import & Export](./web-ui/import-export.md) chapter for details.

---

## How the frontend detects the mode

The detection logic lives in `web/src/lib/api.ts` and `web/src/sw-register.ts`. Here is the decision tree:

1. **Tauri check.** If `window.__TAURI_INTERNALS__` exists, the app is running inside the desktop shell. The frontend calls `initApi()` to discover the embedded server's port via Tauri IPC.
2. **Health check.** The frontend sends `GET /health` to the expected server URL. If the server responds, it uses normal HTTP mode.
3. **WASM fallback.** If the health check times out, the frontend spawns a Web Worker with the WASM binary and switches to `postMessage` transport.

This means the same frontend build works in all three modes without any build flags or configuration.

---

## Choosing a mode

**Start with the Browser SPA** if you want to try yap-orange without installing anything.

**Move to the Desktop App** when you want your notes to persist reliably outside the browser and you prefer a native application.

**Run the Web + Server** when you need multi-user access, want to use the CLI for automation, or are deploying for a team.

Because the data format is the same everywhere, you are never locked in. Export from one mode, import into another, and keep working.
