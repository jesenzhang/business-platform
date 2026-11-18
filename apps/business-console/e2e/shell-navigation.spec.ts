import { expect, test, type Page } from '@playwright/test'
import { mockConsoleApi } from './fixtures/console'

/**
 * Shared shell: navigation and mobile drawer (PLAN-0016 S3-3).
 *
 * React Router stays the navigation source of truth, so these tests assert the
 * behaviours a migration can silently break: client-side navigation (no full
 * reload), the active-route marker, drawer focus containment, Escape, backdrop
 * dismissal, closing on navigation, and touch activation.
 */

const routes = [
  ['/', 'Overview'],
  ['/documents', 'Documents'],
  ['/processing', 'Processing'],
  ['/findings', 'Integrity'],
  ['/repairs', 'Repairs'],
  ['/audit', 'Audit'],
  ['/identities', 'Identities'],
  ['/roles', 'Roles'],
  ['/bindings', 'Bindings'],
  ['/organization', 'Organization'],
  ['/explain', 'Explain'],
] as const

const nav = (page: Page) => page.getByRole('navigation', { name: 'Primary navigation' })

test('desktop navigation exposes every route and marks the current one', async ({ page }) => {
  await mockConsoleApi(page)
  const documentId = '11111111-1111-7111-8111-111111111111'
  await page.route(`**/api/v1/documents/${documentId}/processing-jobs`, (route) => route.fulfill({ json: { success: true, data: [] } }))
  await page.route(`**/api/v1/documents/${documentId}`, (route) => route.fulfill({
    json: {
      success: true,
      data: {
        id: documentId,
        original_filename: 'master-services-agreement.pdf',
        content_type: 'application/pdf',
        status: 'succeeded',
        version: 1,
        content_revision: 1,
        size_bytes: 1024,
        created_at: '2026-11-16T08:00:00.000Z',
        updated_at: '2026-11-16T08:04:00.000Z',
      },
    },
  }))
  await page.goto('/documents')
  const primary = nav(page)
  await expect(primary).toBeVisible()

  for (const [, label] of routes) {
    await expect(primary.getByRole('link', { name: label })).toBeVisible()
  }
  await expect(primary.getByRole('link', { name: 'Documents' })).toHaveAttribute('aria-current', 'page')
  await expect(primary.locator('[aria-current="page"]')).toHaveCount(1)

  // A child route keeps its section selected.
  await page.goto(`/documents/${documentId}`)
  await expect(page.getByRole('heading', { level: 1, name: 'master-services-agreement.pdf' })).toBeVisible()
  await expect(primary.locator('[aria-current="page"]')).toHaveCount(1)
  await expect(primary.getByRole('link', { name: 'Documents' })).toHaveAttribute('aria-current', 'page')

  // The root is only selected on the root.
  await page.goto('/audit')
  await expect(primary.getByRole('link', { name: 'Audit' })).toHaveAttribute('aria-current', 'page')
  await expect(primary.getByRole('link', { name: 'Overview' })).not.toHaveAttribute('aria-current', 'page')
})

test('navigation is client-side and keeps the router in charge', async ({ page }) => {
  await mockConsoleApi(page)
  await page.goto('/')
  await page.evaluate(() => { (window as unknown as { __spa: boolean }).__spa = true })

  await nav(page).getByRole('link', { name: 'Roles' }).click()
  await expect(page).toHaveURL(/\/roles$/)
  // A full document reload would drop this flag.
  expect(await page.evaluate(() => (window as unknown as { __spa?: boolean }).__spa)).toBe(true)
  await expect(page.getByRole('heading', { name: 'Roles & permissions' })).toBeVisible()
})

test.describe('mobile drawer', () => {
  test.use({ viewport: { width: 390, height: 844 } })

  test('opens, traps focus, and closes with Escape returning focus to the toggle', async ({ page }) => {
    await mockConsoleApi(page)
    await page.goto('/')
    const toggle = page.getByRole('button', { name: 'Open navigation' })
    await expect(toggle).toBeVisible()
    await toggle.click()

    const dialog = page.getByRole('dialog', { name: 'Navigation' })
    await expect(dialog).toBeVisible()
    await expect(dialog.getByRole('link', { name: 'Overview' })).toBeVisible()

    // Escape closes the drawer and focus returns to the control that opened it.
    await page.keyboard.press('Escape')
    await expect(dialog).toBeHidden()
    await expect(toggle).toBeFocused()

    // Focus stays inside the drawer while tabbing.
    await toggle.click()
    await expect(dialog).toBeVisible()
    for (let index = 0; index < 6; index += 1) {
      await page.keyboard.press('Tab')
      const inside = await page.evaluate(() => Boolean(document.activeElement?.closest('dialog')))
      expect(inside, `focus stays inside the drawer after ${index + 1} tab presses`).toBe(true)
    }

    // Astryx dismisses exactly one layer per press. Tabbing onto a control opens
    // its tooltip, so the first Escape closes that tooltip and the next one the
    // drawer — never two layers at once.
    await page.keyboard.press('Escape')
    await page.keyboard.press('Escape')
    await expect(dialog).toBeHidden()
  })

  test('closes on navigation and on the backdrop', async ({ page }) => {
    await mockConsoleApi(page)
    await page.goto('/')

    await page.getByRole('button', { name: 'Open navigation' }).click()
    const dialog = page.getByRole('dialog', { name: 'Navigation' })
    await expect(dialog).toBeVisible()
    await dialog.getByRole('link', { name: 'Documents' }).click()
    await expect(page).toHaveURL(/\/documents$/)
    await expect(dialog).toBeHidden()

    await page.getByRole('button', { name: 'Open navigation' }).click()
    await expect(dialog).toBeVisible()
    // Click outside the 300px sheet: the modal backdrop.
    await page.mouse.click(370, 400)
    await expect(dialog).toBeHidden()

    // The close control in the header also works.
    await page.getByRole('button', { name: 'Open navigation' }).click()
    await expect(dialog).toBeVisible()
    await dialog.getByRole('button', { name: 'Close' }).click()
    await expect(dialog).toBeHidden()
  })

  test('drawer links are touch-operable', async ({ browser }) => {
    const context = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true })
    const page = await context.newPage()
    await mockConsoleApi(page)
    await page.goto('/')
    await page.getByRole('button', { name: 'Open navigation' }).tap()
    const dialog = page.getByRole('dialog', { name: 'Navigation' })
    await expect(dialog).toBeVisible()
    await dialog.getByRole('link', { name: 'Audit' }).tap()
    await expect(page).toHaveURL(/\/audit$/)
    await expect(dialog).toBeHidden()
    await expect(page.getByRole('heading', { name: 'Audit' })).toBeVisible()
    await context.close()
  })

  test('the mobile bar exposes the theme control and does not overflow', async ({ page }) => {
    await mockConsoleApi(page)
    await page.goto('/')
    const toggle = page.getByRole('button', { name: 'Toggle theme' })
    await expect(toggle).toBeVisible()
    await toggle.click()
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')

    const overflow = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    }))
    expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth)
  })
})
