import { expect, test } from '@playwright/test'
import { auditEvents, findings, mockConsoleApi } from './fixtures/console'

/**
 * Findings, Audit and Repairs (PLAN-0016 S4-C1/C2).
 *
 * Repairs stays a read-only console surface: the page must never present a
 * finding as already repaired, and it must keep the safe-operating boundary
 * visible.
 */

test('findings render severity/status badges and link to the repair controls', async ({ page }) => {
  await mockConsoleApi(page)
  await page.route('**/api/v1/admin/integrity/findings?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: findings, next_cursor: null } } }))
  await page.goto('/findings')

  await expect(page.getByRole('heading', { level: 1, name: 'Integrity findings' })).toBeVisible()
  await expect(page.getByText('document.storage_missing')).toBeVisible()
  await expect(page.getByText('high')).toBeVisible()
  await expect(page.getByText('open', { exact: true })).toBeVisible()
  await expect(page.getByText('4')).toBeVisible()

  await page.getByRole('link', { name: 'Repair controls' }).click()
  await expect(page).toHaveURL(/\/repairs$/)
  await expect(page.getByRole('heading', { level: 1, name: 'Repairs' })).toBeVisible()
})

test('audit lists events and shows failure codes only in the expanded view', async ({ page }) => {
  await mockConsoleApi(page)
  await page.route('**/api/v1/admin/audit-events?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: auditEvents, next_cursor: null } } }))
  await page.goto('/audit')

  await expect(page.getByRole('heading', { level: 1, name: 'Audit' })).toBeVisible()
  await expect(page.getByText('document uploaded')).toBeVisible()
  await expect(page.getByText('processing job failed')).toBeVisible()
  await expect(page.getByText(/extract_timeout/)).toBeVisible()
  await expect(page.getByText('failed', { exact: true })).toBeVisible()
})

test('repairs keeps the boundary banner and hides already repaired findings', async ({ page }) => {
  await mockConsoleApi(page)
  await page.route('**/api/v1/admin/integrity/findings?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: findings, next_cursor: null } } }))
  await page.goto('/repairs')

  await expect(page.getByText('Safe operating boundary')).toBeVisible()
  await expect(page.getByText('document.storage_missing')).toBeVisible()
  await expect(page.getByText('audit.sequence_gap')).toHaveCount(0)
})

test('governance empty states explain the scope', async ({ page }) => {
  await mockConsoleApi(page)
  await page.route('**/api/v1/admin/integrity/findings?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: [], next_cursor: null } } }))
  await page.route('**/api/v1/admin/audit-events?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: [], next_cursor: null } } }))

  await page.goto('/findings')
  await expect(page.getByRole('heading', { name: 'Integrity is quiet in this tenant.' })).toBeVisible()

  await page.goto('/repairs')
  await expect(page.getByRole('heading', { name: 'No repair candidates are present.' })).toBeVisible()

  await page.goto('/audit')
  await expect(page.getByRole('heading', { name: 'No audit events yet.' })).toBeVisible()
})
