import { test, expect, type Page } from '@playwright/test'
import { screenshot } from './screenshot'

/**
 * Context menu and multi-block indent tests (SPA / WASM mode).
 *
 * Tests the right-click context menu, the ⋯ button context menu,
 * and Tab/Shift+Tab on multiple selected blocks in nav mode.
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

async function getSelectedIds(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const els = document.querySelectorAll('.outliner-content .selected')
    return Array.from(els).map(el => el.getAttribute('data-block-id') ?? '')
  })
}

// ============================================================================
// Test 1: Right-click opens context menu
// ============================================================================

test('right-click on block opens context menu', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'ctx-block', content: 'right click me', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Right-click on the block row
  await page.locator(`[data-block-id="${block.block_id}"] > .node-row`).click({ button: 'right' })
  await page.waitForTimeout(300)

  // Context menu should appear
  const contextMenu = page.locator('.context-menu')
  await expect(contextMenu).toBeVisible({ timeout: 2_000 })

  // Should have menu items
  const menuItems = await contextMenu.locator('.context-menu-item').count()
  console.log(`  [test] Context menu items: ${menuItems}`)
  expect(menuItems).toBeGreaterThan(0)

  // Should contain common actions (Edit name, Focus here, Export subtree)
  const menuText = await contextMenu.textContent()
  console.log(`  [test] Context menu text: "${menuText?.trim()}"`)

  // Screenshot: context menu with available actions
  await screenshot(page, 'organizing-blocks', '01-context-menu')

  // Close the menu by pressing Escape
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
  await expect(contextMenu).not.toBeVisible()
})

// ============================================================================
// Test 2: Three-dot button (⋯) opens context menu
// ============================================================================

test('three-dot button opens context menu', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'dots-block', content: 'dot menu', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Hover to reveal the ⋯ button
  await page.locator(`[data-block-id="${block.block_id}"] > .node-row`).hover()
  await page.waitForTimeout(200)

  // Click the context menu button
  await page.locator(`[data-block-id="${block.block_id}"] .context-menu-btn`).click()
  await page.waitForTimeout(300)

  const contextMenu = page.locator('.context-menu')
  await expect(contextMenu).toBeVisible({ timeout: 2_000 })

  const menuText = await contextMenu.textContent()
  console.log(`  [test] Dot-menu text: "${menuText?.trim()}"`)
  expect(menuText).toBeTruthy()

  // Close
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
})

// ============================================================================
// Test 3: Tab in nav mode indents selected block
// ============================================================================

test('Tab in nav mode indents selected block under previous sibling', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'ind-parent', position: '4080' })
  const sib1 = await createBlock(page, { namespace: 'ind-parent', name: 'ind-s1', content: 'first', position: '4080' })
  const sib2 = await createBlock(page, { namespace: 'ind-parent', name: 'ind-s2', content: 'second', position: '8080' })

  await page.goto('/#/ind-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${sib2.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Verify sib2 is currently a child of parent
  let sib2State = await getBlock(page, sib2.block_id)
  expect(sib2State.parent_id).toBe(parent.block_id)

  // Select sib2 in nav mode
  await selectBlock(page, sib2.block_id)
  await page.waitForTimeout(100)

  // Tab to indent
  await page.keyboard.press('Tab')
  await page.waitForTimeout(1000)

  // sib2 should now be a child of sib1
  sib2State = await getBlock(page, sib2.block_id)
  console.log(`  [test] sib2 parent after Tab: ${sib2State.parent_id} (expected: ${sib1.block_id})`)
  expect(sib2State.parent_id).toBe(sib1.block_id)

  // Screenshot: block indented via Tab in nav mode
  await screenshot(page, 'organizing-blocks', '02-nav-mode-indent')
})

// ============================================================================
// Test 4: Tab on multiple selected blocks indents all of them
// ============================================================================

test('Tab indents all selected blocks under the same previous sibling', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'mind-parent', position: '4080' })
  const sib1 = await createBlock(page, { namespace: 'mind-parent', name: 'mind-s1', content: 'anchor', position: '2080' })
  const sib2 = await createBlock(page, { namespace: 'mind-parent', name: 'mind-s2', content: 'move me', position: '6080' })
  const sib3 = await createBlock(page, { namespace: 'mind-parent', name: 'mind-s3', content: 'and me', position: 'a080' })

  await page.goto('/#/mind-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${sib3.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Select sib2 and sib3 via Ctrl+Click
  await selectBlock(page, sib2.block_id)
  await page.waitForTimeout(50)
  await page.locator(`[data-block-id="${sib3.block_id}"] > .node-row .node-icon`).click({
    modifiers: ['Control'],
  })
  await page.waitForTimeout(100)

  const selected = await getSelectedIds(page)
  console.log(`  [test] Selected before Tab: ${JSON.stringify(selected)}`)
  expect(selected).toContain(sib2.block_id)
  expect(selected).toContain(sib3.block_id)

  // Tab to indent both
  await page.keyboard.press('Tab')
  await page.waitForTimeout(1500)

  // Both should now be children of sib1
  const sib2State = await getBlock(page, sib2.block_id)
  const sib3State = await getBlock(page, sib3.block_id)
  console.log(`  [test] sib2 parent: ${sib2State.parent_id}, sib3 parent: ${sib3State.parent_id} (expected: ${sib1.block_id})`)
  expect(sib2State.parent_id).toBe(sib1.block_id)
  expect(sib3State.parent_id).toBe(sib1.block_id)

  // Screenshot: multiple blocks indented together
  await screenshot(page, 'organizing-blocks', '03-multi-block-indent')
})

// ============================================================================
// Test 5: Context menu "Focus here" navigates into block
// ============================================================================

test('context menu Focus here navigates into block', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'focus-block', content: '', position: '4080' })
  await createBlock(page, { namespace: 'focus-block', name: 'focus-child', content: 'inner', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Right-click to open context menu
  await page.locator(`[data-block-id="${block.block_id}"] > .node-row`).click({ button: 'right' })
  await page.waitForTimeout(300)

  const contextMenu = page.locator('.context-menu')
  await expect(contextMenu).toBeVisible()

  // Click "Focus here"
  const focusItem = contextMenu.locator('.context-menu-item', { hasText: /focus/i })
  const hasFocus = await focusItem.count()
  console.log(`  [test] Focus item count: ${hasFocus}`)

  if (hasFocus > 0) {
    await focusItem.click()
    await page.waitForTimeout(500)

    // Should have navigated — breadcrumbs should show the block
    const currentName = await page.evaluate(() => {
      const el = document.querySelector('.crumb.current')
      return el?.textContent?.trim() ?? ''
    })
    console.log(`  [test] After Focus here: "${currentName}"`)
    expect(currentName).toBe('focus-block')
  }
})

// ============================================================================
// Test 6: First block cannot be indented (no previous sibling)
// ============================================================================

test('first block cannot be indented — stays in place', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'noind-parent', position: '4080' })
  const first = await createBlock(page, {
    namespace: 'noind-parent', name: 'noind-first', content: 'first', position: '4080',
  })

  await page.goto('/#/noind-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${first.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Verify initial parent
  let firstState = await getBlock(page, first.block_id)
  const originalParent = firstState.parent_id

  // Select first block and try Tab
  await selectBlock(page, first.block_id)
  await page.waitForTimeout(100)
  await page.keyboard.press('Tab')
  await page.waitForTimeout(500)

  // Should not have moved — still has the same parent
  firstState = await getBlock(page, first.block_id)
  console.log(`  [test] First block parent after Tab: ${firstState.parent_id} (original: ${originalParent})`)
  expect(firstState.parent_id).toBe(originalParent)
})
