import { test, expect, type Page } from '@playwright/test'
import { screenshot } from './screenshot'

/**
 * Keyboard navigation tests (SPA / WASM mode).
 *
 * Tests arrow-key navigation, expand/collapse, Enter to edit,
 * Escape to deselect, and the transition between nav and edit modes.
 *
 * These document the core keyboard-driven outliner experience.
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

/** Click the icon to select a block in navigation mode. */
async function selectBlock(page: Page, blockId: string) {
  await page.locator(`[data-block-id="${blockId}"] > .node-row .node-icon`).click()
}

/** Get the currently selected block IDs from the DOM. */
async function getSelectedIds(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const els = document.querySelectorAll('.outliner-content .selected')
    return Array.from(els).map(el => el.getAttribute('data-block-id') ?? '')
  })
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

/** Check what mode the status bar reports. */
async function getStatusMode(page: Page): Promise<string> {
  return page.evaluate(() => {
    const el = document.querySelector('.status-mode')
    return el?.textContent?.trim() ?? ''
  })
}

/** Check if a block is currently in editing state. */
async function isBlockEditing(page: Page, blockId: string): Promise<boolean> {
  return page.evaluate((id) => {
    const el = document.querySelector(`[data-block-id="${id}"]`)
    return el?.classList.contains('editing') ?? false
  }, blockId)
}

// ============================================================================
// Test 1: ArrowDown/ArrowUp moves selection through sibling blocks
// ============================================================================

test('arrow keys navigate between sibling blocks', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Use a dedicated namespace to avoid bootstrap blocks (types, schema, settings)
  // polluting the flat node list.
  const parent = await createBlock(page, { namespace: '', name: 'nav-parent', position: '4080' })
  const a = await createBlock(page, { namespace: 'nav-parent', name: 'nav-a', content: 'content a', position: '4080' })
  const b = await createBlock(page, { namespace: 'nav-parent', name: 'nav-b', content: 'content b', position: '8080' })
  const c = await createBlock(page, { namespace: 'nav-parent', name: 'nav-c', content: 'content c', position: 'c080' })

  // Navigate into the namespace so only our blocks are visible
  await page.goto('/#/nav-parent')
  await waitForApp(page)

  // Wait for all three blocks to be visible
  await expect(page.locator(`[data-block-id="${a.block_id}"]`)).toBeVisible({ timeout: 5_000 })
  await expect(page.locator(`[data-block-id="${b.block_id}"]`)).toBeVisible({ timeout: 5_000 })
  await expect(page.locator(`[data-block-id="${c.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Select the first block
  await selectBlock(page, a.block_id)
  await page.waitForTimeout(100)

  let selected = await getSelectedIds(page)
  expect(selected).toContain(a.block_id)

  // ArrowDown → should move to b
  await page.keyboard.press('ArrowDown')
  await page.waitForTimeout(100)
  selected = await getSelectedIds(page)
  expect(selected).toContain(b.block_id)
  expect(selected).not.toContain(a.block_id)

  // ArrowDown → should move to c
  await page.keyboard.press('ArrowDown')
  await page.waitForTimeout(100)
  selected = await getSelectedIds(page)
  expect(selected).toContain(c.block_id)

  // ArrowUp → should move back to b
  await page.keyboard.press('ArrowUp')
  await page.waitForTimeout(100)
  selected = await getSelectedIds(page)
  expect(selected).toContain(b.block_id)

  // ArrowUp → back to a
  await page.keyboard.press('ArrowUp')
  await page.waitForTimeout(100)
  selected = await getSelectedIds(page)
  expect(selected).toContain(a.block_id)

  // ArrowUp — moves to nav-parent (the virtual root at index 0).
  // The virtual root IS a node in flatNodes, so navigation stops there.
  await page.keyboard.press('ArrowUp')
  await page.waitForTimeout(100)
  selected = await getSelectedIds(page)
  // Note: this selects nav-parent (virtual root), not staying on 'a'
  console.log(`  [test] After ArrowUp past top: ${JSON.stringify(selected)}`)

  // ArrowUp at absolute top — should stay on whatever is at index 0
  const topId = selected[0]
  await page.keyboard.press('ArrowUp')
  await page.waitForTimeout(100)
  selected = await getSelectedIds(page)
  expect(selected).toContain(topId)
})

// ============================================================================
// Test 2: ArrowRight expands, ArrowLeft collapses
// ============================================================================

test('ArrowRight expands children, ArrowLeft collapses', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Create parent with children
  const parent = await createBlock(page, { namespace: '', name: 'expand-parent', content: '', position: '4080' })
  const child1 = await createBlock(page, { namespace: 'expand-parent', name: 'child-1', content: 'hello', position: '4080' })
  const child2 = await createBlock(page, { namespace: 'expand-parent', name: 'child-2', content: 'world', position: '8080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${parent.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Select the parent
  await selectBlock(page, parent.block_id)
  await page.waitForTimeout(200)

  // Children should not be visible yet (parent has no content → auto-expands,
  // but let's check what state we get)
  // Note: blocks with empty content auto-expand on mount, so children might already be visible.
  // This test verifies the ArrowRight/Left toggle regardless.

  // ArrowRight on parent → should expand (or navigate to first child if already expanded)
  await page.keyboard.press('ArrowRight')
  await page.waitForTimeout(500)

  // Children should be visible now
  const child1Visible = await page.locator(`[data-block-id="${child1.block_id}"]`).isVisible()
  const child2Visible = await page.locator(`[data-block-id="${child2.block_id}"]`).isVisible()
  console.log(`  [test] After ArrowRight: child1=${child1Visible}, child2=${child2Visible}`)
  expect(child1Visible || child2Visible).toBe(true)

  // If already expanded, ArrowRight should move selection to first child
  // Select parent again and try ArrowRight when already expanded
  await selectBlock(page, parent.block_id)
  await page.waitForTimeout(100)
  await page.keyboard.press('ArrowRight')
  await page.waitForTimeout(200)

  const selected = await getSelectedIds(page)
  // Should now be on child-1 (first child)
  console.log(`  [test] Selected after second ArrowRight: ${JSON.stringify(selected)}`)
  expect(selected).toContain(child1.block_id)

  // ArrowLeft on child → should move to parent
  await page.keyboard.press('ArrowLeft')
  await page.waitForTimeout(200)
  const selectedAfterLeft = await getSelectedIds(page)
  expect(selectedAfterLeft).toContain(parent.block_id)

  // ArrowLeft on expanded parent → should collapse
  await page.keyboard.press('ArrowLeft')
  await page.waitForTimeout(200)

  // Children should no longer be visible (parent collapsed)
  const child1AfterCollapse = await page.locator(`[data-block-id="${child1.block_id}"]`).isVisible()
  console.log(`  [test] child1 visible after collapse: ${child1AfterCollapse}`)
  expect(child1AfterCollapse).toBe(false)
})

// ============================================================================
// Test 3: Enter enters edit mode, Escape exits back to nav mode
// ============================================================================

test('Enter enters edit mode, Escape exits to nav mode', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'edit-test', content: 'editable content', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Verify initial state: NAV mode
  const initialMode = await getStatusMode(page)
  console.log(`  [test] Initial mode: ${initialMode}`)

  // Select block in nav mode
  await selectBlock(page, block.block_id)
  await page.waitForTimeout(200)

  const mode1 = await getStatusMode(page)
  expect(mode1).toBe('NAV')
  expect(await isBlockEditing(page, block.block_id)).toBe(false)

  // Screenshot: block selected in NAV mode
  await screenshot(page, 'editing-content', '01-nav-mode-selected')

  // Press Enter to edit
  await page.keyboard.press('Enter')
  await page.waitForTimeout(300)

  const mode2 = await getStatusMode(page)
  expect(mode2).toBe('EDIT')
  expect(await isBlockEditing(page, block.block_id)).toBe(true)

  // Verify CodeMirror editor is present
  const hasCM = await page.locator(`[data-block-id="${block.block_id}"] .cm-editor`).isVisible()
  expect(hasCM).toBe(true)

  // Screenshot: block in EDIT mode with CodeMirror editor
  await screenshot(page, 'editing-content', '02-edit-mode-active')

  // Press Escape to exit edit mode
  await page.keyboard.press('Escape')
  await page.waitForTimeout(300)

  const mode3 = await getStatusMode(page)
  expect(mode3).toBe('NAV')
  expect(await isBlockEditing(page, block.block_id)).toBe(false)

  // Screenshot: back in NAV mode after saving
  await screenshot(page, 'editing-content', '03-nav-mode-after-save')
})

// ============================================================================
// Test 4: Escape deselects all in nav mode
// ============================================================================

test('Escape in nav mode deselects all blocks', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'esc-test', content: 'some content', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 10_000 })

  // Select the block
  await selectBlock(page, block.block_id)
  await page.waitForTimeout(200)

  let selected = await getSelectedIds(page)
  expect(selected.length).toBe(1)

  // Press Escape
  await page.keyboard.press('Escape')
  await page.waitForTimeout(100)

  selected = await getSelectedIds(page)
  expect(selected.length).toBe(0)
})

// ============================================================================
// Test 5: ArrowDown at bottom of list stays on last block
// ============================================================================

test('ArrowDown at bottom stays on last block', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Use a namespace to isolate from bootstrap blocks
  const parent = await createBlock(page, { namespace: '', name: 'bottom-parent', position: '4080' })
  const a = await createBlock(page, { namespace: 'bottom-parent', name: 'bottom-a', content: 'a', position: '4080' })
  const b = await createBlock(page, { namespace: 'bottom-parent', name: 'bottom-b', content: 'b', position: '8080' })

  await page.goto('/#/bottom-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${b.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Select last block — parent is the virtual root [0], a is [1], b is [2]
  await selectBlock(page, b.block_id)
  await page.waitForTimeout(100)

  // Press ArrowDown multiple times at the bottom
  await page.keyboard.press('ArrowDown')
  await page.waitForTimeout(50)
  await page.keyboard.press('ArrowDown')
  await page.waitForTimeout(50)

  const selected = await getSelectedIds(page)
  expect(selected).toContain(b.block_id)
})

// ============================================================================
// Test 6: Content editing round-trip (edit, type, save, verify)
// ============================================================================

test('edit mode allows typing and saving content', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'roundtrip-test', content: 'original', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Enter edit mode via click on content area
  await page.locator(`[data-block-id="${block.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  expect(await isBlockEditing(page, block.block_id)).toBe(true)

  // The CM editor should have the original content
  const cmEditor = page.locator(`[data-block-id="${block.block_id}"] .cm-editor`)
  await expect(cmEditor).toBeVisible()

  // Select all and type new content
  await page.keyboard.press('Meta+a')
  await page.waitForTimeout(50)
  await page.keyboard.type('updated content here', { delay: 20 })
  await page.waitForTimeout(100)

  // Screenshot: editing content in CodeMirror
  await screenshot(page, 'editing-content', '04-typing-content')

  // Save via Escape
  await page.keyboard.press('Escape')
  await page.waitForTimeout(500)

  // Screenshot: content saved and displayed
  await screenshot(page, 'editing-content', '05-content-saved')

  // Verify content was saved by checking through the API
  const savedBlock = await page.evaluate(async (id) => {
    const api = (window as any).__yap?.api
    return api.blocks.get(id)
  }, block.block_id)

  console.log(`  [test] Saved content: "${savedBlock.content}"`)
  // The content should contain our update (atom system may add metadata)
  expect(savedBlock.content).toContain('updated content here')
})

// ============================================================================
// Test 7: Edit mode blocks keyboard navigation
// ============================================================================

test('keyboard navigation is disabled during edit mode', async ({ page }) => {
  // In edit mode, ArrowDown in the middle of multi-line content should move
  // the cursor within the CM6 editor, NOT navigate to the next block.
  // ArrowDown only exits edit mode when the cursor is already on the LAST line.

  await page.goto('/')
  await waitForApp(page)

  // Pre-create a block with multi-line content so we don't have to type newlines
  // (Enter in this editor creates a new block, not a newline; Shift+Enter is newline)
  const parent = await createBlock(page, { namespace: '', name: 'editnav-parent', position: '4080' })
  const a = await createBlock(page, {
    namespace: 'editnav-parent', name: 'editnav-a',
    content: 'line1\nline2\nline3',
    position: '4080',
  })
  const b = await createBlock(page, {
    namespace: 'editnav-parent', name: 'editnav-b',
    content: 'other',
    position: '8080',
  })

  await page.goto('/#/editnav-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${a.block_id}"]`)).toBeVisible({ timeout: 5_000 })
  await expect(page.locator(`[data-block-id="${b.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Enter edit mode on block a via nav-mode Enter
  // (click on .node-content enters edit with cursor at end = last line,
  //  which means ArrowDown would immediately exit. Use icon click + Enter instead.)
  await selectBlock(page, a.block_id)
  await page.waitForTimeout(100)
  await page.keyboard.press('Enter')
  await page.waitForTimeout(300)

  expect(await isBlockEditing(page, a.block_id)).toBe(true)

  // initialCursorPosition is 'end' by default, so cursor is at end of line 3.
  // Move cursor to the beginning of the document using Ctrl+Home (Linux)
  await page.keyboard.press('Control+Home')
  await page.waitForTimeout(100)

  // Now cursor is on line 1 of 3. ArrowDown moves within the editor (to line 2).
  await page.keyboard.press('ArrowDown')
  await page.waitForTimeout(100)

  // Still in edit mode on block a (not jumped to block b)
  expect(await isBlockEditing(page, a.block_id)).toBe(true)
  expect(await isBlockEditing(page, b.block_id)).toBe(false)
})
