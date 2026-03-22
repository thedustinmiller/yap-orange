# CLI Overview

The `yap` command-line interface is an HTTP client that talks to a running yap-orange server. It does not access the database directly -- all operations go through the REST API.

## Server Connection

By default the CLI connects to `http://localhost:3000`. Override this with the `--server-url` flag or the `YAP_SERVER_URL` environment variable:

```bash
# Flag (per-command)
yap --server-url http://myhost:4000 block list

# Environment variable (persistent)
export YAP_SERVER_URL=http://myhost:4000
yap block list
```

Make sure the server is running before using the CLI:

```bash
# Start the server (in another terminal)
cargo run -p yap-server

# Or via xtask
cargo xtask run server
```

## General Usage

Every command follows the pattern:

```
yap [--server-url URL] [--json] <subcommand> [subcommand] [options]
```

Top-level subcommands:

| Command  | Description |
|----------|-------------|
| `block`  | Create, read, update, delete, move, and tree-render blocks |
| `atom`   | Inspect atom content, backlinks, and graph neighborhoods |
| `edge`   | Create, list, and delete semantic edges between lineages |
| `ns`     | Create namespaces and view the namespace tree |
| `link`   | Resolve wiki-link paths to lineage and block IDs |
| `schema` | Manage custom type definitions |
| `search` | Search blocks by name or namespace path |
| `export` | Export a subtree to portable JSON |
| `import` | Import a subtree from JSON |
| `db`     | Run migrations, check status, or reset the database |

## Getting Help

Every command and subcommand supports `--help`:

```bash
yap --help              # Top-level help
yap block --help        # All block subcommands
yap block create --help # Flags for block create
yap export --help       # Export options
```

## JSON Output

Pass `--json` (a global flag) to any command for machine-readable JSON output. This is useful for scripting and piping into tools like `jq`:

```bash
# Human-readable (default)
yap block get 019414a0-b1c2-7def-8000-000000000001

# JSON output
yap --json block get 019414a0-b1c2-7def-8000-000000000001

# Pipe to jq
yap --json block list --namespace research | jq '.[].name'
```

When `--json` is active, errors are also returned as JSON objects with an `"error"` field instead of printing to stderr.

## IDs

yap-orange uses UUIDv7 identifiers (time-sortable). Most commands that accept an ID expect a full UUID string like `019414a0-b1c2-7def-8000-000000000001`. Some commands (like `export` and `import --parent`) also accept namespace paths, which are resolved to block IDs automatically.

## Content from Stdin

Commands that accept content (`block create`, `block update`) can read from stdin by passing `-` as the content argument, or by piping input when no content argument is given:

```bash
# Explicit stdin
echo "Hello world" | yap block create - --namespace notes --name greeting

# Pipe detection (no content argument)
cat document.md | yap block create --namespace docs --name readme
```
