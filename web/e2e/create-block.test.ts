import { test, expect } from '@playwright/test'
import { screenshot } from './screenshot'

/**
 * Create-block workflow test (SPA / WASM mode).
 *
 * Requires: no backend server on port 3000 — the app auto-detects
 * and boots the in-browser WASM worker with SQLite + OPFS.
 *
 * Each Playwright BrowserContext gets its own storage partition,
 * so OPFS data is isolated between tests — no cleanup needed.
 */

/** Wait for the app to finish booting in SPA mode. */
async function waitForApp(page: import('@playwright/test').Page) {
  await page.waitForSelector('.dv-dockview', { timeout: 15_000 })
  await page.waitForSelector('.outliner-content .node-row', { timeout: 20_000 })
}

test('create a root block via quick-create', async ({ page }) => {
  page.on('console', (msg) => {
    if (msg.text().includes('[yap]')) {
      console.log(`  [browser] ${msg.text()}`)
    }
  })

  await page.goto('/')
  await waitForApp(page)

  const initialCount = await page.locator('.outliner-content .node-row').count()

  // Screenshot: initial outliner with the + button visible
  await screenshot(page, 'creating-blocks', '01-initial-outliner')

  // Click "+" to show quick-create
  await page.locator('[title="Create block"]').click()
  const input = page.locator('.quick-create-input')
  await expect(input).toBeVisible({ timeout: 2_000 })

  // Click and type using real keystrokes
  await input.click()
  await input.pressSequentially('test-block', { delay: 30 })

  await expect(input).toHaveValue('test-block')

  // Screenshot: quick-create input with typed name
  await screenshot(page, 'creating-blocks', '02-quick-create-input')

  await input.press('Enter')

  // Verify the new block appears
  await expect(page.locator('.node-row', { hasText: 'test-block' })).toBeVisible({ timeout: 5_000 })

  // Screenshot: new block visible in the outliner
  await screenshot(page, 'creating-blocks', '03-block-created')

  const finalCount = await page.locator('.outliner-content .node-row').count()
  expect(finalCount).toBeGreaterThan(initialCount)
})

test('quick-create input loses focus on click (known bug)', async ({ page }) => {
  // This test characterizes the focus-stealing bug:
  //
  //   Root cause: Outliner.svelte line 378
  //     onclick={() => { if (appState.mode === 'navigate') containerEl?.focus(); })
  //
  //   The click event from the input bubbles up to the .outliner div,
  //   which calls containerEl.focus() — stealing focus from the input.
  //
  //   Timeline:
  //     mousedown  → browser focuses input (correct)
  //     mouseup    → click event fires, bubbles to .outliner
  //     onclick    → containerEl.focus() steals focus (bug)
  //
  //   Playwright's click() dispatches trusted mousedown/mouseup/click events,
  //   but the event dispatch may be batched differently than real user input.
  //   We use page.mouse to simulate the exact low-level sequence.

  await page.goto('/')
  await waitForApp(page)

  // Open quick-create
  await page.locator('[title="Create block"]').click()
  const input = page.locator('.quick-create-input')
  await expect(input).toBeVisible({ timeout: 2_000 })

  // Get the input's bounding box for manual mouse events
  const box = await input.boundingBox()
  expect(box).toBeTruthy()
  const cx = box!.x + box!.width / 2
  const cy = box!.y + box!.height / 2

  // Step 1: mousedown — should focus the input
  await page.mouse.move(cx, cy)
  await page.mouse.down()

  // Small delay to let focus events propagate
  await page.waitForTimeout(50)

  const focusedAfterDown = await page.evaluate(() =>
    document.activeElement?.classList.contains('quick-create-input')
  )
  console.log(`  [test] Focused after mousedown: ${focusedAfterDown}`)

  // Step 2: mouseup — the click event bubbles to .outliner → containerEl.focus()
  await page.mouse.up()

  // Small delay to let the onclick handler and any microtasks run
  await page.waitForTimeout(50)

  const focusedAfterUp = await page.evaluate(() =>
    document.activeElement?.classList.contains('quick-create-input')
  )
  console.log(`  [test] Focused after mouseup: ${focusedAfterUp}`)

  // The bug: input is focused on mousedown but loses focus on mouseup.
  // If this test starts passing (both true), the bug has been fixed.
  expect(focusedAfterDown).toBe(true)
  expect(focusedAfterUp).toBe(false) // <-- THIS IS THE BUG: should be true
})
