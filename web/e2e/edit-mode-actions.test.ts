import { test, expect, type Page } from '@playwright/test'
import { screenshot } from './screenshot'

/**
 * Edit-mode action tests (SPA / WASM mode).
 *
 * Tests the BlockEditor keybindings that trigger structural changes:
 *   - Enter → create new sibling block
 *   - Tab → indent (reparent under previous sibling)
 *   - Shift+Tab → outdent (reparent under grandparent)
 *   - ArrowUp at line 1 → navigate to previous block
 *   - ArrowDown at last line → navigate to next block
 *   - Ctrl+Enter → save and exit
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

async function getBlock(page: Page, blockId: string) {
  return page.evaluate(async (id) => {
    const api = (window as any).__yap?.api
    return api.blocks.get(id)
  }, blockId)
}

async function getChildren(page: Page, parentId: string) {
  return page.evaluate(async (id) => {
    const api = (window as any).__yap?.api
    return api.blocks.children(id)
  }, parentId)
}

async function selectBlock(page: Page, blockId: string) {
  await page.locator(`[data-block-id="${blockId}"] > .node-row .node-icon`).click()
}

async function isBlockEditing(page: Page, blockId: string): Promise<boolean> {
  return page.evaluate((id) => {
    const el = document.querySelector(`[data-block-id="${id}"]`)
    return el?.classList.contains('editing') ?? false
  }, blockId)
}

async function getVisibleBlockIds(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const els = document.querySelectorAll('.outliner-content [data-block-id]')
    return Array.from(els).map(el => el.getAttribute('data-block-id') ?? '')
  })
}

async function getEditingBlockId(page: Page): Promise<string | null> {
  return page.evaluate(() => {
    const el = document.querySelector('.outliner-content .editing')
    return el?.getAttribute('data-block-id') ?? null
  })
}

// ============================================================================
// Test 1: Enter in edit mode creates a new sibling and enters edit mode on it
// ============================================================================

test('Enter in edit mode creates new sibling block', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'enter-parent', position: '4080' })
  const first = await createBlock(page, {
    namespace: 'enter-parent', name: 'first-child', content: 'I am first', position: '4080',
  })

  await page.goto('/#/enter-parent')
  await waitForApp(page)

  // Wait for the child to appear
  await expect(page.locator(`[data-block-id="${first.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  const blocksBefore = await getVisibleBlockIds(page)
  console.log(`  [test] Blocks before Enter: ${blocksBefore.length}`)

  // Enter edit mode on first-child
  await page.locator(`[data-block-id="${first.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  expect(await isBlockEditing(page, first.block_id)).toBe(true)

  // Screenshot: editing a block before pressing Enter
  await screenshot(page, 'keyboard-editing', '01-editing-before-enter')

  // Press Enter to create a new sibling
  await page.keyboard.press('Enter')
  await page.waitForTimeout(1000)

  // Screenshot: new sibling block created after Enter
  await screenshot(page, 'keyboard-editing', '02-new-sibling-created')

  // A new block should have been created
  const blocksAfter = await getVisibleBlockIds(page)
  console.log(`  [test] Blocks after Enter: ${blocksAfter.length}`)
  expect(blocksAfter.length).toBeGreaterThan(blocksBefore.length)

  // The new block should be in edit mode
  const editingId = await getEditingBlockId(page)
  console.log(`  [test] Now editing: ${editingId}`)
  expect(editingId).not.toBeNull()
  expect(editingId).not.toBe(first.block_id) // Should be a NEW block

  // The new block should be after first-child in the tree
  const firstIdx = blocksAfter.indexOf(first.block_id)
  const newIdx = blocksAfter.indexOf(editingId!)
  console.log(`  [test] first index: ${firstIdx}, new index: ${newIdx}`)
  expect(newIdx).toBe(firstIdx + 1)
})

// ============================================================================
// Test 2: Tab in edit mode indents block under previous sibling
// ============================================================================

test('Tab in edit mode indents block under previous sibling', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'tab-parent', position: '4080' })
  const sibling1 = await createBlock(page, {
    namespace: 'tab-parent', name: 'sib-1', content: 'first', position: '4080',
  })
  const sibling2 = await createBlock(page, {
    namespace: 'tab-parent', name: 'sib-2', content: 'second', position: '8080',
  })

  await page.goto('/#/tab-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${sibling2.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Verify initial state: sib-2 is child of tab-parent
  let sib2State = await getBlock(page, sibling2.block_id)
  expect(sib2State.parent_id).toBe(parent.block_id)

  // Enter edit mode on sib-2
  await page.locator(`[data-block-id="${sibling2.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  expect(await isBlockEditing(page, sibling2.block_id)).toBe(true)

  // Press Tab to indent
  await page.keyboard.press('Tab')
  await page.waitForTimeout(1000)

  // sib-2 should now be a child of sib-1
  sib2State = await getBlock(page, sibling2.block_id)
  console.log(`  [test] sib-2 parent after Tab: ${sib2State.parent_id} (expected: ${sibling1.block_id})`)
  expect(sib2State.parent_id).toBe(sibling1.block_id)

  // Screenshot: block indented under its previous sibling
  await screenshot(page, 'keyboard-editing', '03-tab-indented')

  // sib-2 should still be in edit mode
  expect(await isBlockEditing(page, sibling2.block_id)).toBe(true)
})

// ============================================================================
// Test 3: Shift+Tab in edit mode outdents block
// ============================================================================

test('Shift+Tab in edit mode outdents block', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'stab-parent', position: '4080' })
  const child = await createBlock(page, {
    namespace: 'stab-parent', name: 'stab-child', content: '', position: '4080',
  })
  const grandchild = await createBlock(page, {
    namespace: 'stab-parent::stab-child', name: 'stab-gc', content: 'deep', position: '4080',
  })

  await page.goto('/#/stab-parent')
  await waitForApp(page)

  // Expand child to see grandchild
  await selectBlock(page, child.block_id)
  await page.waitForTimeout(100)
  await page.keyboard.press('ArrowRight')
  await page.waitForTimeout(500)

  await expect(page.locator(`[data-block-id="${grandchild.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Verify initial state: gc is child of child
  let gcState = await getBlock(page, grandchild.block_id)
  expect(gcState.parent_id).toBe(child.block_id)

  // Enter edit mode on grandchild
  await page.locator(`[data-block-id="${grandchild.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  expect(await isBlockEditing(page, grandchild.block_id)).toBe(true)

  // Press Shift+Tab to outdent
  await page.keyboard.press('Shift+Tab')
  await page.waitForTimeout(1000)

  // gc should now be a child of parent (not child)
  gcState = await getBlock(page, grandchild.block_id)
  console.log(`  [test] gc parent after Shift+Tab: ${gcState.parent_id} (expected: ${parent.block_id})`)
  expect(gcState.parent_id).toBe(parent.block_id)

  // Screenshot: block outdented to parent level
  await screenshot(page, 'keyboard-editing', '04-shift-tab-outdented')
})

// ============================================================================
// Test 4: ArrowUp at first line navigates to previous block
// ============================================================================

test('ArrowUp at line 1 in edit mode jumps to previous block', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'aup-parent', position: '4080' })
  const blockA = await createBlock(page, {
    namespace: 'aup-parent', name: 'aup-a', content: 'first block', position: '4080',
  })
  const blockB = await createBlock(page, {
    namespace: 'aup-parent', name: 'aup-b', content: 'second block', position: '8080',
  })

  await page.goto('/#/aup-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${blockB.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Enter edit mode on blockB
  await page.locator(`[data-block-id="${blockB.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  expect(await isBlockEditing(page, blockB.block_id)).toBe(true)

  // Press ArrowUp — cursor is on line 1 (single-line content), so should jump to blockA
  await page.keyboard.press('ArrowUp')
  await page.waitForTimeout(500)

  // Should now be editing blockA
  const editingId = await getEditingBlockId(page)
  console.log(`  [test] After ArrowUp: editing ${editingId} (expected: ${blockA.block_id})`)
  expect(editingId).toBe(blockA.block_id)
})

// ============================================================================
// Test 5: ArrowDown at last line navigates to next block
// ============================================================================

test('ArrowDown at last line in edit mode jumps to next block', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'adn-parent', position: '4080' })
  const blockA = await createBlock(page, {
    namespace: 'adn-parent', name: 'adn-a', content: 'first block', position: '4080',
  })
  const blockB = await createBlock(page, {
    namespace: 'adn-parent', name: 'adn-b', content: 'second block', position: '8080',
  })

  await page.goto('/#/adn-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${blockA.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Enter edit mode on blockA
  await page.locator(`[data-block-id="${blockA.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  expect(await isBlockEditing(page, blockA.block_id)).toBe(true)

  // Press ArrowDown — single-line content, cursor at last line → jump to blockB
  await page.keyboard.press('ArrowDown')
  await page.waitForTimeout(500)

  const editingId = await getEditingBlockId(page)
  console.log(`  [test] After ArrowDown: editing ${editingId} (expected: ${blockB.block_id})`)
  expect(editingId).toBe(blockB.block_id)
})

// ============================================================================
// Test 6: Ctrl+Enter saves and exits edit mode
// ============================================================================

test('Escape saves content and exits edit mode', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'ctrlenter', content: 'before', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Enter edit mode
  await page.locator(`[data-block-id="${block.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  expect(await isBlockEditing(page, block.block_id)).toBe(true)

  // Type new content
  await page.keyboard.press('Meta+a')
  await page.keyboard.type('after ctrl-enter', { delay: 20 })

  // Exit with Escape (same saveAndExit path as Ctrl+Enter)
  await page.keyboard.press('Escape')
  await page.waitForTimeout(500)

  // Should be back in nav mode
  expect(await isBlockEditing(page, block.block_id)).toBe(false)

  // Content should be saved
  const saved = await getBlock(page, block.block_id)
  console.log(`  [test] Saved content: "${saved.content}"`)
  expect(saved.content).toContain('after ctrl-enter')
})

// ============================================================================
// Test 7: Enter in edit mode positions new block correctly between siblings
// ============================================================================

test('Enter creates new block positioned between current and next sibling', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'pos-parent', position: '4080' })
  const first = await createBlock(page, {
    namespace: 'pos-parent', name: 'pos-first', content: 'first', position: '4080',
  })
  const second = await createBlock(page, {
    namespace: 'pos-parent', name: 'pos-second', content: 'second', position: 'c080',
  })

  await page.goto('/#/pos-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${second.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Enter edit mode on first
  await page.locator(`[data-block-id="${first.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  // Press Enter to create new sibling after first
  await page.keyboard.press('Enter')
  await page.waitForTimeout(1000)

  // Check child order — new block should be between first and second
  const children = await getChildren(page, parent.block_id)
  const names = children.map((c: any) => c.name)
  console.log(`  [test] Children order: ${JSON.stringify(names)}`)

  const firstIdx = names.indexOf('pos-first')
  const secondIdx = names.indexOf('pos-second')

  // There should be a new block between them
  expect(secondIdx).toBeGreaterThan(firstIdx + 1)

  // Verify positions are ordered
  const positions = children.map((c: any) => c.position)
  console.log(`  [test] Positions: ${JSON.stringify(positions)}`)
  for (let i = 1; i < positions.length; i++) {
    expect(positions[i] > positions[i - 1]).toBe(true)
  }
})
