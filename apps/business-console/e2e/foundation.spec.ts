import { expect, test } from '@playwright/test'
import { mockConsoleApi } from './fixtures/console'

/**
 * Foundation Check (PLAN-0016 S1).
 *
 * A broken cascade-layer order fails silently: components keep their DOM but
 * lose padding, borders and surfaces. These assertions read computed styles in
 * real Chromium so that failure becomes a test failure.
 */

const computed = async (page: import('@playwright/test').Page, selector: string) => page.locator(selector).first().evaluate((element) => {
  const style = getComputedStyle(element)
  return {
    paddingInline: style.paddingInline,
    paddingTop: style.paddingTop,
    borderTopWidth: style.borderTopWidth,
    borderTopStyle: style.borderTopStyle,
    backgroundColor: style.backgroundColor,
    color: style.color,
    fontFamily: style.fontFamily,
  }
})

test('foundation primitives keep design-system padding, borders and surfaces', async ({ page }) => {
  await mockConsoleApi(page)
  await page.goto('/__foundation-check')
  await expect(page.locator('[data-foundation-check]')).toBeVisible()

  // The theme boundary is real, and it matches the console theme state.
  await expect(page.locator('html[data-astryx-theme="neutral"]')).toHaveCount(1)

  // Buttons: real padding and a resting surface. The primary variant is a
  // filled button without a border by design, so the edge assertion targets the
  // secondary variant's surface instead.
  const primary = await computed(page, '[data-foundation-check] button[data-variant="primary"]')
  expect(primary.paddingInline).not.toBe('0px')
  expect(primary.backgroundColor).not.toBe('rgba(0, 0, 0, 0)')
  expect(primary.backgroundColor).not.toBe('transparent')
  const secondary = await computed(page, '[data-foundation-check] button[data-variant="secondary"]')
  expect(secondary.paddingInline).not.toBe('0px')
  expect(secondary.backgroundColor).not.toBe('rgba(0, 0, 0, 0)')

  // TextInput: the field chrome is the `.astryx-text-input` wrapper (the input
  // itself is intentionally chrome-free). Border, padding and surface must hold.
  const input = page.getByLabel('Email')
  await expect(input).toBeVisible()
  const field = await computed(page, '[data-foundation-check] .astryx-text-input')
  expect(field.paddingInline).not.toBe('0px')
  expect(field.borderTopStyle).toBe('solid')
  expect(field.borderTopWidth).not.toBe('0px')
  expect(field.backgroundColor).not.toBe('rgba(0, 0, 0, 0)')

  // Card: internal padding and an opaque surface. `default` keeps a border,
  // `muted` is a borderless surface by design.
  const card = await computed(page, '[data-foundation-check] .astryx-card[data-variant="default"]')
  expect(card.paddingInline).not.toBe('0px')
  expect(card.borderTopWidth).not.toBe('0px')
  expect(card.borderTopStyle).toBe('solid')
  expect(card.backgroundColor).not.toBe('rgba(0, 0, 0, 0)')
  const mutedCard = await computed(page, '[data-foundation-check] .astryx-card[data-variant="muted"]')
  expect(mutedCard.paddingInline).not.toBe('0px')
  expect(mutedCard.backgroundColor).not.toBe('rgba(0, 0, 0, 0)')

  // Table: real cells with non-zero padding and readable header text.
  await expect(page.getByRole('table')).toBeVisible()
  await expect(page.getByRole('columnheader', { name: 'Name' })).toBeVisible()
  const cell = page.locator('[data-foundation-check] tbody td').first()
  await expect(cell).toBeVisible()
  const cellStyle = await cell.evaluate((element) => {
    const style = getComputedStyle(element)
    return { paddingInline: style.paddingInline, paddingTop: style.paddingTop }
  })
  expect(cellStyle.paddingInline).not.toBe('0px')
  expect(cellStyle.paddingTop).not.toBe('0px')

  // Theme tokens resolve on the boundary element.
  const accent = await page.locator('html').evaluate((element) => getComputedStyle(element).getPropertyValue('--color-accent').trim())
  expect(accent).not.toBe('')
  expect(accent).not.toBe('initial')
})

test('console theme state drives the Astryx mode and survives a reload', async ({ page }) => {
  await mockConsoleApi(page)
  await page.goto('/__foundation-check')
  const html = page.locator('html')
  await expect(html).toHaveAttribute('data-astryx-theme', 'neutral')
  await expect(html).toHaveAttribute('data-theme', 'light')
  const lightBackground = await computed(page, '[data-foundation-check] .astryx-card[data-variant="default"]')

  // The shell toggle is the single source of truth for both the legacy CSS
  // variables and the Astryx theme.
  await page.getByRole('button', { name: 'Toggle theme' }).click()
  await expect(html).toHaveAttribute('data-theme', 'dark')
  await expect(page.locator('.app')).toHaveClass(/dark/)
  expect(await page.evaluate(() => window.localStorage.getItem('business-console-theme'))).toBe('dark')

  const darkBackground = await computed(page, '[data-foundation-check] .astryx-card[data-variant="default"]')
  expect(darkBackground.backgroundColor).not.toBe(lightBackground.backgroundColor)

  await page.reload()
  await expect(page.locator('html')).toHaveAttribute('data-astryx-theme', 'neutral')
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  await expect(page.locator('.app')).toHaveClass(/dark/)
})

test('foundation check does not widen the document', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await mockConsoleApi(page)
  await page.goto('/__foundation-check')
  await expect(page.locator('[data-foundation-check]')).toBeVisible()
  const overflow = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }))
  expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth)
})
