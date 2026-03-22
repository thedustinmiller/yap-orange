import { test, expect } from '@playwright/test'

test('app loads and renders dockview layout', async ({ page }) => {
  await page.goto('/')
  // Wait for the app to load
  await page.waitForSelector('.dv-dockview', { timeout: 10_000 })
  // Verify the dockview container rendered
  const dockview = page.locator('.dv-dockview')
  await expect(dockview).toBeVisible()
})
