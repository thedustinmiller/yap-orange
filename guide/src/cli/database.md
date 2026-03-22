# Database Management

The `yap db` commands manage the PostgreSQL database directly. These commands connect to the database using the `DATABASE_URL` environment variable (defaulting to `postgres://yap:yap@localhost:5432/yap`).

> **Note:** These commands are only relevant when running the standalone server with an external PostgreSQL database. The desktop app (Tauri) manages its embedded Postgres automatically, and the browser SPA mode uses SQLite in WASM -- neither requires manual database management.

## Run Migrations

Apply any pending database migrations:

```bash
yap db migrate
```

This reads migration files from the `./migrations` directory and applies them in order using SQLx's built-in migrator. Already-applied migrations are skipped.

Output:

```
Successfully migrations applied
```

Run this after cloning the repo for the first time, or after pulling changes that include new migration files.

## Check Status

View which migrations have been applied and when:

```bash
yap db status
```

Output:

```
Database: connected

Applied migrations:
  20240101000000 initial_schema (2025-01-15 10:30:00)
  20240201000000 content_type_index (2025-01-20 14:15:00)
```

This is useful for verifying that your database schema is up to date, especially when debugging issues after an upgrade.

With `--json`:

```bash
yap --json db status
```

```json
{
  "connected": true,
  "migrations": [
    {
      "version": 20240101000000,
      "description": "initial_schema",
      "installed_on": "2025-01-15T10:30:00+00:00"
    }
  ]
}
```

## Reset Database

Drop all tables and re-run migrations from scratch:

```bash
yap db reset
```

This is a destructive operation. In interactive mode, it prompts for confirmation:

```
WARNING: This will delete all data. Continue? [y/N]
```

The reset drops the `edges`, `blocks`, `lineages`, `atoms`, and `_sqlx_migrations` tables (in that order, with `CASCADE`), then re-applies all migrations.

When running with `--json`, the confirmation prompt is skipped (useful for CI scripts, but be careful):

```bash
yap --json db reset
```

```json
{
  "status": "success",
  "message": "Database reset complete"
}
```

## Database URL

All `db` commands use the `DATABASE_URL` environment variable. You can set it in your `.env` file or pass it directly:

```bash
# Using .env file
echo 'DATABASE_URL=postgres://yap:yap@localhost:5432/yap' >> .env

# Or inline
DATABASE_URL=postgres://user:pass@host:5432/mydb yap db migrate
```

The default value is `postgres://yap:yap@localhost:5432/yap`, which matches the Docker Compose configuration included in the repository.

## Typical Setup Workflow

When setting up a new development environment:

```bash
# 1. Start PostgreSQL
docker compose up -d

# 2. Run migrations
yap db migrate

# 3. (Optional) Seed sample data
cargo xtask db reseed

# 4. Start the server
cargo run -p yap-server
```
