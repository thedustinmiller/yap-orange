# Contributing

You're very much welcome to contribute if you'd like, here's an overview.

See [QUICKSTART.md](./QUICKSTART.md) for getting your environment set up. Hopefully straightforward, td;dr:
- Rust 2024
- Node 22+
- Postgres (optional, docker compose setup included)
- wasm-pack for WASM

Just in case, there is a code of conduct that's adapted from the Contributor Covenant 3.0. Currently there's just one person, so enforcement will necessarily be ad hoc for now; but I hope that won't be an issue.

## Workflow

1. Fork off of the `prod` branch.
2. Make your changes.
3. Cover your changes with tests! If it's a bug fix your test should fail before and pass after.
4. Run full test suite to ensure functionality `cargo xtask test all`.
5. Lint; `cargo fmt`, `cargo clippy`, `npm run check`.
6. Submit pull request :)

[QUICKSTART.md](./QUICKSTART.md) also has some handy xtask commands to make life easier.

## Testing
I'm a bit particular about testing. If a feature isn't tested, does it really exist? So a few explanations.

### Property based testing
Property based testing is neat. The Rust side uses proptest, the JS side fast-check. Basically anything that we're depending on the accurate modelling, especially when things will be built on it, should have prop tests. 

### Backends
This project uses a Store trait to abstract the backends, and all backends should behave identically. Currently this means testing SQLite and Postgres versions. There's a store_tests! macro in yap-store-tests that puts whatever Store implementation through the same battery of tests.

### Accessibility Testing
Playwright + axe-core gives us as good of automated accessibility testing as we can get. This does NOT mean no test failures == accessible, but it's a start.

### Front end parity
We have four main front ends; OpenAPI HTTP, CLI, Tauri, and WASM SPA web app. All of them should expose the same functionality and their operations should be intercompatible. This does not mean they all need to work the same, a lot of the concepts don't make any sense going from web app to CLI, but in general someone should be able to achieve the same results from either. 

## Checks

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

Changes in in yap-core, yap-store-sqlite, or the WASM crates, need validation:

```bash
cargo check --manifest-path crates/yap-store-wasm/Cargo.toml --target wasm32-unknown-unknown
cargo check --manifest-path crates/yap-server-wasm/Cargo.toml --target wasm32-unknown-unknown
```
