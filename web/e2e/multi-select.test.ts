import { test, expect, type Page } from '@playwright/test'
import { screenshot } from './screenshot'

/**
 * Multi-select tests (SPA / WASM mode).
 *
 * Tests Ctrl+Click toggle, Shift+Click range select,
 * and Shift+Arrow extend selection.
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

async function selectBlock(page: Page, blockId: string) {
  await page.locator(`[data-block-id="${blockId}"] > .node-row .node-icon`).click()
}

async function getSelectedIds(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const els = document.querySelectorAll('.outliner-content .selected')
    return Array.from(els).map(el => el.getAttribute('data-block-id') ?? '')
  })
}

// ============================================================================
// Test 1: Ctrl+Click toggles blocks into/out of multi-select
// ============================================================================

test('Ctrl+Click toggles blocks in multi-select', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const a = await createBlock(page, { namespace: '', name: 'msel-a', content: 'a', position: '4080' })
  const b = await createBlock(page, { namespace: '', name: 'msel-b', content: 'b', position: '8080' })
  const c = await createBlock(page, { namespace: '', name: 'msel-c', content: 'c', position: 'c080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${a.block_id}"]`)).toBeVisible({ timeout: 5_000 })
  await expect(page.locator(`[data-block-id="${c.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Select a
  await selectBlock(page, a.block_id)
  await page.waitForTimeout(100)

  let selected = await getSelectedIds(page)
  expect(selected).toEqual([a.block_id])

  // Ctrl+Click on c → should add c to selection
  await page.locator(`[data-block-id="${c.block_id}"] > .node-row .node-icon`).click({
    modifiers: ['Control'],
  })
  await page.waitForTimeout(100)

  selected = await getSelectedIds(page)
  expect(selected).toContain(a.block_id)
  expect(selected).toContain(c.block_id)
  expect(selected).not.toContain(b.block_id)
  console.log(`  [test] Multi-select a+c: ${JSON.stringify(selected)}`)

  // Screenshot: two non-adjacent blocks selected via Ctrl+Click
  await screenshot(page, 'selecting-blocks', '01-ctrl-click-multi')

  // Ctrl+Click on a → should toggle a off, leaving only c
  await page.locator(`[data-block-id="${a.block_id}"] > .node-row .node-icon`).click({
    modifiers: ['Control'],
  })
  await page.waitForTimeout(100)

  selected = await getSelectedIds(page)
  expect(selected).not.toContain(a.block_id)
  expect(selected).toContain(c.block_id)
  console.log(`  [test] After toggling a off: ${JSON.stringify(selected)}`)
})

// ============================================================================
// Test 2: Shift+Click selects a contiguous range
// ============================================================================

test('Shift+Click selects a contiguous range', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const a = await createBlock(page, { namespace: '', name: 'range-a', content: 'a', position: '2080' })
  const b = await createBlock(page, { namespace: '', name: 'range-b', content: 'b', position: '4080' })
  const c = await createBlock(page, { namespace: '', name: 'range-c', content: 'c', position: '6080' })
  const d = await createBlock(page, { namespace: '', name: 'range-d', content: 'd', position: '8080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${d.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Select a (anchor)
  await selectBlock(page, a.block_id)
  await page.waitForTimeout(100)

  // Shift+Click on c → should select a, b, c
  await page.locator(`[data-block-id="${c.block_id}"] > .node-row .node-icon`).click({
    modifiers: ['Shift'],
  })
  await page.waitForTimeout(100)

  const selected = await getSelectedIds(page)
  console.log(`  [test] Shift-select a→c: ${JSON.stringify(selected)}`)
  expect(selected).toContain(a.block_id)
  expect(selected).toContain(b.block_id)
  expect(selected).toContain(c.block_id)
  expect(selected).not.toContain(d.block_id)

  // Screenshot: contiguous range selected via Shift+Click
  await screenshot(page, 'selecting-blocks', '02-shift-range-select')
})

// ============================================================================
// Test 3: Shift+ArrowDown extends selection downward
// ============================================================================

test('Shift+ArrowDown extends selection', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Use namespace to isolate from bootstrap blocks
  const parent = await createBlock(page, { namespace: '', name: 'ext-parent', position: '4080' })
  const a = await createBlock(page, { namespace: 'ext-parent', name: 'ext-a', content: 'a', position: '4080' })
  const b = await createBlock(page, { namespace: 'ext-parent', name: 'ext-b', content: 'b', position: '8080' })
  const c = await createBlock(page, { namespace: 'ext-parent', name: 'ext-c', content: 'c', position: 'c080' })

  await page.goto('/#/ext-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${c.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Select a
  await selectBlock(page, a.block_id)
  await page.waitForTimeout(100)

  // Shift+ArrowDown → should select a and b
  await page.keyboard.press('Shift+ArrowDown')
  await page.waitForTimeout(100)

  let selected = await getSelectedIds(page)
  console.log(`  [test] After Shift+Down 1: ${JSON.stringify(selected)}`)
  expect(selected).toContain(a.block_id)
  expect(selected).toContain(b.block_id)

  // Shift+ArrowDown again → should select a, b, c
  await page.keyboard.press('Shift+ArrowDown')
  await page.waitForTimeout(100)

  selected = await getSelectedIds(page)
  console.log(`  [test] After Shift+Down 2: ${JSON.stringify(selected)}`)
  expect(selected).toContain(a.block_id)
  expect(selected).toContain(b.block_id)
  expect(selected).toContain(c.block_id)

  // Screenshot: selection extended via Shift+ArrowDown
  await screenshot(page, 'selecting-blocks', '03-shift-arrow-extend')
})

// ============================================================================
// Test 4: Shift+ArrowUp extends selection upward
// ============================================================================

test('Shift+ArrowUp extends selection upward', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Use namespace to isolate from bootstrap blocks
  const parent = await createBlock(page, { namespace: '', name: 'extu-parent', position: '4080' })
  const a = await createBlock(page, { namespace: 'extu-parent', name: 'extu-a', content: 'a', position: '4080' })
  const b = await createBlock(page, { namespace: 'extu-parent', name: 'extu-b', content: 'b', position: '8080' })
  const c = await createBlock(page, { namespace: 'extu-parent', name: 'extu-c', content: 'c', position: 'c080' })

  await page.goto('/#/extu-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${c.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Select c (bottom)
  await selectBlock(page, c.block_id)
  await page.waitForTimeout(100)

  // Shift+ArrowUp → should select c and b
  await page.keyboard.press('Shift+ArrowUp')
  await page.waitForTimeout(100)

  let selected = await getSelectedIds(page)
  console.log(`  [test] After Shift+Up 1: ${JSON.stringify(selected)}`)
  expect(selected).toContain(b.block_id)
  expect(selected).toContain(c.block_id)

  // Shift+ArrowUp again → should select a, b, c
  await page.keyboard.press('Shift+ArrowUp')
  await page.waitForTimeout(100)

  selected = await getSelectedIds(page)
  console.log(`  [test] After Shift+Up 2: ${JSON.stringify(selected)}`)
  expect(selected).toContain(a.block_id)
  expect(selected).toContain(b.block_id)
  expect(selected).toContain(c.block_id)
})

// ============================================================================
// Test 5: Plain click after multi-select collapses to single selection
// ============================================================================

test('plain click after multi-select deselects others', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const a = await createBlock(page, { namespace: '', name: 'col-a', content: 'a', position: '4080' })
  const b = await createBlock(page, { namespace: '', name: 'col-b', content: 'b', position: '8080' })
  const c = await createBlock(page, { namespace: '', name: 'col-c', content: 'c', position: 'c080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${c.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Multi-select a and c
  await selectBlock(page, a.block_id)
  await page.waitForTimeout(50)
  await page.locator(`[data-block-id="${c.block_id}"] > .node-row .node-icon`).click({
    modifiers: ['Control'],
  })
  await page.waitForTimeout(100)

  let selected = await getSelectedIds(page)
  expect(selected.length).toBe(2)

  // Plain click on b → should deselect a and c, select only b
  await selectBlock(page, b.block_id)
  await page.waitForTimeout(100)

  selected = await getSelectedIds(page)
  expect(selected).toEqual([b.block_id])
})
