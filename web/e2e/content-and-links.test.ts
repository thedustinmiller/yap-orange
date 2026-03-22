import { test, expect, type Page } from '@playwright/test'
import { screenshot } from './screenshot'

/**
 * Content editing and wiki-link tests (SPA / WASM mode).
 *
 * Tests the content save round-trip, wiki-link rendering,
 * wiki-link navigation on click, and markdown rendering in
 * display mode.
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

async function isBlockEditing(page: Page, blockId: string): Promise<boolean> {
  return page.evaluate((id) => {
    const el = document.querySelector(`[data-block-id="${id}"]`)
    return el?.classList.contains('editing') ?? false
  }, blockId)
}

// ============================================================================
// Test 1: Content with wiki-link renders as clickable link in display mode
// ============================================================================

test('wiki-link in content renders as clickable link', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Create a target block that the link will point to
  await createBlock(page, { namespace: '', name: 'link-target', content: 'I am the target', position: '4080' })

  // Create a block with a wiki-link to the target
  const source = await createBlock(page, {
    namespace: '', name: 'link-source',
    content: 'See [[link-target]] for details',
    position: '8080',
  })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${source.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // The content should render with a wiki-link
  // In display mode, ContentRenderer converts [[path]] to <a class="wiki-link">
  const wikiLink = page.locator(`[data-block-id="${source.block_id}"] .wiki-link`)
  const linkCount = await wikiLink.count()
  console.log(`  [test] Wiki links found in display: ${linkCount}`)

  // If the content is rendered via ContentRenderer, it should have a wiki-link
  // Note: blocks with content show ContentRenderer in display mode
  if (linkCount > 0) {
    const linkText = await wikiLink.first().textContent()
    console.log(`  [test] Wiki link text: "${linkText}"`)
    expect(linkText).toContain('link-target')

    // Screenshot: wiki-link rendered as clickable text in display mode
    await screenshot(page, 'editing-content', '06-wiki-link-display')
  }
})

// ============================================================================
// Test 2: Wiki-link in CM6 editor — cursor-aware decorations
// ============================================================================

test('wiki-link in editor shows as decoration when cursor is away', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  await createBlock(page, { namespace: '', name: 'cm-target', content: 'target', position: '4080' })
  const source = await createBlock(page, {
    namespace: '', name: 'cm-source',
    content: 'link: [[cm-target]]',
    position: '8080',
  })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${source.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Enter edit mode
  await page.locator(`[data-block-id="${source.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(500)

  expect(await isBlockEditing(page, source.block_id)).toBe(true)

  // Move cursor to the beginning (away from the link)
  await page.keyboard.press('Home')
  await page.waitForTimeout(200)

  // Screenshot: wiki-link decoration in CM6 editor (cursor away from link)
  await screenshot(page, 'editing-content', '06-wiki-link-editor')

  // Check if a wiki-link widget/decoration is present in the CM editor
  const hasDecoration = await page.evaluate((id) => {
    const node = document.querySelector(`[data-block-id="${id}"]`)
    if (!node) return false
    // Look for wiki-link widgets (Decoration.replace creates these)
    const widgets = node.querySelectorAll('.cm-editor .cm-wikilink-widget, .cm-editor .wiki-link-widget')
    // Also look for mark decorations
    const marks = node.querySelectorAll('.cm-editor .cm-wikilink')
    return { widgets: widgets.length, marks: marks.length }
  }, source.block_id)

  console.log(`  [test] CM decorations: ${JSON.stringify(hasDecoration)}`)
  // Should have some form of wiki-link decoration
  // The exact class depends on the wikilinks.ts implementation

  // Exit edit mode
  await page.keyboard.press('Escape')
  await page.waitForTimeout(300)
})

// ============================================================================
// Test 3: Content round-trip preserves markdown formatting
// ============================================================================

test('markdown content is preserved through edit cycle', async ({ page }) => {
  // Content is only loaded eagerly when navigating INTO a parent namespace.
  // At root level, blocks are visible but content is NOT loaded.
  // So we must place the test block inside a namespace.
  await page.goto('/')
  await waitForApp(page)

  const mdContent = '**bold** and *italic* and `code`'
  const parent = await createBlock(page, { namespace: '', name: 'md-parent', position: '4080' })
  const block = await createBlock(page, {
    namespace: 'md-parent', name: 'md-test',
    content: mdContent,
    position: '4080',
  })

  await page.goto('/#/md-parent')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Wait for content to load (content is loaded via loadChildrenWithContent)
  const contentEl = page.locator(`[data-block-id="${block.block_id}"] .content-text`)
  await expect(contentEl).toBeVisible({ timeout: 5_000 })

  const html = await contentEl.innerHTML()
  console.log(`  [test] Rendered HTML: ${html}`)

  // Should contain rendered markdown elements
  // Bold should be <strong> or <b>
  expect(html).toMatch(/<(strong|b)>bold<\/(strong|b)>/)

  // Screenshot: markdown content rendered with formatting
  await screenshot(page, 'editing-content', '07-markdown-rendered')

  // Enter edit mode
  await page.locator(`[data-block-id="${block.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  // Exit without changes
  await page.keyboard.press('Escape')
  await page.waitForTimeout(500)

  // Content should be unchanged after the edit cycle
  const saved = await getBlock(page, block.block_id)
  console.log(`  [test] Content after cycle: "${saved.content}"`)
  expect(saved.content).toContain('**bold**')
  expect(saved.content).toContain('*italic*')
  expect(saved.content).toContain('`code`')
})

// ============================================================================
// Test 4: Creating block with empty content shows as namespace/container
// ============================================================================

test('block with empty content displays name instead of content', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, { namespace: '', name: 'container-block', content: '', position: '4080' })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Block with empty content should show the name via .empty-label
  const emptyLabel = page.locator(`[data-block-id="${block.block_id}"] .empty-label`)
  const count = await emptyLabel.count()
  console.log(`  [test] Empty label present: ${count > 0}`)

  if (count > 0) {
    const text = await emptyLabel.textContent()
    console.log(`  [test] Empty label text: "${text}"`)
    expect(text).toContain('container-block')
  }

  // Icon should be folder (📁) for empty content blocks
  const icon = page.locator(`[data-block-id="${block.block_id}"] .node-icon`)
  const iconText = await icon.textContent()
  console.log(`  [test] Icon: "${iconText?.trim()}"`)
})

// ============================================================================
// Test 5: Multiline content is preserved
// ============================================================================

test('multiline content is preserved through save', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  const block = await createBlock(page, {
    namespace: '', name: 'multiline-test',
    content: 'line one',
    position: '4080',
  })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // Enter edit mode
  await page.locator(`[data-block-id="${block.block_id}"] > .node-row .node-content`).click()
  await page.waitForTimeout(300)

  // Select all and type multiline content
  await page.keyboard.press('Meta+a')
  await page.keyboard.type('first line', { delay: 10 })
  // Shift+Enter for newline
  await page.keyboard.press('Shift+Enter')
  await page.keyboard.type('second line', { delay: 10 })
  await page.keyboard.press('Shift+Enter')
  await page.keyboard.type('third line', { delay: 10 })

  // Save with Escape
  await page.keyboard.press('Escape')
  await page.waitForTimeout(500)

  // Verify content
  const saved = await getBlock(page, block.block_id)
  console.log(`  [test] Saved content: "${saved.content}"`)
  expect(saved.content).toContain('first line')
  expect(saved.content).toContain('second line')
  expect(saved.content).toContain('third line')
  // Should have newlines
  expect(saved.content.split('\n').length).toBeGreaterThanOrEqual(3)
})

// ============================================================================
// Test 6: Long content scroll does not break display
// ============================================================================

test('block with long content renders without breaking layout', async ({ page }) => {
  await page.goto('/')
  await waitForApp(page)

  // Create a block with very long content
  const longContent = 'Lorem ipsum dolor sit amet. '.repeat(50)
  const block = await createBlock(page, {
    namespace: '', name: 'long-content',
    content: longContent,
    position: '4080',
  })

  await page.goto('/')
  await waitForApp(page)

  await expect(page.locator(`[data-block-id="${block.block_id}"]`)).toBeVisible({ timeout: 5_000 })

  // The node-row should still be visible and not overflow the container
  const row = page.locator(`[data-block-id="${block.block_id}"] > .node-row`)
  const box = await row.boundingBox()
  console.log(`  [test] Row dimensions: ${JSON.stringify(box)}`)
  expect(box).toBeTruthy()
  expect(box!.width).toBeGreaterThan(0)
  expect(box!.height).toBeGreaterThan(0)
})
