# Testing Practices

## E2E Tests (Playwright)

### Stack

E2E tests run against the **WASM SPA mode** — no backend server needed. The Vite dev server starts automatically, and when no backend is running on port 3000, the app auto-detects and boots an in-browser WASM worker with SQLite + OPFS persistence.

- **Framework**: Playwright (chromium)
- **Config**: `web/playwright.config.ts`
- **Test dir**: `web/e2e/`
- **Run**: `cd web && npm run test:e2e`

### Data Isolation

Each Playwright test gets a fresh `BrowserContext` with its own storage partition. OPFS data is isolated per context, so tests don't share state — no explicit cleanup needed.

Bootstrap blocks (`types`, `schema`, `settings`, `Settings Theme`) are always present on a fresh database via `ensure_meta_schema()` and `ensure_settings()`.

### Programmatic Block Creation

For tests that need a specific tree structure, use the `window.__yap.api` bridge (exposed in dev mode by `api.ts`):

```ts
const block = await page.evaluate(async (opts) => {
  return (window as any).__yap.api.blocks.create(opts)
}, { namespace: 'parent', name: 'child', content: '' })
```

This routes through the same WASM worker as the UI, so blocks are immediately available. Use fractional position strings (e.g. `'4080'`, `'c080'`) to control ordering.

### Click Targets Matter

The OutlinerNode has overlapping click zones:

| Zone | Handler | Effect |
|------|---------|--------|
| `.node-content` | `handleContentClick` | Enters **edit mode** (`e.stopPropagation()`) |
| `.node-bullet` | `handleToggleExpand` | Toggles expand (`e.stopPropagation()`) |
| `.node-icon` | *(none — bubbles to row)* | Enters **nav mode** via `handleRowClick` |
| `.node-indent` | *(none — bubbles to row)* | Enters **nav mode** via `handleRowClick` |
| `.center-btn` | `handleCenterOn` | Navigates into block (`e.stopPropagation()`) |

**To select a block in navigation mode**, click `.node-icon` — it has no dedicated handler, so the click bubbles to `.node-row` → `handleRowClick` → `enterNavigationMode(node.id)`.

Clicking the center of `.node-row` (Playwright's default) lands on `.node-content`, which enters edit mode. In edit mode, the outliner ignores all keyboard shortcuts (`handleKeydown` returns early). This silently breaks Tab/Shift+Tab, Delete, arrow navigation, etc.

```ts
// WRONG — enters edit mode:
await page.locator(`[data-block-id="${id}"] > .node-row`).click()

// RIGHT — enters nav mode:
await page.locator(`[data-block-id="${id}"] > .node-row .node-icon`).click()
```

### Known Focus Bug

The `.outliner` div has `onclick={() => { if (mode === 'navigate') containerEl?.focus(); }}` (line 378). Click events from child inputs bubble up to this handler, which steals focus back to the container div. This means clicking the quick-create input (or any future input in the outliner) focuses on mousedown but unfocuses on mouseup.

The `create-block.test.ts` characterizes this with `page.mouse.down()`/`.up()` to demonstrate the exact sequence.

### Drag-and-Drop Testing

Playwright's `locator.dragTo(target)` works for single-block HTML5 DnD in this app. Use `targetPosition` to control which drop zone (above/below/inside) is triggered:

```ts
const targetBox = await targetRow.boundingBox()
await sourceRow.dragTo(targetRow, {
  targetPosition: { x: targetBox.width / 2, y: targetBox.height * 0.9 }, // 'below' zone
})
```

Drop zones: top 25% = above, middle 50% = inside, bottom 25% = below (see `handleDragOver`).

For multi-block drag testing, the `handleDrop` function reads block IDs from `dataTransfer.getData('application/yap-block-ids')`. Since Playwright can't easily set custom dataTransfer data, multi-block drag tests simulate the behavior by calling `api.blocks.move()` in the same sequence as `handleDrop`.

Multi-block drops compute sequential positions: the first block gets the standard drop position, and each subsequent block is positioned after the previous one. This ensures all blocks get distinct, ordered positions.

### Namespace Isolation for Tests

Bootstrap blocks (`types`, `schema`, `settings`, `Settings Theme`) are present in every fresh database. Tests that use arrow-key navigation, selection, or block counting at the **root level** will see these blocks interspersed with test blocks.

**Always create a parent namespace** and navigate into it (`/#/namespace`) to isolate test blocks:

```ts
const parent = await createBlock(page, { namespace: '', name: 'test-ns', position: '4080' })
const a = await createBlock(page, { namespace: 'test-ns', name: 'a', content: 'a', position: '4080' })
await page.goto('/#/test-ns')
```

### Content Loading Is Lazy

At root level, blocks are visible but their **content is NOT loaded**. Content only loads eagerly when navigating INTO a namespace (via `loadChildrenWithContent`). Tests that check `.content-text` rendering must navigate into a parent namespace.

### Virtual Root in Outliner

When navigated into a block (`/#/parent`), that block becomes the **virtual root** at index [0] in `flatNodes`. It is always expanded (via the `$effect` in Outliner.svelte). Arrow navigation from the first child block goes UP to the virtual root, not stopping at the first child.

### Sidebar Buttons Are Hidden Until Hover

The bookmark (☆/★) and delete (✕) buttons on sidebar tree items have `opacity: 0` until the row is hovered. Use `{ force: true }` when clicking them:

```ts
await sidebarItem.hover()
await sidebarItem.locator('.tree-delete').click({ force: true })
```

### Accessibility Testing

`web/e2e/accessibility.test.ts` uses `@axe-core/playwright` to run automated WCAG 2.1 AA audits against major app states (initial load, outliner with tree, edit mode, context menu, settings view, sidebar, color contrast, keyboard/focus, ARIA and landmarks).

Each test runs `AxeBuilder.analyze()` and prints a detailed violation report grouped by impact level (`critical`, `serious`, `moderate`, `minor`). The tests always pass but produce console output — once violations are resolved the assertions can be uncommented to enforce a zero-violation target.

The zero `svelte-ignore a11y_*` suppressions policy means all accessibility warnings must be fixed at the source rather than silenced.

### Vite Proxy Noise

When no backend is running, Vite logs `http proxy error: /api/debug/logs` and `/health` every second. This is harmless — the debug log poller and health check both fail gracefully, and the app falls back to WASM mode. The noise can be ignored.

## Property-Based Testing

Property-based tests generate hundreds of random inputs to find edge cases that hand-written examples miss.

**Rust** (`yap-core`): Uses the `proptest` crate. Key properties tested:
- `parse_links(format_link(segments))` recovers the original segments (parse/format round-trip symmetry)
- `compute_export_hash` is deterministic and order-independent for internal link IDs

**TypeScript** (`web/`): Uses `fast-check` in vitest. Key properties tested:
- Fractional index insertion ordering: for any two positions `a < b`, a position inserted between them satisfies `a < mid < b`
- Position generation never produces duplicates for sequential inserts

Property testing is especially valuable here because both the link format and fractional index ordering are invariants the rest of the system depends on silently — a bug in either corrupts data without a visible error.

## Unit Tests (Vitest)

- **Framework**: Vitest + fast-check (property-based testing)
- **Config**: `web/vitest.config.ts`
- **Test files**: `web/src/lib/*.test.ts`
- **Run**: `cd web && npm test`

Covers pure logic: fractional indexing (`position.test.ts`), link parsing, content segmentation (`content.test.ts`).

## Cross-Backend Testing

The `yap-store-tests` crate defines a `store_tests!` macro that expands an identical test suite for each registered Store backend. This ensures behavioral parity between the PostgreSQL and SQLite implementations.

Tests cover: health check, atom CRUD, atom link round-trips, block hierarchy operations, move safety (cycle detection, move-to-deleted-parent), edge creation/deduplication, and export/import flows.

SQLite variants run without any external dependencies. PostgreSQL variants are conditionally ignored when `DATABASE_URL` is not set (28 tests in CI without a database). To run PG tests locally:

```bash
docker compose up -d
cargo test -p yap-store-tests
```

## Rust Tests

- **Run**: `cargo test` (all crates) or `cargo test -p yap-core` (specific)
- **Core tests**: `crates/yap-core/src/` (links, content, models, hash, export)
- **Integration tests**: `crates/yap-core/tests/export_tests.rs`
- **CLI tests**: `crates/yap-cli/src/main.rs` (utility functions: UUID parsing, tree building, truncation)

Backend tests require a running PostgreSQL database (via `docker compose up -d`).

Store integration tests (`crates/yap-store-tests/`) cover single-block move, cycle detection (`is_move_safe`), and move-to-deleted-parent. API tests cover move endpoint status codes and link resolution after move.

**Coverage gap**: No Rust tests for moving multiple blocks sequentially or what happens when multiple blocks share the same position string.
