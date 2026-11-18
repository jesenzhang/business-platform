import { expect, test } from '@playwright/test'
import { emptyOverview, mockConsoleApi, overview, applyConsoleTheme } from './fixtures/console'

/**
 * Dashboard pilot coverage (PLAN-0016 S2).
 *
 * The point of the pilot is real business data on a real route, so these tests
 * assert the business contract (values, links, loading/error/empty states) and
 * the layout contract (no horizontal overflow, no clipped cards) in Chromium —
 * not just that the page renders.
 */

const card = (page: import('@playwright/test').Page, label: string) =>
  page.locator('.astryx-card').filter({ hasText: label }).first()

test('renders the four live metrics and the three dashboard sections', async ({ page }) => {
  await mockConsoleApi(page)
  await page.goto('/')
  await expect(page.getByRole('heading', { level: 1, name: 'Good morning, operator.' })).toBeVisible()

  await expect(card(page, 'Documents')).toContainText('128')
  await expect(card(page, 'Review queue')).toContainText('4')
  await expect(card(page, 'Open findings')).toContainText('3')
  await expect(card(page, 'Succeeded jobs')).toContainText('41')

  await expect(page.getByText('+6 in the last day')).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Processing flow' })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Recent activity' })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Recently moving' })).toBeVisible()
  await expect(page.locator('canvas')).toBeVisible()
  await expect(page.getByText('Document Uploaded')).toBeVisible()
  await expect(page.getByText('await review')).toBeVisible()
})

test('renders an explicit empty state when the overview has no data', async ({ page }) => {
  await mockConsoleApi(page, { overview: emptyOverview })
  await page.goto('/')
  await expect(page.getByRole('heading', { level: 1, name: 'Good morning, operator.' })).toBeVisible()

  await expect(card(page, 'Documents')).toContainText('0')
  await expect(card(page, 'Succeeded jobs')).toContainText('0')
  await expect(page.getByText('+0 in the last day')).toBeVisible()
  await expect(page.getByText('No processing jobs in this scope yet.')).toBeVisible()
  await expect(page.getByText('No audit events yet.')).toBeVisible()
  await expect(page.getByText('No processing jobs in this scope.')).toBeVisible()
  // No chart canvas is created for an empty status map.
  await expect(page.locator('canvas')).toHaveCount(0)
})

test('surfaces the bounded error state instead of a blank page', async ({ page }) => {
  await mockConsoleApi(page, { overviewStatus: 500 })
  await page.goto('/')
  await expect(page.getByText('Couldn’t load this view.')).toBeVisible()
  await expect(page.getByText('Operations overview is temporarily unavailable')).toBeVisible()
  await expect(page.getByRole('heading', { level: 1, name: 'Good morning, operator.' })).toHaveCount(0)
})

test('a refetch updates the metrics and the chart', async ({ page }) => {
  await page.clock.install()
  let requests = 0
  await page.route('**/api/v1/**', (route) => route.fulfill({ json: { success: true, data: { items: [], next_cursor: null } } }))
  await page.route('**/api/v1/operations/overview', async (route) => {
    requests += 1
    const payload = requests === 1
      ? overview
      : {
          ...overview,
          document_total: 129,
          review_pending: 5,
          unresolved_findings: 1,
          processing_by_status: { queued: 1, succeeded: 42 },
        }
    await route.fulfill({ json: { success: true, data: payload } })
  })

  await page.goto('/')
  await expect(card(page, 'Documents')).toContainText('128')
  await expect(page.getByText('Candidates awaiting a decision')).toBeVisible()

  await page.clock.fastForward(11_000)

  await expect(card(page, 'Documents')).toContainText('129')
  await expect(card(page, 'Review queue')).toContainText('5')
  await expect(card(page, 'Open findings')).toContainText('1')
  await expect(card(page, 'Succeeded jobs')).toContainText('42')
  expect(requests).toBeGreaterThanOrEqual(2)
})

test('dashboard links keep their existing destinations', async ({ page }) => {
  await mockConsoleApi(page)
  await page.goto('/')

  await page.getByRole('link', { name: 'View jobs →' }).click()
  await expect(page).toHaveURL(/\/processing$/)
  await expect(page.getByRole('heading', { level: 1, name: 'Processing' })).toBeVisible()

  await page.goBack()
  await page.getByRole('link', { name: 'Open audit →' }).click()
  await expect(page).toHaveURL(/\/audit$/)
  await expect(page.getByRole('heading', { level: 1, name: 'Audit' })).toBeVisible()

  await page.goBack()
  await page.getByRole('link', { name: 'All processing →' }).click()
  await expect(page).toHaveURL(/\/processing$/)
})

for (const viewport of [{ name: 'desktop', width: 1440, height: 900 }, { name: 'mobile', width: 390, height: 844 }]) {
  for (const theme of ['light', 'dark'] as const) {
    test(`layout holds at ${viewport.name} in ${theme} mode`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height })
      await applyConsoleTheme(page, theme)
      await mockConsoleApi(page)
      await page.goto('/')
      await expect(page.getByRole('heading', { level: 1, name: 'Good morning, operator.' })).toBeVisible()
      await expect(page.locator('html')).toHaveAttribute('data-theme', theme)

      const layout = await page.evaluate(() => {
        const root = document.documentElement
        const metrics = Array.from(document.querySelectorAll('.astryx-card')).map((element) => {
          const rect = element.getBoundingClientRect()
          return { left: Math.round(rect.left), right: Math.round(rect.right), width: Math.round(rect.width) }
        })
        return {
          scrollWidth: root.scrollWidth,
          clientWidth: root.clientWidth,
          innerWidth: window.innerWidth,
          metrics,
          themeClass: document.querySelector('.app')?.className ?? '',
          cardBackground: getComputedStyle(document.querySelectorAll('.astryx-card')[0]).backgroundColor,
        }
      })

      expect(layout.scrollWidth, 'no unexpected horizontal scroll').toBeLessThanOrEqual(layout.clientWidth)
      expect(layout.metrics.length).toBeGreaterThanOrEqual(7)
      for (const metric of layout.metrics) {
        expect(metric.left, 'card starts inside the viewport').toBeGreaterThanOrEqual(0)
        expect(metric.right, 'card ends inside the viewport').toBeLessThanOrEqual(layout.innerWidth + 1)
        expect(metric.width, 'card keeps a usable width').toBeGreaterThan(100)
      }
      expect(layout.themeClass).toContain(theme === 'dark' ? 'dark' : 'app')
      expect(layout.cardBackground).not.toBe('rgba(0, 0, 0, 0)')
    })
  }
}

test('light and dark use different surface tokens', async ({ browser }) => {
  const readSurface = async (theme: 'light' | 'dark') => {
    const context = await browser.newContext({ locale: 'en-US', timezoneId: 'UTC' })
    const page = await context.newPage()
    await applyConsoleTheme(page, theme)
    await mockConsoleApi(page)
    await page.goto('/')
    await expect(page.locator('.astryx-card').first()).toBeVisible()
    const value = await page.locator('.astryx-card').first().evaluate((element) => getComputedStyle(element).backgroundColor)
    await context.close()
    return value
  }
  expect(await readSurface('light')).not.toBe(await readSurface('dark'))
})
