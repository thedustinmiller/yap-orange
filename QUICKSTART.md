# Quick Start

Get a yap-orange development environment running. Choose the deployment mode that fits your needs.

## Prerequisites

| Tool | Version | Required For |
|------|---------|-------------|
| Rust | stable | All modes |
| Node.js | 22+ | Web UI |
| Docker | any | Server mode (PostgreSQL) |
| wasm-pack | any | WASM SPA mode only |

## Clone and Setup

```bash
git clone https://git.rcr.pub/dustin/yap-orange.git
cd yap-orange
cp .env.example .env
```

## Server + Web UI (Development Mode)

The standard development setup: PostgreSQL in Docker, Rust API server, Svelte frontend.

```bash
# Start PostgreSQL
docker compose up -d

# Run database migrations
cargo xtask db setup

# Terminal 1: Start the API server
cargo xtask run server

# Terminal 2: Start the web UI
cargo xtask run web
```

Open http://localhost:5173.

## Desktop App

Self-contained desktop application using Tauri. Embeds PostgreSQL via pg-embed — no Docker needed.

```bash
cd crates/yap-desktop && cargo tauri dev
```

First run downloads the PostgreSQL binary (~50 MB) to `~/.cache/pg-embed/`. Data is persisted to `~/.local/share/yap-orange/` (Linux) or `~/Library/Application Support/yap-orange/` (macOS).

## Browser SPA (WASM)

Runs the entire API server as WebAssembly inside the browser. No backend server or Docker needed. Data persists in browser storage (OPFS).

```bash
# Build the WASM module
cargo xtask build wasm

# Start the web UI
cd web && npm install && npm run dev
```

Open http://localhost:5173. The app auto-detects that no backend is running and boots in WASM mode.

## Running Tests

```bash
cargo xtask test all       # Everything (Rust + frontend)
cargo xtask test rust      # Rust workspace tests
cargo xtask test web       # Frontend tests (vitest + Playwright)
cargo xtask test unit      # Vitest unit tests only
cargo xtask test e2e       # Playwright E2E tests only
```

Rust tests require PostgreSQL for the full suite. Without `DATABASE_URL`, PG-specific tests are skipped and SQLite variants run instead.

## Seeding Sample Data

```bash
cargo xtask db reseed      # Clear database and load sample data
```

## CLI

```bash
# Run directly
cargo run -p yap-cli -- --help

# Or build and install
cargo xtask build cli
./target/debug/yap --help

# Set server URL (default: http://localhost:3000)
export YAP_SERVER_URL=http://localhost:3000
```

## Common xtask Commands

```bash
cargo xtask db setup       # Run migrations
cargo xtask db reseed      # Clear + seed sample data
cargo xtask run server     # Start API server
cargo xtask run web        # Start Vite dev server
cargo xtask run desktop    # Start Tauri desktop app
cargo xtask check all      # cargo check + npm typecheck
cargo xtask build all      # Build everything
```

## Next Steps

- [guide/](./guide/) — User guide (mdBook)
- [docs/architecture.md](./docs/architecture.md) — Data model, deployment modes, crate structure
- [docs/api.md](./docs/api.md) — HTTP API reference
- [docs/testing.md](./docs/testing.md) — Testing practices and E2E guide
- [CONTRIBUTING.md](./CONTRIBUTING.md) — How to contribute
