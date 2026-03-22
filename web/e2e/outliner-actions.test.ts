import { test, expect, type Page } from '@playwright/test'

/**
 * Outliner action tests (SPA / WASM mode).
 *
 * window.__yap.api is exposed in dev mode for programmatic tree setup.
 *
 * IMPORTANT: Clicking a block's .node-row in the center hits .node-content,
 * which calls enterEditMode(). To select in NAV mode, click the .node-icon
 * (which has no handler and bubbles to .node-row → handleRowClick → enterNavigationMode).
 */

/** Wait for the app to finish booting in SPA mode. */
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

/** Create a block via the in-browser API. */
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

/** Get a block's current state from the API. */
async function getBlock(page: Page, blockId: string) {
  return page.evaluate(async (id) => {
    const api = (window as any).__yap?.api
    return api.blocks.get(id)
  }, blockId)
}

/** Get the children of a block, sorted by position. */
async function getChildren(page: Page, parentId: string) {
  return page.evaluate(async (id) => {
    const api = (window as any).__yap?.api
    return api.blocks.children(id)
  }, parentId)
}

/** Get visible block names in DOM order. */
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

/**
 * Select a block in NAVIGATION mode by clicking its icon.
 *
 * Clicking .node-icon has no dedicated handler — the event bubbles up to
 * .node-row's onclick={handleRowClick} which calls enterNavigationMode(node.id).
 *
 * DO NOT click .node-content — that enters edit mode via handleContentClick().
 */
async function selectBlock(page: Page, blockId: string) {
  await page.locator(`[data-block-id="${blockId}"] > .node-row .node-icon`).click()
}

// ============================================================================
// Test 1: Outdent positions block after its former parent, not at the end
// ============================================================================

test('outdent places block after its former parent (not at end)', async ({ page }) => {
  //   Before:          parent > [child-1 > [gc1, gc2], child-2]
  //   Expected after:  parent > [child-1 > [gc1], gc2, child-2]

  await page.goto('/')
  await waitForApp(page)

  // --- Build the tree via API ---
  const parent = await createBlock(page, { namespace: '', name: 'parent' })
  const child1 = await createBlock(page, {
    namespace: 'parent', name: 'child-1', position: '4080',
  })
  await createBlock(page, {
    namespace: 'parent', name: 'child-2', position: 'c080',
  })
  await createBlock(page, {
    namespace: 'parent::child-1', name: 'grandchild-1', position: '4080',
  })
  const gc2 = await createBlock(page, {
    namespace: 'parent::child-1', name: 'grandchild-2', position: 'c080',
  })

  // Verify initial state: gc2 is a child of child-1
  let gc2State = await getBlock(page, gc2.block_id)
  expect(gc2State.parent_id).toBe(child1.block_id)

  // --- Navigate into parent namespace ---
  await page.goto('/#/parent')
  await waitForApp(page)

  // Expand child-1: select via icon click, then ArrowRight
  await selectBlock(page, child1.block_id)
  await page.waitForTimeout(100)
  await page.keyboard.press('ArrowRight')
  await page.waitForTimeout(500)

  // Verify gc2 is visible
  await expect(page.locator(`[data-block-id="${gc2.block_id}"] > .node-row`))
    .toBeVisible({ timeout: 5_000 })

  let names = await getVisibleBlockNames(page)
  console.log(`  [test] Before outdent: ${JSON.stringify(names)}`)

  // Verify we're in navigate mode
  const modeBefore = await page.evaluate(() => {
    const selected = document.querySelectorAll('.outliner-content .selected')
    const editing = document.querySelectorAll('.outliner-content .editing')
    return { selected: selected.length, editing: editing.length }
  })
  console.log(`  [test] Mode state: ${JSON.stringify(modeBefore)}`)

  // --- Select gc2 via icon click (NAV mode) and Shift+Tab ---
  await selectBlock(page, gc2.block_id)
  await page.waitForTimeout(200)

  // Verify gc2 is selected and NOT in edit mode
  const gc2Wrapper = page.locator(`[data-block-id="${gc2.block_id}"]`)
  await expect(gc2Wrapper).toHaveClass(/selected/)
  const isEditing = await gc2Wrapper.evaluate(el => el.classList.contains('editing'))
  console.log(`  [test] gc2 selected, editing=${isEditing}`)
  expect(isEditing).toBe(false)

  // Shift+Tab to outdent
  await page.keyboard.press('Shift+Tab')
  await page.waitForTimeout(2000)

  // --- Verify via API that gc2 was actually moved ---
  gc2State = await getBlock(page, gc2.block_id)
  console.log(`  [test] gc2 parent after outdent: ${gc2State.parent_id} (parent: ${parent.block_id})`)

  // gc2 should now be a child of parent (not child-1)
  expect(gc2State.parent_id).toBe(parent.block_id)

  // --- Verify the order of parent's children ---
  const parentChildren = await getChildren(page, parent.block_id)
  const childOrder = parentChildren.map((c: any) => c.name)
  console.log(`  [test] Parent's children order: ${JSON.stringify(childOrder)}`)

  // EXPECTED: [child-1, grandchild-2, child-2]
  const c1Idx = childOrder.indexOf('child-1')
  const gc2CIdx = childOrder.indexOf('grandchild-2')
  const c2Idx = childOrder.indexOf('child-2')

  expect(gc2CIdx).toBeGreaterThan(c1Idx)
  expect(gc2CIdx).toBeLessThan(c2Idx)
})

// ============================================================================
// Test 2: Delete key in nav mode should delete selected blocks
// ============================================================================

test('delete key in nav mode removes selected block', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Create a test block via quick-create UI
  await page.locator('[title="Create block"]').click()
  const input = page.locator('.quick-create-input')
  await expect(input).toBeVisible({ timeout: 2_000 })
  await input.click()
  await input.pressSequentially('delete-me', { delay: 30 })
  await input.press('Enter')
  await page.waitForTimeout(500)

  // The block should appear
  const blockRow = page.locator('.node-row', { hasText: 'delete-me' })
  await expect(blockRow).toBeVisible({ timeout: 5_000 })

  // Get block ID from DOM
  const blockId = await blockRow.evaluate(el => {
    return el.closest('[data-block-id]')?.getAttribute('data-block-id') ?? ''
  })
  expect(blockId).toBeTruthy()

  // Select via icon click (not content — that enters edit mode)
  await selectBlock(page, blockId)
  await page.waitForTimeout(200)

  const wrapper = page.locator(`[data-block-id="${blockId}"]`)
  await expect(wrapper).toHaveClass(/selected/)

  // Verify NOT in edit mode
  const isEditing = await wrapper.evaluate(el => el.classList.contains('editing'))
  console.log(`  [test] Block selected, editing=${isEditing}`)
  expect(isEditing).toBe(false)

  const countBefore = await page.locator('.outliner-content [data-block-id]').count()
  console.log(`  [test] Blocks before delete: ${countBefore}`)

  // Press Delete
  await page.keyboard.press('Delete')
  await page.waitForTimeout(1000)

  const countAfter = await page.locator('.outliner-content [data-block-id]').count()
  console.log(`  [test] Blocks after delete: ${countAfter}`)

  // Block should be gone
  await expect(wrapper).not.toBeVisible({ timeout: 3_000 })
  expect(countAfter).toBeLessThan(countBefore)
})
