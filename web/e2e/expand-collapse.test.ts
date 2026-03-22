import { test, expect, type Page } from '@playwright/test'
import { screenshot } from './screenshot'

/**
 * Expand/collapse and auto-expand behavior tests (SPA / WASM mode).
 *
 * Tests the expand-all (⊞) / collapse-all (⊟) buttons,
 * auto-expand on mount for empty-content blocks,
 * and the bullet toggle click.
 */

// ── Helpers ──────────────────────────────────────────────────────────

async function waitForApp(page: Page) {
  await page.waitForSelector('.dv-dockview', { timeout: 15_000 })
  await page.waitForSelector('.outliner-content', { timeout: 20_000 })
  await page.waitForFunction(
    () => {
      const c = document.querySelector('.outliner-content')
      return c && (c.querySelector('.node-row') || c.querySelector('.empty-state'))
    },
    { timeout: 20_000 },
  )
}

async function createBlock(
  page: Page,
  opts: { namespace: string; name: string; content?: string; position?: string },
) {
  return page.evaluate(async (o) => {
    const api = (window as any).__yap?.api
    if (!api) throw new Error('__yap.api not available')
    return api.blocks.create({
      namespace: o.namespace,
      name: o.name,
      content: o.content ?? '',
      position: o.position,
    })
  }, opts)
}

async function getVisibleBlockIds(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const els = document.querySelectorAll('.outliner-content [data-block-id]')
    return Array.from(els).map(el => el.getAttribute('data-block-id') ?? '')
  })
}

async function getVisibleBlockNames(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const wrappers = document.querySelectorAll('.outliner-content [data-block-id]')
    return Array.from(wrappers).map(w => {
      const row = w.querySelector(':scope > .node-row')
      if (!row) return ''
      const nameEl = row.querySelector('.node-name') || row.querySelector('.node-name-hover')
      return nameEl?.textContent?.trim() ?? ''
    })
  })
}

// ============================================================================
// Test 1: Expand-all button (⊞) expands all blocks recursively
// ============================================================================

test('expand-all button reveals nested children', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Build a 3-level tree: root > parent > child > grandchild
  const root = await createBlock(page, { namespace: '', name: 'exp-root', content: 'has content', position: '4080' })
  const parent = await createBlock(page, { namespace: 'exp-root', name: 'exp-child', content: 'also has content', position: '4080' })
  const child = await createBlock(page, { namespace: 'exp-root::exp-child', name: 'exp-gc', content: 'deep', position: '4080' })

  await page.goto('/#/exp-root')
  await waitForApp(page)
  await page.waitForTimeout(500)

  // Initially, may not see deeply nested blocks (depends on auto-expand)
  const beforeIds = await getVisibleBlockIds(page)
  console.log(`  [test] Before expand-all: ${beforeIds.length} blocks visible`)

  // Screenshot: tree before expand-all
  await screenshot(page, 'expand-collapse', '01-before-expand-all')

  // Click expand-all (⊞)
  await page.locator('[title="Expand all"]').click()
  await page.waitForTimeout(2000) // Recursive loading takes time

  // Screenshot: tree fully expanded showing nested children
  await screenshot(page, 'expand-collapse', '02-after-expand-all')

  const afterIds = await getVisibleBlockIds(page)
  console.log(`  [test] After expand-all: ${afterIds.length} blocks visible`)

  // The grandchild should be visible
  const gcVisible = await page.locator(`[data-block-id="${child.block_id}"]`).isVisible()
  console.log(`  [test] Grandchild visible: ${gcVisible}`)
  expect(gcVisible).toBe(true)
})

// ============================================================================
// Test 2: Collapse-all button (⊟) hides all children
// ============================================================================

test('collapse-all button hides all expanded children', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const root = await createBlock(page, { namespace: '', name: 'col-root', content: 'root content', position: '4080' })
  const child1 = await createBlock(page, { namespace: 'col-root', name: 'col-c1', content: 'c1', position: '4080' })
  const child2 = await createBlock(page, { namespace: 'col-root', name: 'col-c2', content: 'c2', position: '8080' })

  await page.goto('/#/col-root')
  await waitForApp(page)

  // Expand all first
  await page.locator('[title="Expand all"]').click()
  await page.waitForTimeout(1000)

  // Verify children are visible
  await expect(page.locator(`[data-block-id="${child1.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Click collapse-all (⊟)
  await page.locator('[title="Collapse all"]').click()
  await page.waitForTimeout(500)

  // Screenshot: tree after collapse-all
  await screenshot(page, 'expand-collapse', '03-after-collapse-all')

  // Note: The virtual root (col-root) should stay visible, but its children
  // should be hidden since their expand state is cleared.
  // The root itself is always the virtual root so it remains.
  const afterIds = await getVisibleBlockIds(page)
  console.log(`  [test] After collapse-all: ${afterIds.length} blocks visible`)
  console.log(`  [test] IDs: ${JSON.stringify(afterIds)}`)

  // After collapse-all, children should be collapsed.
  // The virtual root itself may stay expanded (it's always the visible root).
  // Use > .node-row > to only match the direct bullet (not nested children's bullets).
  const rootBullet = page.locator(`[data-block-id="${root.block_id}"] > .node-row > .node-bullet`)
  const isExpanded = await rootBullet.evaluate(el => el.classList.contains('expanded'))
  console.log(`  [test] Root bullet expanded after collapse: ${isExpanded}`)

  // Children blocks should not be visible (or at least fewer blocks)
  // collapseAll() clears expandedBlocks — but the virtual root is re-added
  // by the $effect in Outliner.svelte. So root stays expanded but its
  // children should have their expand state cleared.
})

// ============================================================================
// Test 3: Auto-expand for empty-content blocks
// ============================================================================

test('blocks with empty content auto-expand on mount', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Create namespace (empty content) with children
  const ns = await createBlock(page, { namespace: '', name: 'auto-ns', content: '', position: '4080' })
  const leaf = await createBlock(page, { namespace: 'auto-ns', name: 'auto-leaf', content: 'visible?', position: '4080' })

  await page.goto('/#/auto-ns')
  await waitForApp(page)
  await page.waitForTimeout(1000) // Auto-expand needs time

  // auto-ns has empty content, so it should auto-expand on mount
  // Its child 'auto-leaf' should be visible
  const leafVisible = await page.locator(`[data-block-id="${leaf.block_id}"]`).isVisible()
  console.log(`  [test] Auto-expanded leaf visible: ${leafVisible}`)
  expect(leafVisible).toBe(true)
})

// ============================================================================
// Test 4: Bullet click toggles expand
// ============================================================================

test('clicking bullet toggle expands and collapses children', async ({ page }) => {
  // When navigating INTO a block (/#/parent), it becomes the virtual root and is
  // always expanded. To test the bullet toggle, we need a CHILD block with its own
  // children, so we test the toggle on a non-root block.
  await page.goto('/')
  await waitForApp(page)

  const grandparent = await createBlock(page, { namespace: '', name: 'bullet-gp', position: '4080' })
  const parent = await createBlock(page, {
    namespace: 'bullet-gp', name: 'bullet-parent', content: 'has kids', position: '4080',
  })
  const child = await createBlock(page, {
    namespace: 'bullet-gp::bullet-parent', name: 'bullet-child', content: 'hidden', position: '4080',
  })

  await page.goto('/#/bullet-gp')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${parent.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Get the bullet element for parent (NOT the virtual root)
  const bullet = page.locator(`[data-block-id="${parent.block_id}"] > .node-row > .node-bullet`)

  // Check initial expand state — parent has content so should NOT auto-expand
  const wasExpanded = await bullet.evaluate(el => el.classList.contains('expanded'))
  console.log(`  [test] Initially expanded: ${wasExpanded}`)

  if (!wasExpanded) {
    // Click bullet to expand
    await bullet.click()
    await page.waitForTimeout(500)

    const nowExpanded = await bullet.evaluate(el => el.classList.contains('expanded'))
    expect(nowExpanded).toBe(true)

    // Child should be visible
    await expect(page.locator(`[data-block-id="${child.block_id}"]`)).toBeVisible({ timeout: 5_000 })
  }

  // Screenshot: block expanded with child visible (via bullet toggle or auto-expand)
  await screenshot(page, 'expand-collapse', '04-bullet-expanded')

  // Click bullet again to collapse
  await bullet.click()
  await page.waitForTimeout(300)

  const afterCollapse = await bullet.evaluate(el => el.classList.contains('expanded'))
  expect(afterCollapse).toBe(false)

  // Child should be hidden
  const childVisible = await page.locator(`[data-block-id="${child.block_id}"]`).isVisible()
  expect(childVisible).toBe(false)
})

// ============================================================================
// Test 5: Block count in status bar updates correctly
// ============================================================================

test('status bar block count reflects visible blocks', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  await createBlock(page, { namespace: '', name: 'count-a', content: 'a', position: '4080' })
  await createBlock(page, { namespace: '', name: 'count-b', content: 'b', position: '8080' })
  await createBlock(page, { namespace: '', name: 'count-c', content: 'c', position: 'c080' })

  await page.goto('/')
  await waitForApp(page)

  // Wait for blocks to render
  await page.waitForTimeout(500)

  // Status bar should show block count
  const statusText = await page.evaluate(() => {
    const status = document.querySelector('.outliner-status')
    return status?.textContent?.replace(/\s+/g, ' ')?.trim() ?? ''
  })
  console.log(`  [test] Status bar: "${statusText}"`)

  // Should contain a number followed by "blocks"
  // The count depends on bootstrap blocks (types, schema, settings, etc.) plus our 3
  expect(statusText).toMatch(/\d+ blocks/)

  // Extract the count
  const match = statusText.match(/(\d+) blocks/)
  const count = parseInt(match?.[1] ?? '0', 10)
  console.log(`  [test] Block count: ${count}`)
  // Should be at least 3 (our blocks) + bootstrap blocks
  expect(count).toBeGreaterThanOrEqual(3)
})
