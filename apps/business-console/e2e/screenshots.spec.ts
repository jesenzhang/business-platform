import { mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { expect, test } from '@playwright/test'
import { auditEvents, emptyOverview, findings, jobs, mockConsoleApi, applyConsoleTheme } from './fixtures/console'

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

/**
 * ECharts animates its first paint in JavaScript, so a screenshot taken as soon
 * as the canvas exists can catch a half-grown bar. Sample the canvas until two
 * consecutive samples agree; bounded so a continuously animating chart still
 * produces a screenshot.
 */
async function waitForCanvasToSettle(page: import('@playwright/test').Page) {
  const sample = () => page.evaluate(() => {
    const canvas = document.querySelector('canvas')
    if (!canvas) return 'missing'
    const pixels = canvas.getContext('2d')?.getImageData(0, 0, canvas.width, canvas.height).data
    if (!pixels) return 'unreadable'
    let hash = 0
    for (let index = 0; index < pixels.length; index += 41) hash = (hash * 31 + pixels[index]) | 0
    return String(hash)
  })
  let previous = await sample()
  for (let attempt = 0; attempt < 30; attempt += 1) {
    await page.waitForTimeout(120)
    const next = await sample()
    if (next === previous) return
    previous = next
  }
}

for (const view of views) {
  for (const viewport of viewports) {
    for (const theme of themes) {
      test(`${view.name} ${viewport.name} ${theme}`, async ({ page }) => {
        await page.setViewportSize({ width: viewport.width, height: viewport.height })
        await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: theme })
        await applyConsoleTheme(page, theme)
        await mockConsoleApi(page)
        await page.goto(view.path)
        await expect(page.getByText(view.readyText, { exact: false })).toBeVisible()
        await page.waitForLoadState('networkidle')
        await page.evaluate(() => document.fonts.ready)
        if (view.name === 'dashboard') {
          await expect(page.locator('canvas').first()).toBeVisible()
          await waitForCanvasToSettle(page)
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

/**
 * Dashboard state screenshots for review: an empty overview and a failed
 * request. They are captured at desktop width in both themes because the state
 * treatment (not the responsive layout) is what a reviewer needs to compare.
 */
const dashboardStates = [
  { name: 'dashboard-empty', options: { overview: emptyOverview }, readyText: 'No processing jobs in this scope yet.' },
  { name: 'dashboard-error', options: { overviewStatus: 500 }, readyText: 'Couldn’t load this view.' },
] as const

for (const state of dashboardStates) {
  for (const theme of themes) {
    test(`${state.name} ${theme}`, async ({ page }) => {
      await page.setViewportSize({ width: 1440, height: 900 })
      await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: theme })
      await applyConsoleTheme(page, theme)
      await mockConsoleApi(page, state.options)
      await page.goto('/')
      await expect(page.getByText(state.readyText, { exact: false })).toBeVisible()
      await page.waitForLoadState('networkidle')
      await page.evaluate(() => document.fonts.ready)

      const directory = path.resolve(String(outDir))
      await mkdir(directory, { recursive: true })
      const file = path.join(directory, `${state.name}--desktop--${theme}.png`)
      await page.screenshot({ path: file, fullPage: true, animations: 'disabled' })
    })
  }
}

/** Mobile navigation drawer, open, in both themes. */
for (const theme of themes) {
  test(`mobile drawer ${theme}`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 })
    await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: theme })
    await applyConsoleTheme(page, theme)
    await mockConsoleApi(page)
    await page.goto('/')
    await page.getByRole('button', { name: 'Open navigation' }).click()
    await expect(page.getByRole('dialog', { name: 'Navigation' })).toBeVisible()
    await page.evaluate(() => document.fonts.ready)

    const directory = path.resolve(String(outDir))
    await mkdir(directory, { recursive: true })
    const file = path.join(directory, `mobile-drawer--mobile--${theme}.png`)
    await page.screenshot({ path: file, fullPage: false, animations: 'disabled' })
  })
}

/**
 * One desktop screenshot per top-level route, so the S4 review can see every
 * migrated page rather than only the three sampled views.
 */
const captureRoutes = [
  { name: 'overview', path: '/' },
  { name: 'documents', path: '/documents' },
  { name: 'processing', path: '/processing' },
  { name: 'findings', path: '/findings' },
  { name: 'repairs', path: '/repairs' },
  { name: 'audit', path: '/audit' },
  { name: 'identities', path: '/identities' },
  { name: 'roles', path: '/roles' },
  { name: 'bindings', path: '/bindings' },
  { name: 'organization', path: '/organization' },
  { name: 'explain', path: '/explain' },
] as const

test('all routes (desktop, light)', async ({ page }) => {
  test.setTimeout(180_000)
  await page.setViewportSize({ width: 1440, height: 900 })
  await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: 'light' })
  await applyConsoleTheme(page, 'light')
  await mockConsoleApi(page)
  await page.route('**/api/v1/processing-jobs?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: jobs, next_cursor: null } } }))
  await page.route('**/api/v1/admin/integrity/findings?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: findings, next_cursor: null } } }))
  await page.route('**/api/v1/admin/audit-events?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: auditEvents, next_cursor: null } } }))

  const directory = path.resolve(String(outDir))
  await mkdir(directory, { recursive: true })
  for (const entry of captureRoutes) {
    await page.goto(entry.path)
    await expect(page.locator('h1')).toBeVisible()
    await page.waitForLoadState('networkidle')
    await page.evaluate(() => document.fonts.ready)
    await page.screenshot({ path: path.join(directory, `route--${entry.name}.png`), fullPage: true, animations: 'disabled' })
  }
})
