import { mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { expect, test } from '@playwright/test'
import { mockConsoleApi, useConsoleTheme } from './fixtures/console'

/**
 * Opt-in screenshot capture for migration evidence.
 *
 * Run it explicitly; it is skipped in the regular `test:e2e` gate because it
 * writes PNG files:
 *
 *   CAPTURE_DIR=../../docs/reports/assets/astryx-migration/baseline \
 *     npx playwright test e2e/screenshots.spec.ts
 *
 * The capture is deterministic: fixed fixtures, fixed locale/timezone, reduced
 * motion, device scale factor 1, and no current-time values in the data.
 */

const outDir = process.env.CAPTURE_DIR
test.skip(!outDir, 'screenshot capture is opt-in: set CAPTURE_DIR')

const views = [
  { name: 'dashboard', path: '/', readyText: 'Good morning, operator.' },
  { name: 'documents', path: '/documents', readyText: 'Document register' },
  { name: 'roles', path: '/roles', readyText: 'Registered roles' },
] as const

const viewports = [
  { name: 'desktop', width: 1440, height: 900 },
  { name: 'mobile', width: 390, height: 844 },
] as const

const themes = ['light', 'dark'] as const

const metrics: Array<Record<string, unknown>> = []

for (const view of views) {
  for (const viewport of viewports) {
    for (const theme of themes) {
      test(`${view.name} ${viewport.name} ${theme}`, async ({ page }) => {
        await page.setViewportSize({ width: viewport.width, height: viewport.height })
        await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: theme })
        await useConsoleTheme(page, theme)
        await mockConsoleApi(page)
        await page.goto(view.path)
        await expect(page.getByText(view.readyText, { exact: false })).toBeVisible()
        await page.waitForLoadState('networkidle')
        await page.evaluate(() => document.fonts.ready)
        if (view.name === 'dashboard') {
          await expect(page.locator('canvas').first()).toBeVisible()
        }

        const measured = await page.evaluate(() => {
          const root = document.documentElement
          const body = document.body
          return {
            scrollWidth: root.scrollWidth,
            clientWidth: root.clientWidth,
            bodyScrollWidth: body.scrollWidth,
            horizontalOverflow: root.scrollWidth > root.clientWidth,
            themeClass: document.querySelector('.app')?.className ?? null,
            astryxThemeMode: document.querySelector('[data-astryx-theme]')?.getAttribute('data-astryx-theme') ?? null,
          }
        })

        const directory = path.resolve(String(outDir))
        await mkdir(directory, { recursive: true })
        const file = path.join(directory, `${view.name}--${viewport.name}--${theme}.png`)
        await page.screenshot({ path: file, fullPage: true, animations: 'disabled' })
        metrics.push({
          view: view.name,
          viewport: viewport.name,
          theme,
          file: path.basename(file),
          width: viewport.width,
          height: viewport.height,
          ...measured,
        })
        await writeFile(path.join(directory, 'metrics.json'), `${JSON.stringify(metrics, null, 2)}\n`, 'utf8')
      })
    }
  }
}
