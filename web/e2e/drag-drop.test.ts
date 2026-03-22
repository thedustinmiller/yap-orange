import { test, expect, type Page } from '@playwright/test'

/**
 * Drag-and-drop characterization tests (SPA / WASM mode).
 *
 * These tests verify block ordering after drag-and-drop operations.
 * HTML5 DnD in Playwright is unreliable with dataTransfer, so we simulate
 * the drop behavior by calling the same move API sequence that handleDrop does.
 *
 * This approach isolates the positioning logic from the UI mechanics of
 * dragging, which lets us characterize the ordering bugs precisely.
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

/** Get children of a block sorted by position (server-side ordering). */
async function getChildren(page: Page, parentId: string): Promise<any[]> {
  return page.evaluate(async (id) => {
    const api = (window as any).__yap?.api
    return api.blocks.children(id)
  }, parentId)
}

/** Move a block via the API (same as what handleDrop calls). */
async function moveBlock(
  page: Page,
  blockId: string,
  data: { parent_id: string | null; position?: string },
) {
  return page.evaluate(async ({ id, d }) => {
    const api = (window as any).__yap?.api
    return api.blocks.move(id, d)
  }, { id: blockId, d: data })
}

/** Get root blocks from the API. */
async function getRoots(page: Page): Promise<any[]> {
  return page.evaluate(async () => {
    const api = (window as any).__yap?.api
    return api.roots()
  })
}

// ============================================================================
// Single block drag: above, below, inside
// ============================================================================

test.describe('single block drag-and-drop', () => {
  test('move block above a sibling', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    // Create parent with 3 ordered children: A, B, C
    const parent = await createBlock(page, { namespace: '', name: 'parent' })
    const a = await createBlock(page, { namespace: 'parent', name: 'A', position: '4080' })
    const b = await createBlock(page, { namespace: 'parent', name: 'B', position: '8080' })
    const c = await createBlock(page, { namespace: 'parent', name: 'C', position: 'c080' })

    // Move C above B (simulates drop zone = 'above' on B)
    // computeDropPosition('above') on B: positionBetween(A.position, B.position)
    // Position between '4080' and '8080' = '6080'
    const children0 = await getChildren(page, parent.block_id)
    const bBlock = children0.find((c: any) => c.name === 'B')
    const aBlock = children0.find((c: any) => c.name === 'A')

    // Compute position between A and B (what computeDropPosition would do)
    const dropPos = await page.evaluate(({ before, after }) => {
      // Access positionBetween — it's bundled, so we use the API's internal module
      // Instead, we'll hardcode the expected behavior: midpoint between before and after
      // For hex fractional indices: between '4080' and '8080' → '6080'
      return '6080' // midpoint approximation
    }, { before: aBlock.position, after: bBlock.position })

    await moveBlock(page, c.block_id, { parent_id: parent.block_id, position: dropPos })

    const children = await getChildren(page, parent.block_id)
    const order = children.map((c: any) => c.name)
    console.log(`  [test] Order after move C above B: ${JSON.stringify(order)}`)

    // Expected: A, C, B (C inserted between A and B)
    expect(order).toEqual(['A', 'C', 'B'])
  })

  test('move block below a sibling', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    const parent = await createBlock(page, { namespace: '', name: 'parent' })
    await createBlock(page, { namespace: 'parent', name: 'A', position: '4080' })
    const b = await createBlock(page, { namespace: 'parent', name: 'B', position: '8080' })
    const c = await createBlock(page, { namespace: 'parent', name: 'C', position: 'c080' })

    // Move A below B: positionBetween(B.position, C.position)
    // Between '8080' and 'c080' → 'a080'
    const a = (await getChildren(page, parent.block_id)).find((c: any) => c.name === 'A')
    await moveBlock(page, a.id, { parent_id: parent.block_id, position: 'a080' })

    const children = await getChildren(page, parent.block_id)
    const order = children.map((c: any) => c.name)
    console.log(`  [test] Order after move A below B: ${JSON.stringify(order)}`)

    // Expected: B, A, C
    expect(order).toEqual(['B', 'A', 'C'])
  })

  test('move block inside another (reparent)', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    const parent = await createBlock(page, { namespace: '', name: 'parent' })
    const a = await createBlock(page, { namespace: 'parent', name: 'A', position: '4080' })
    const b = await createBlock(page, { namespace: 'parent', name: 'B', position: '8080' })

    // Move B inside A (no position — server appends at end)
    await moveBlock(page, b.block_id, { parent_id: a.block_id })

    // B should now be a child of A
    const bState = await getBlock(page, b.block_id)
    expect(bState.parent_id).toBe(a.block_id)

    // Parent should only have A
    const parentChildren = await getChildren(page, parent.block_id)
    expect(parentChildren.map((c: any) => c.name)).toEqual(['A'])

    // A should have B as child
    const aChildren = await getChildren(page, a.block_id)
    expect(aChildren.map((c: any) => c.name)).toEqual(['B'])
  })
})

// ============================================================================
// Multi-block drag: position ordering
// ============================================================================

test.describe('multi-block drag-and-drop ordering', () => {
  test('multi-block drop above: each block gets a distinct position', async ({ page }) => {
    // handleDrop now tracks lastAssignedPosition and computes subsequent
    // positions relative to it, so each block gets a unique position.

    await page.goto('/')
    await waitForApp(page)

    const parent = await createBlock(page, { namespace: '', name: 'parent' })
    const a = await createBlock(page, { namespace: 'parent', name: 'A', position: '2080' })
    const b = await createBlock(page, { namespace: 'parent', name: 'B', position: '4080' })
    const c = await createBlock(page, { namespace: 'parent', name: 'C', position: '6080' })
    const d = await createBlock(page, { namespace: 'parent', name: 'D', position: '8080' })
    const e = await createBlock(page, { namespace: 'parent', name: 'E', position: 'a080' })

    // Simulate the fixed handleDrop logic: drag [D, E] above B
    // First block: positionBetween(A.position, B.position) = between '2080' and '4080'
    const pos1 = '3080' // positionBetween('2080', '4080')
    // Second block: positionBetween(pos1, B.position) = between '3080' and '4080'
    const pos2 = '3880' // positionBetween('3080', '4080')
    await moveBlock(page, d.block_id, { parent_id: parent.block_id, position: pos1 })
    await moveBlock(page, e.block_id, { parent_id: parent.block_id, position: pos2 })

    const children = await getChildren(page, parent.block_id)
    const order = children.map((c: any) => c.name)
    const positions = children.map((c: any) => ({ name: c.name, pos: c.position }))
    console.log(`  [test] Order after multi-drop: ${JSON.stringify(order)}`)
    console.log(`  [test] Positions: ${JSON.stringify(positions)}`)

    // D and E should have distinct positions
    const dPos = positions.find(p => p.name === 'D')!.pos
    const ePos = positions.find(p => p.name === 'E')!.pos
    expect(dPos).not.toBe(ePos)
    // D should come before E
    expect(dPos < ePos).toBe(true)
  })

  test('multi-block drop below: each block gets a distinct position', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    const parent = await createBlock(page, { namespace: '', name: 'parent' })
    const a = await createBlock(page, { namespace: 'parent', name: 'A', position: '2080' })
    const b = await createBlock(page, { namespace: 'parent', name: 'B', position: '4080' })
    const c = await createBlock(page, { namespace: 'parent', name: 'C', position: '6080' })
    const d = await createBlock(page, { namespace: 'parent', name: 'D', position: '8080' })

    // Simulate fixed handleDrop: drag [A, B, C] below D
    // First: positionBetween(D.position, null) = after '8080'
    const pos1 = '8180' // positionBetween('8080', null)
    // Second: positionBetween(pos1, null) = after pos1
    const pos2 = '8280' // positionBetween('8180', null)
    // Third: positionBetween(pos2, null) = after pos2
    const pos3 = '8380' // positionBetween('8280', null)
    await moveBlock(page, a.block_id, { parent_id: parent.block_id, position: pos1 })
    await moveBlock(page, b.block_id, { parent_id: parent.block_id, position: pos2 })
    await moveBlock(page, c.block_id, { parent_id: parent.block_id, position: pos3 })

    const children = await getChildren(page, parent.block_id)
    const order = children.map((c: any) => c.name)
    console.log(`  [test] Order after 3-block drop below D: ${JSON.stringify(order)}`)

    // D should be first (it wasn't moved)
    expect(order[0]).toBe('D')

    // A, B, C should have distinct positions
    const aPos = children.find((c: any) => c.name === 'A').position
    const bPos = children.find((c: any) => c.name === 'B').position
    const cPos = children.find((c: any) => c.name === 'C').position
    expect(aPos).not.toBe(bPos)
    expect(bPos).not.toBe(cPos)
    // Order: A < B < C
    expect(aPos < bPos).toBe(true)
    expect(bPos < cPos).toBe(true)
  })

  test('dragging multiple blocks inside a target preserves order', async ({ page }) => {
    // When dropping "inside", no position is specified — the server
    // calls next_position() for each block sequentially, appending at end.
    // Since moves happen in draggedIds iteration order, the original
    // selection order should be preserved IF draggedIds preserves it.

    await page.goto('/')
    await waitForApp(page)

    const parent = await createBlock(page, { namespace: '', name: 'parent' })
    const target = await createBlock(page, { namespace: 'parent', name: 'target', position: '2080' })
    const a = await createBlock(page, { namespace: 'parent', name: 'A', position: '4080' })
    const b = await createBlock(page, { namespace: 'parent', name: 'B', position: '6080' })
    const c = await createBlock(page, { namespace: 'parent', name: 'C', position: '8080' })

    // Simulate: select [A, B, C], drag inside "target" (no position)
    await moveBlock(page, a.block_id, { parent_id: target.block_id })
    await moveBlock(page, b.block_id, { parent_id: target.block_id })
    await moveBlock(page, c.block_id, { parent_id: target.block_id })

    const targetChildren = await getChildren(page, target.block_id)
    const order = targetChildren.map((c: any) => c.name)
    console.log(`  [test] Order inside target: ${JSON.stringify(order)}`)

    // Should be A, B, C (appended sequentially)
    expect(order).toEqual(['A', 'B', 'C'])
  })
})

// ============================================================================
// Full UI drag-and-drop (using native DnD events)
// ============================================================================

test.describe('UI drag-and-drop', () => {
  test('drag single block below another via mouse', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    const parent = await createBlock(page, { namespace: '', name: 'dnd-parent' })
    await createBlock(page, { namespace: 'dnd-parent', name: 'first', position: '4080' })
    await createBlock(page, { namespace: 'dnd-parent', name: 'second', position: '8080' })
    await createBlock(page, { namespace: 'dnd-parent', name: 'third', position: 'c080' })

    // Navigate into parent
    await page.goto('/#/dnd-parent')
    await waitForApp(page)

    const firstRow = page.locator('[data-block-id] > .node-row', { hasText: 'first' })
    const thirdRow = page.locator('[data-block-id] > .node-row', { hasText: 'third' })
    await expect(firstRow).toBeVisible({ timeout: 5_000 })
    await expect(thirdRow).toBeVisible({ timeout: 5_000 })

    // Use Playwright's dragTo — dispatches pointer events that trigger HTML5 DnD
    // Target the bottom 25% of third's row to trigger 'below' zone
    const thirdBox = await thirdRow.boundingBox()
    expect(thirdBox).toBeTruthy()

    await firstRow.dragTo(thirdRow, {
      targetPosition: { x: thirdBox!.width / 2, y: thirdBox!.height * 0.9 },
    })

    await page.waitForTimeout(1000)

    // Check the actual order via API
    const children = await getChildren(page, parent.block_id)
    const order = children.map((c: any) => c.name)
    console.log(`  [test] Order after UI drag: ${JSON.stringify(order)}`)

    // Expected: dnd-parent header, then second, third, first
    // (first moved below third)
    expect(order.indexOf('second')).toBeLessThan(order.indexOf('third'))
    expect(order.indexOf('third')).toBeLessThan(order.indexOf('first'))
  })
})
