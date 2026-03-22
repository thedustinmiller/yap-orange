import { mkdirSync } from 'fs'
import { dirname, join } from 'path'
import { fileURLToPath } from 'url'
import type { Page, Locator } from '@playwright/test'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)

/** Root directory for guide screenshots, relative to this file's location */
const SCREENSHOTS_DIR = join(__dirname, '../../guide/src/screenshots')

/**
 * Capture a screenshot during a test and save it to the guide's screenshot directory.
 *
 * Screenshots are saved to: guide/src/screenshots/{workflow}/{name}.png
 * Reference in mdbook pages as: ../screenshots/{workflow}/{name}.png
 *
 * Use `locator` option to crop the screenshot to a specific element (native crop).
 * Without it, the full page viewport is captured.
 */
export async function screenshot(
  page: Page,
  workflow: string,
  name: string,
  options?: { locator?: Locator },
): Promise<void> {
  const dir = join(SCREENSHOTS_DIR, workflow)
  mkdirSync(dir, { recursive: true })
  const path = join(dir, `${name}.png`)

  if (options?.locator) {
    await options.locator.screenshot({ path })
  } else {
    await page.screenshot({ path })
  }
}
