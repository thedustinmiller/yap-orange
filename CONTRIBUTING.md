# Contributing to yap-orange

Contributions are welcome. This document covers how to set up the project, write good contributions, and submit changes.

See [QUICKSTART.md](./QUICKSTART.md) for environment setup.

## Code of Conduct

This project follows the [Contributor Covenant 3.0](./CODE_OF_CONDUCT.md). Please read it before participating.

## Getting Started

Prerequisites:

- **Rust** (stable toolchain)
- **Node.js** 22+
- **Docker** (for PostgreSQL in development mode)
- **wasm-pack** (optional, for WASM SPA builds)

Clone the repo and follow [QUICKSTART.md](./QUICKSTART.md) to get a working development environment.

## Development Workflow

1. Fork the repository and create a feature branch from `main`
2. Make your changes
3. Write tests covering your changes
4. Run the full test suite: `cargo xtask test all`
5. Run linting checks (see Code Quality below)
6. Submit a pull request referencing any related issues

Use `cargo xtask` for common development tasks:

```bash
cargo xtask db setup       # Run migrations
cargo xtask db reseed      # Clear + seed sample data
cargo xtask run server     # Start API server
cargo xtask run web        # Start Vite dev server
cargo xtask test all       # Run all tests (Rust + web)
cargo xtask check all      # cargo check + npm typecheck
```

## Testing Requirements

All pull requests must include tests. The project uses several testing strategies:

### Rust Tests

```bash
cargo xtask test rust                  # All Rust tests
cargo xtask test rust -- -p yap-core   # Specific crate
```

PostgreSQL integration tests require `DATABASE_URL` to be set (via `docker compose up -d` and `.env`). Without it, 28 PG-specific tests are ignored — SQLite variants still run.

### Frontend Tests

```bash
cargo xtask test unit    # Vitest unit tests
cargo xtask test e2e     # Playwright E2E tests
cargo xtask test web     # Both
```

### Property-Based Testing

- **Rust**: proptest is used in yap-core for link parsing round-trips and content serialization invariants
- **TypeScript**: fast-check is used in vitest for fractional indexing properties

When adding new pure functions (parsers, serializers, position logic), consider adding property tests.

### Cross-Backend Testing

The `store_tests!` macro in `crates/yap-store-tests/` generates identical test suites for each Store backend (SQLite and PostgreSQL). Changes to the `Store` trait or its implementations should be covered by tests in this crate.

### Accessibility Testing

- Playwright E2E tests include axe-core scans (`@axe-core/playwright`) for automated WCAG 2.1 AA checking
- The project maintains **zero** `svelte-ignore a11y_*` suppressions — do not add new ones
- All interactive elements must use native `<button>` elements with appropriate ARIA attributes

## Frontend Parity

yap-orange runs in four deployment modes: server + web UI, Tauri desktop, WASM SPA, and CLI. Features that touch the core data model or API should work across all modes:

- Store trait changes need test coverage in `yap-store-tests`
- API endpoint changes need corresponding CLI commands in `yap-cli`
- Frontend changes should work in both server-backed and WASM SPA modes

## Code Quality

Run these checks before submitting:

### Rust

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
```

### TypeScript / Svelte

```bash
cd web && npm run check    # svelte-check + tsc
```

### WASM Compilation

If you change code in yap-core, yap-store-sqlite, or the WASM crates, verify WASM compilation:

```bash
cargo check --manifest-path crates/yap-store-wasm/Cargo.toml --target wasm32-unknown-unknown
cargo check --manifest-path crates/yap-server-wasm/Cargo.toml --target wasm32-unknown-unknown
```

## Submitting Changes

- Open an issue or discussion before starting large features
- PRs should reference related issues
- Keep PRs focused — one logical change per PR
- Feature requests and discussions are welcome

This project is maintained in good faith. There is no entitlement to features or timeline commitments, but thoughtful contributions are appreciated.

## License

Contributions are licensed under the [GNU General Public License v3.0 or later](./LICENSE).
