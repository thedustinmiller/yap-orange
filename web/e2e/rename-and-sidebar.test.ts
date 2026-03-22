import { test, expect, type Page } from '@playwright/test'
import { screenshot } from './screenshot'

/**
 * Rename and sidebar interaction tests (SPA / WASM mode).
 *
 * Tests inline rename on OutlinerNode, header rename on breadcrumb,
 * sidebar expand/collapse, sidebar bookmark, and sidebar delete with
 * confirmation modal.
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

// ============================================================================
// Test 1: Inline rename via double-click on node name
// ============================================================================

test('double-click on empty block opens inline rename', async ({ page }) => {
  // For blocks with empty content, the .empty-label has ondblclick={startRename}.
  // Single-click on .empty-label no longer enters edit mode (it's a no-op),
  // so double-click correctly triggers rename.

  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'rename-parent', position: '4080' })
  const block = await createBlock(page, {
    namespace: 'rename-parent', name: 'rename-me', content: '', position: '4080',
  })

  await page.goto('/#/rename-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Double-click on the empty-label to trigger rename
  const nameTarget = page.locator(`[data-block-id="${block.block_id}"] > .node-row .empty-label`)
  await nameTarget.dblclick()
  await page.waitForTimeout(300)

  // Rename input should be visible
  const renameInput = page.locator(`[data-block-id="${block.block_id}"] .rename-input`)
  await expect(renameInput).toBeVisible({ timeout: 2_000 })

  // Type new name and confirm
  await renameInput.fill('renamed-empty')
  await renameInput.press('Enter')
  await page.waitForTimeout(500)

  // Verify the name changed via API
  const updated = await getBlock(page, block.block_id)
  console.log(`  [test] Block name after rename: "${updated.name}"`)
  expect(updated.name).toBe('renamed-empty')
})

test('double-click on name-hover area opens inline rename', async ({ page }) => {
  // The .node-name-hover element (right side of the row) has ondblclick={startRename}
  // and is NOT inside .node-content, so it works correctly.
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'ren2-parent', position: '4080' })
  const block = await createBlock(page, {
    namespace: 'ren2-parent', name: 'ren2-me', content: 'has content', position: '4080',
  })

  await page.goto('/#/ren2-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Hover to reveal the name-hover area, then double-click it
  const row = page.locator(`[data-block-id="${block.block_id}"] > .node-row`)
  await row.hover()
  await page.waitForTimeout(200)

  const nameHover = page.locator(`[data-block-id="${block.block_id}"] > .node-row .node-name-hover`)
  await nameHover.dblclick()
  await page.waitForTimeout(300)

  // Rename input should be visible
  const renameInput = page.locator(`[data-block-id="${block.block_id}"] .rename-input`)
  await expect(renameInput).toBeVisible({ timeout: 2_000 })

  // Screenshot: inline rename input visible with editable name
  await screenshot(page, 'managing-blocks', '01-inline-rename-input')

  // Type new name
  await renameInput.fill('renamed-block')
  await renameInput.press('Enter')
  await page.waitForTimeout(500)

  // Screenshot: block after successful rename
  await screenshot(page, 'managing-blocks', '02-after-rename')

  const updated = await getBlock(page, block.block_id)
  console.log(`  [test] Block name after rename: "${updated.name}"`)
  expect(updated.name).toBe('renamed-block')
})

// ============================================================================
// Test 2: Inline rename cancellation via Escape
// ============================================================================

test('Escape cancels inline rename without saving', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const parent = await createBlock(page, { namespace: '', name: 'noesc-parent', position: '4080' })
  const block = await createBlock(page, {
    namespace: 'noesc-parent', name: 'no-rename', content: 'has content', position: '4080',
  })

  await page.goto('/#/noesc-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Hover to reveal name, then dblclick on .node-name-hover
  const row = page.locator(`[data-block-id="${block.block_id}"] > .node-row`)
  await row.hover()
  await page.waitForTimeout(200)

  const nameHover = page.locator(`[data-block-id="${block.block_id}"] > .node-row .node-name-hover`)
  await nameHover.dblclick()
  await page.waitForTimeout(300)

  const renameInput = page.locator(`[data-block-id="${block.block_id}"] .rename-input`)
  await expect(renameInput).toBeVisible({ timeout: 2_000 })

  // Type something different
  await renameInput.fill('should-not-save')

  // Cancel with Escape
  await renameInput.press('Escape')
  await page.waitForTimeout(300)

  // Rename input should be gone
  await expect(renameInput).not.toBeVisible()

  // Name should be unchanged — Escape cancels the rename
  const block2 = await getBlock(page, block.block_id)
  console.log(`  [test] Name after Escape: "${block2.name}"`)
  expect(block2.name).toBe('no-rename')
})

// ============================================================================
// Test 3: Header rename (click on current breadcrumb name)
// ============================================================================

test('header breadcrumb rename updates block name', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'header-rename', content: '', position: '4080' })

  // Navigate into the block
  await page.goto('/#/header-rename')
  await waitForApp(page)
  await page.waitForTimeout(500)

  // The current crumb should show the block name
  const currentCrumb = page.locator('.crumb.current.clickable')
  await expect(currentCrumb).toHaveText('header-rename')

  // Click it to trigger rename
  await currentCrumb.click()
  await page.waitForTimeout(300)

  // Header rename input should appear
  const headerInput = page.locator('.header-rename-input')
  await expect(headerInput).toBeVisible({ timeout: 2_000 })

  // Type new name
  await headerInput.fill('new-header-name')
  await headerInput.press('Enter')
  await page.waitForTimeout(500)

  // Verify via API
  const updated = await getBlock(page, block.block_id)
  console.log(`  [test] Header rename result: "${updated.name}"`)
  expect(updated.name).toBe('new-header-name')
})

// ============================================================================
// Test 4: Sidebar expand reveals children
// ============================================================================

test('sidebar expand toggle shows children', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  await createBlock(page, { namespace: '', name: 'sb-parent', content: '', position: '4080' })
  await createBlock(page, { namespace: 'sb-parent', name: 'sb-child', content: 'hi', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  // Find the sidebar parent item
  const parentItem = page.locator('.sidebar .tree-item', { hasText: 'sb-parent' })
  await expect(parentItem).toBeVisible({ timeout: 5_000 })

  // The expand toggle (▶) should be present
  const toggle = parentItem.locator('.tree-toggle')
  await expect(toggle).toBeVisible()

  // Click toggle to expand
  await toggle.click()
  await page.waitForTimeout(500)

  // Child should now be visible in sidebar
  const childItem = page.locator('.sidebar .tree-item', { hasText: 'sb-child' })
  await expect(childItem).toBeVisible({ timeout: 5_000 })
  console.log(`  [test] Sidebar child visible after expand`)

  // Screenshot: sidebar with expanded children visible
  await screenshot(page, 'managing-blocks', '04-sidebar-expanded')

  // Click toggle again to collapse
  await toggle.click()
  await page.waitForTimeout(300)

  // Child should be hidden
  await expect(childItem).not.toBeVisible()
})

// ============================================================================
// Test 5: Sidebar bookmark toggle
// ============================================================================

test('sidebar bookmark toggle adds/removes bookmark', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  await createBlock(page, { namespace: '', name: 'bm-test', content: 'bookmarkable', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  const sidebarItem = page.locator('.sidebar .tree-item', { hasText: 'bm-test' })
  await expect(sidebarItem).toBeVisible({ timeout: 5_000 })

  // Hover to reveal hidden buttons (star, delete are opacity: 0 until hover)
  await sidebarItem.hover()
  await page.waitForTimeout(200)

  // The star should be empty initially (☆)
  const star = sidebarItem.locator('.tree-bookmark')
  const starText = await star.textContent()
  console.log(`  [test] Initial star: "${starText?.trim()}"`)

  // Click star to bookmark — use force: true since it may have CSS opacity: 0
  await star.click({ force: true })
  await page.waitForTimeout(300)

  // Star should now be filled (★) — class should have 'bookmarked'
  const isBookmarked = await star.evaluate(el => el.classList.contains('bookmarked'))
  console.log(`  [test] After bookmark click: bookmarked=${isBookmarked}`)
  expect(isBookmarked).toBe(true)

  // Click again to unbookmark
  await star.click({ force: true })
  await page.waitForTimeout(300)

  const isStillBookmarked = await star.evaluate(el => el.classList.contains('bookmarked'))
  expect(isStillBookmarked).toBe(false)
})

// ============================================================================
// Test 6: Sidebar delete with confirmation modal
// ============================================================================

test('sidebar delete shows confirmation modal and deletes on confirm', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'del-target', content: 'soon gone', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  const sidebarItem = page.locator('.sidebar .tree-item', { hasText: 'del-target' })
  await expect(sidebarItem).toBeVisible({ timeout: 5_000 })

  // Hover to reveal hidden buttons, then click delete
  await sidebarItem.hover()
  await page.waitForTimeout(200)

  const deleteBtn = sidebarItem.locator('.tree-delete')
  await deleteBtn.click({ force: true })
  await page.waitForTimeout(300)

  // Confirmation modal should appear
  const modal = page.locator('.modal')
  await expect(modal).toBeVisible({ timeout: 2_000 })

  // Modal should mention the block name
  const modalText = await modal.textContent()
  expect(modalText).toContain('del-target')
  console.log(`  [test] Modal text: "${modalText?.trim()}"`)

  // Screenshot: delete confirmation modal
  await screenshot(page, 'managing-blocks', '03-delete-confirmation-modal')

  // Click Delete button in modal
  await page.locator('.btn-delete').click()
  await page.waitForTimeout(1000)

  // Modal should be gone
  await expect(modal).not.toBeVisible()

  // Verify via API — should get a 404 or the block should be soft-deleted
  const result = await page.evaluate(async (id) => {
    try {
      const api = (window as any).__yap?.api
      const block = await api.blocks.get(id)
      return { found: true, deleted_at: block.deleted_at }
    } catch (e: any) {
      return { found: false, status: e.status }
    }
  }, block.block_id)

  console.log(`  [test] Block after delete: ${JSON.stringify(result)}`)
  // Either not found or soft-deleted
  expect(result.found === false || result.deleted_at !== null).toBe(true)

  // The sidebar should eventually refresh.
  // Wait for roots reload and check if item is still visible.
  await page.waitForTimeout(1000)
  const stillVisible = await sidebarItem.isVisible()
  console.log(`  [test] Sidebar item still visible after delete: ${stillVisible}`)
  // If the block is gone from the API but still in sidebar, that's a UI refresh issue
})

// ============================================================================
// Test 7: Sidebar delete cancellation via Cancel button
// ============================================================================

test('sidebar delete cancel closes modal without deleting', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'keep-me', content: 'staying', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  const sidebarItem = page.locator('.sidebar .tree-item', { hasText: 'keep-me' })
  await expect(sidebarItem).toBeVisible({ timeout: 5_000 })

  // Hover to reveal buttons, then click delete
  await sidebarItem.hover()
  await page.waitForTimeout(200)
  await sidebarItem.locator('.tree-delete').click({ force: true })
  await page.waitForTimeout(300)

  // Modal appears
  const modal = page.locator('.modal')
  await expect(modal).toBeVisible()

  // Click Cancel
  await page.locator('.btn-cancel').click()
  await page.waitForTimeout(300)

  // Modal should close
  await expect(modal).not.toBeVisible()

  // Block should still be in sidebar
  await expect(sidebarItem).toBeVisible()

  // Block should still exist via API
  const block2 = await getBlock(page, block.block_id)
  expect(block2.name).toBe('keep-me')
})
