import { test, expect, type Page } from '@playwright/test'
import AxeBuilder from '@axe-core/playwright'

/**
 * Accessibility test suite using axe-core.
 *
 * Runs WCAG 2.1 AA audits against major views/states of the app.
 * Each test navigates to a specific state, then runs axe analysis
 * and reports violations grouped by impact level.
 *
 * All tests are baseline audits — they always pass but print a
 * detailed report. Once fixes are made, uncomment the assertions
 * to enforce zero-violation targets.
 */

// Use a longer timeout — WASM worker init can be slow per test
test.setTimeout(60_000)

// ── Helpers ──────────────────────────────────────────────────────────

async function waitForApp(page: Page) {
  await page.waitForSelector('.dv-dockview', { timeout: 30_000 })
  // The outliner might not render if we're on a settings page,
  // so wait for either outliner content or a custom view
  await page.waitForFunction(
    () => {
      return (
        document.querySelector('.outliner-content') ||
        document.querySelector('.settings-view') ||
        document.querySelector('.dv-dockview')
      )
    },
    { timeout: 30_000 },
  )
  // Give panels time to finish rendering
  await page.waitForTimeout(1000)
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

interface AxeViolation {
  id: string
  impact: string | null
  description: string
  helpUrl: string
  nodes: { html: string; target: string[] }[]
}

/** Format axe violations into a readable report string. */
function formatViolations(violations: AxeViolation[], label: string): string {
  const lines: string[] = []
  lines.push(`\n${'='.repeat(70)}`)
  lines.push(`AXE: ${label} — ${violations.length} violation(s)`)
  lines.push(`${'='.repeat(70)}`)

  if (violations.length === 0) {
    lines.push('No violations found.')
    return lines.join('\n')
  }

  const grouped: Record<string, AxeViolation[]> = {
    critical: [],
    serious: [],
    moderate: [],
    minor: [],
  }

  for (const v of violations) {
    const impact = v.impact ?? 'minor'
    if (!grouped[impact]) grouped[impact] = []
    grouped[impact].push(v)
  }

  for (const level of ['critical', 'serious', 'moderate', 'minor']) {
    const items = grouped[level]
    if (!items || items.length === 0) continue
    lines.push(`\n[${level.toUpperCase()}] — ${items.length} violation(s)`)
    lines.push(`${'-'.repeat(50)}`)
    for (const v of items) {
      lines.push(`  Rule: ${v.id}`)
      lines.push(`  Desc: ${v.description}`)
      lines.push(`  Help: ${v.helpUrl}`)
      lines.push(`  Affected elements (${v.nodes.length}):`)
      for (const node of v.nodes.slice(0, 5)) {
        const selector = node.target.join(' > ')
        const html = node.html.length > 120 ? node.html.slice(0, 120) + '…' : node.html
        lines.push(`    - ${selector}`)
        lines.push(`      ${html}`)
      }
      if (v.nodes.length > 5) {
        lines.push(`    ... and ${v.nodes.length - 5} more`)
      }
      lines.push('')
    }
  }

  return lines.join('\n')
}

function summarizeCounts(violations: AxeViolation[]): Record<string, number> {
  const counts: Record<string, number> = { critical: 0, serious: 0, moderate: 0, minor: 0 }
  for (const v of violations) {
    const impact = v.impact ?? 'minor'
    counts[impact] = (counts[impact] || 0) + 1
  }
  return counts
}

// ── Tests ────────────────────────────────────────────────────────────

test.describe('Accessibility — axe-core audits', () => {

  test('full page audit — initial load', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'])
      .analyze()

    console.log(formatViolations(results.violations, 'Full Page Audit'))
    console.log('Summary:', JSON.stringify(summarizeCounts(results.violations)))
  })

  test('outliner with expanded tree', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    // Create blocks without reloading — they appear in the current WASM session
    await createBlock(page, { namespace: '', name: 'a11y-parent', content: 'Parent content', position: '4080' })
    await createBlock(page, { namespace: 'a11y-parent', name: 'child-one', content: 'First child', position: '4080' })
    await createBlock(page, { namespace: 'a11y-parent', name: 'child-two', content: 'Second child', position: '8080' })

    // Navigate into the parent to see children
    await page.goto('/#/a11y-parent')
    await waitForApp(page)

    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'])
      .analyze()

    console.log(formatViolations(results.violations, 'Outliner with Tree'))
    console.log('Summary:', JSON.stringify(summarizeCounts(results.violations)))
  })

  test('edit mode — CodeMirror editor', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    // Create a block, then interact without full reload
    await createBlock(page, {
      namespace: '',
      name: 'a11y-edit-test',
      content: 'Content with a [[wiki-link]] inside',
      position: '4080',
    })

    // Navigate into it so the outliner refreshes
    await page.goto('/#/')
    await waitForApp(page)

    // Find and double-click the block to enter edit mode
    const nodeRow = page.locator('[data-block-id] > .node-row', { hasText: 'a11y-edit-test' }).first()
    if (await nodeRow.isVisible({ timeout: 5000 }).catch(() => false)) {
      const nodeName = nodeRow.locator('.node-name, .node-name-hover')
      await nodeName.dblclick()
      await page.waitForTimeout(500)
    }

    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'])
      .analyze()

    console.log(formatViolations(results.violations, 'Edit Mode (CodeMirror)'))
    console.log('Summary:', JSON.stringify(summarizeCounts(results.violations)))
  })

  test('context menu overlay', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    await createBlock(page, { namespace: '', name: 'a11y-ctx', content: '', position: '4080' })
    await page.goto('/#/')
    await waitForApp(page)

    // Right-click a block to open context menu
    const nodeRow = page.locator('[data-block-id] > .node-row', { hasText: 'a11y-ctx' }).first()
    if (await nodeRow.isVisible({ timeout: 5000 }).catch(() => false)) {
      await nodeRow.click({ button: 'right' })
      await page.waitForTimeout(500)
    }

    const ctxMenu = page.locator('.context-menu')
    const menuVisible = await ctxMenu.isVisible().catch(() => false)

    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'])
      .analyze()

    console.log(formatViolations(results.violations, `Context Menu (visible=${menuVisible})`))
    console.log('Summary:', JSON.stringify(summarizeCounts(results.violations)))
  })

  test('settings view — form controls', async ({ page }) => {
    await page.goto('/#/settings::ui')
    await waitForApp(page)

    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'])
      .analyze()

    console.log(formatViolations(results.violations, 'Settings View'))
    console.log('Summary:', JSON.stringify(summarizeCounts(results.violations)))
  })

  test('sidebar navigation panel', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    const sidebarVisible = await page.locator('.sidebar').isVisible().catch(() => false)
    if (!sidebarVisible) {
      console.log('Sidebar not visible — skipping')
      return
    }

    const results = await new AxeBuilder({ page })
      .include('.sidebar')
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'])
      .analyze()

    console.log(formatViolations(results.violations, 'Sidebar Panel'))
    console.log('Summary:', JSON.stringify(summarizeCounts(results.violations)))
  })

  test('color contrast', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    const results = await new AxeBuilder({ page })
      .withRules(['color-contrast'])
      .analyze()

    console.log(formatViolations(results.violations, 'Color Contrast'))
    if (results.violations.length > 0) {
      const totalElements = results.violations.reduce((sum, v) => sum + v.nodes.length, 0)
      console.log(`Total elements with contrast issues: ${totalElements}`)
    }
  })

  test('keyboard and focus', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    const results = await new AxeBuilder({ page })
      .withRules([
        'focus-order-semantics',
        'tabindex',
        'scrollable-region-focusable',
        'nested-interactive',
      ])
      .analyze()

    console.log(formatViolations(results.violations, 'Keyboard & Focus'))
    console.log('Summary:', JSON.stringify(summarizeCounts(results.violations)))
  })

  test('ARIA and landmarks', async ({ page }) => {
    await page.goto('/')
    await waitForApp(page)

    const results = await new AxeBuilder({ page })
      .withRules([
        'aria-allowed-attr',
        'aria-allowed-role',
        'aria-hidden-body',
        'aria-hidden-focus',
        'aria-required-attr',
        'aria-required-children',
        'aria-required-parent',
        'aria-roles',
        'aria-valid-attr-value',
        'aria-valid-attr',
        'button-name',
        'document-title',
        'image-alt',
        'input-button-name',
        'label',
        'landmark-one-main',
        'link-name',
        'list',
        'listitem',
        'region',
      ])
      .analyze()

    console.log(formatViolations(results.violations, 'ARIA & Landmarks'))
    console.log('Summary:', JSON.stringify(summarizeCounts(results.violations)))
  })
})
