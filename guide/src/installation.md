# Installation

yap-orange can run in three modes. Pick the one that fits your setup.

| Mode | What you need | Database | Best for |
|------|--------------|----------|----------|
| Web + Server | Docker, Rust, Node.js | PostgreSQL (Docker) | Development, teams |
| Desktop App | Rust, Tauri prerequisites | Embedded PostgreSQL | Single-user, offline |
| Browser SPA | Rust (wasm target), wasm-pack, Node.js | SQLite in browser | Zero-install trial, portability |

## Web + Server (Development Mode)

This is the standard development setup: PostgreSQL in Docker, the Axum API server, and the Svelte frontend via Vite.

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) and Docker Compose
- [Rust toolchain](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) (v18+) and npm

### Steps

1. **Start PostgreSQL:**

   ```bash
   docker compose up -d
   ```

2. **Configure environment:**

   ```bash
   cp .env.example .env
   ```

   The defaults work out of the box with the Docker Compose setup. See [Environment Variables](#environment-variables) below if you need to customize.

3. **Run database migrations:**

   ```bash
   cargo run -p yap-cli -- db migrate
   ```

4. **Start the API server** (runs on port 3000 by default):

   ```bash
   cargo run -p yap-server
   ```

5. **Start the web frontend** (in a separate terminal, runs on port 5173):

   ```bash
   cd web && npm install && npm run dev
   ```

6. Open [http://localhost:5173](http://localhost:5173) in your browser.

### Seeding sample data

To populate the database with example content for testing:

```bash
cargo xtask db reseed
```

## Desktop App

The desktop app bundles everything into a single Tauri window. It downloads and manages its own PostgreSQL instance automatically.

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable)
- Tauri prerequisites for your platform -- see the [Tauri v2 prerequisites guide](https://v2.tauri.app/start/prerequisites/)

### Steps

1. **Launch the app:**

   ```bash
   cd crates/yap-desktop && cargo tauri dev
   ```

2. On first run, the app downloads the PostgreSQL binary (~50 MB). This is cached for future launches.

3. The app starts an embedded PostgreSQL instance and the Axum server internally -- no Docker or external database needed.

### Data storage

| Platform | Data directory |
|----------|---------------|
| Linux | `~/.local/share/yap-orange/` |
| macOS | `~/Library/Application Support/yap-orange/` |

The PostgreSQL binary cache lives at `~/.cache/pg-embed/`.

## Browser SPA (No Server)

The browser SPA compiles the entire server to WebAssembly. It runs in a Web Worker inside your browser, using SQLite with OPFS for persistent storage. No backend server is needed.

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable) with the `wasm32-unknown-unknown` target
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)
- [Node.js](https://nodejs.org/) (v18+) and npm

Add the WASM target if you don't have it:

```bash
rustup target add wasm32-unknown-unknown
```

Install wasm-pack:

```bash
cargo install wasm-pack
```

### Steps

1. **Build the WASM bundle:**

   ```bash
   cd crates/yap-server-wasm && wasm-pack build --target web --out-dir ../../web/public/wasm --out-name yap_server_wasm
   ```

2. **Start the frontend:**

   ```bash
   cd web && npm install && npm run dev
   ```

3. Open [http://localhost:5173](http://localhost:5173). The frontend auto-detects that no API server is running and starts the WASM worker instead.

### Notes

- Data persists in the browser's [Origin Private File System (OPFS)](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system). Clearing site data will erase your notes.
- OPFS requires a [secure context](https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts) (HTTPS or localhost).
- Performance is good for personal use. For large datasets, the server mode with PostgreSQL will be faster.

## CLI Setup

The CLI communicates with a running yap-orange server over HTTP. It does not access the database directly.

### Prerequisites

A running yap-orange server (either the standalone server or the desktop app).

### Configuration

Tell the CLI where to find the server:

```bash
export YAP_SERVER_URL=http://localhost:3000
```

Or pass it per-command:

```bash
cargo run -p yap-cli -- --server-url http://localhost:3000 block list
```

### Verify the connection

```bash
cargo run -p yap-cli -- block list
```

For a full list of commands:

```bash
cargo run -p yap-cli -- --help
```

## Environment Variables

Configure these in your `.env` file or export them in your shell.

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://yap:yap@localhost:5432/yap` | PostgreSQL connection string (server mode) |
| `SERVER_HOST` | `127.0.0.1` | Address the API server binds to |
| `SERVER_PORT` | `3000` | Port the API server listens on |
| `YAP_SERVER_URL` | `http://localhost:3000` | Server URL for the CLI client |
| `RUST_LOG` | `info` | Log level filter ([tracing syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)) |

### Example `.env`

```env
DATABASE_URL=postgres://yap:yap@localhost:5432/yap
SERVER_HOST=127.0.0.1
SERVER_PORT=3000
RUST_LOG=info,yap_server=debug
```

## Verifying your setup

Regardless of which mode you chose, once the frontend is open you should see the outliner panel on the left. If you seeded sample data, it will show a tree of example blocks. If not, you'll see an empty workspace ready for your first note.

Next, learn about the [core concepts](./concepts/index.md) that underpin yap-orange's data model.
