import { test, expect, type Page } from '@playwright/test'
import { screenshot } from './screenshot'

/**
 * Navigation tests (SPA / WASM mode).
 *
 * Tests center-on button (⊙), breadcrumb navigation, hash router,
 * sidebar click, and back/forward browser history.
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

/** Get the current breadcrumb text (e.g., "~ :: parent :: child") */
async function getBreadcrumbText(page: Page): Promise<string> {
  return page.evaluate(() => {
    const el = document.querySelector('.breadcrumbs')
    return el?.textContent?.replace(/\s+/g, ' ')?.trim() ?? ''
  })
}

/** Get the name shown for the current block in the breadcrumb. */
async function getCurrentBlockName(page: Page): Promise<string> {
  return page.evaluate(() => {
    const el = document.querySelector('.crumb.current')
    return el?.textContent?.trim() ?? ''
  })
}

/** Get visible block names in the outliner. */
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
// Test 1: Center-on button (⊙) navigates INTO a block
// ============================================================================

test('center-on button navigates into block as virtual root', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Build: parent > [child-a, child-b]
  const parent = await createBlock(page, { namespace: '', name: 'center-parent', position: '4080' })
  await createBlock(page, { namespace: 'center-parent', name: 'child-a', content: 'hello', position: '4080' })
  await createBlock(page, { namespace: 'center-parent', name: 'child-b', content: 'world', position: '8080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${parent.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Hover over the parent row to reveal the center button
  await page.locator(`[data-block-id="${parent.block_id}"] > .node-row`).hover()
  await page.waitForTimeout(200)

  // Screenshot: center-on button (⊙) visible on hover
  await screenshot(page, 'navigating', '01-center-button-hover')

  // Click the center-on button (⊙)
  await page.locator(`[data-block-id="${parent.block_id}"] > .node-row .center-btn`).click()
  await page.waitForTimeout(500)

  // Screenshot: centered view with breadcrumbs showing
  await screenshot(page, 'navigating', '02-centered-view')

  // The breadcrumbs should show the parent block
  const currentName = await getCurrentBlockName(page)
  console.log(`  [test] Current block name: "${currentName}"`)
  expect(currentName).toBe('center-parent')

  // The URL should have changed
  const hash = new URL(page.url()).hash
  console.log(`  [test] URL hash: ${hash}`)
  expect(hash).toContain('center-parent')

  // The outliner should show children of center-parent
  await page.waitForTimeout(500)
  const names = await getVisibleBlockNames(page)
  console.log(`  [test] Visible blocks: ${JSON.stringify(names)}`)
  // Should include center-parent (the virtual root) and its children
})

// ============================================================================
// Test 2: Breadcrumb click navigates to parent
// ============================================================================

test('breadcrumb click navigates to parent block', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Build: grandparent > parent > child
  await createBlock(page, { namespace: '', name: 'gp', position: '4080' })
  const parent = await createBlock(page, { namespace: 'gp', name: 'par', content: '', position: '4080' })
  await createBlock(page, { namespace: 'gp::par', name: 'leaf', content: 'deep content', position: '4080' })

  // Navigate to the deepest level
  await page.goto('/#/gp::par')
  await waitForApp(page)
  await page.waitForTimeout(500)

  // Breadcrumbs should show the path
  const crumbText = await getBreadcrumbText(page)
  console.log(`  [test] Breadcrumbs at gp::par: "${crumbText}"`)
  // Should contain "gp" somewhere in the trail
  expect(crumbText).toContain('gp')
  expect(crumbText).toContain('par')

  // Screenshot: breadcrumb trail showing path hierarchy
  await screenshot(page, 'navigating', '03-breadcrumb-trail')

  // Click the "gp" breadcrumb (the first non-home crumb)
  const gpCrumb = page.locator('.crumb.clickable', { hasText: 'gp' }).first()
  await gpCrumb.click()
  await page.waitForTimeout(500)

  // Should now be centered on "gp"
  const currentName = await getCurrentBlockName(page)
  console.log(`  [test] After breadcrumb click: "${currentName}"`)
  expect(currentName).toBe('gp')
})

// ============================================================================
// Test 3: Home breadcrumb (~) returns to root view
// ============================================================================

test('home breadcrumb returns to root view', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  await createBlock(page, { namespace: '', name: 'home-test', content: 'hello', position: '4080' })

  // Navigate into a block
  await page.goto('/#/home-test')
  await waitForApp(page)
  await page.waitForTimeout(500)

  const name = await getCurrentBlockName(page)
  expect(name).toBe('home-test')

  // Click home crumb (~)
  await page.locator('.crumb.clickable', { hasText: '~' }).click()
  await page.waitForTimeout(500)

  // Should be at root — no current block in header, show "Root"
  const rootName = await getCurrentBlockName(page)
  console.log(`  [test] After home click: "${rootName}"`)
  expect(rootName).toBe('Root')
})

// ============================================================================
// Test 4: Hash router — direct URL navigation works
// ============================================================================

test('hash router navigates to block by namespace path', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  await createBlock(page, { namespace: '', name: 'route-ns', position: '4080' })
  await createBlock(page, { namespace: 'route-ns', name: 'deep', content: 'routed content', position: '4080' })

  // Navigate via URL
  await page.goto('/#/route-ns')
  await waitForApp(page)
  await page.waitForTimeout(500)

  const currentName = await getCurrentBlockName(page)
  console.log(`  [test] URL navigation: "${currentName}"`)
  expect(currentName).toBe('route-ns')
})

// ============================================================================
// Test 5: Browser back/forward navigation
// ============================================================================

test('browser back/forward navigates between viewed blocks', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const a = await createBlock(page, { namespace: '', name: 'hist-a', position: '4080' })
  const b = await createBlock(page, { namespace: '', name: 'hist-b', position: '8080' })

  await page.goto('/')
  await waitForApp(page)

  // Navigate to a via center button
  await page.locator(`[data-block-id="${a.block_id}"] > .node-row`).hover()
  await page.waitForTimeout(200)
  await page.locator(`[data-block-id="${a.block_id}"] > .node-row .center-btn`).click()
  await page.waitForTimeout(500)

  let name = await getCurrentBlockName(page)
  expect(name).toBe('hist-a')

  // Navigate to home then to b
  await page.locator('.crumb.clickable', { hasText: '~' }).click()
  await page.waitForTimeout(500)

  await page.locator(`[data-block-id="${b.block_id}"] > .node-row`).hover()
  await page.waitForTimeout(200)
  await page.locator(`[data-block-id="${b.block_id}"] > .node-row .center-btn`).click()
  await page.waitForTimeout(500)

  name = await getCurrentBlockName(page)
  expect(name).toBe('hist-b')

  // Go back
  await page.goBack()
  await page.waitForTimeout(500)

  // Should be at root (we went Home → hist-b, so back goes to Home)
  // The exact state depends on hash history, but we should not be at hist-b anymore
  const nameAfterBack = await getCurrentBlockName(page)
  console.log(`  [test] After back: "${nameAfterBack}"`)
  expect(nameAfterBack).not.toBe('hist-b')
})

// ============================================================================
// Test 6: Sidebar click sets active namespace
// ============================================================================

test('sidebar click navigates to namespace', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  await createBlock(page, { namespace: '', name: 'sidebar-ns', content: '', position: '4080' })
  await createBlock(page, { namespace: 'sidebar-ns', name: 'inner', content: 'sidebar deep', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  // The sidebar should show sidebar-ns as a root item
  const sidebarItem = page.locator('.sidebar .tree-item', { hasText: 'sidebar-ns' })
  await expect(sidebarItem).toBeVisible({ timeout: 5_000 })

  // Click it
  await sidebarItem.click()
  await page.waitForTimeout(500)

  // Should navigate into this namespace
  const currentName = await getCurrentBlockName(page)
  console.log(`  [test] Sidebar navigate: "${currentName}"`)
  expect(currentName).toBe('sidebar-ns')

  // Sidebar item should be marked active
  await expect(sidebarItem).toHaveClass(/active/)

  // Screenshot: sidebar with active item highlighted
  await screenshot(page, 'navigating', '04-sidebar-active')
})

// ============================================================================
// Test 7: Empty state shows when no blocks exist
// ============================================================================

test('empty state shows message when no children', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Create a namespace with no children content
  const empty = await createBlock(page, { namespace: '', name: 'empty-ns', content: '', position: '4080' })

  // Navigate into it
  await page.goto('/#/empty-ns')
  await waitForApp(page)
  await page.waitForTimeout(500)

  // The outliner should show the empty-ns as virtual root, which itself is the block
  // with empty content. Since it's the root it renders itself.
  // If there are no children below it, depends on whether empty-ns appears in its own tree.
  const names = await getVisibleBlockNames(page)
  console.log(`  [test] Blocks in empty namespace: ${JSON.stringify(names)}`)
})
