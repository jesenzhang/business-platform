import AxeBuilder from '@axe-core/playwright'
import { expect, test } from '@playwright/test'
import { auditEvents, candidate, documents, findings, jobs, mockConsoleApi, overview, roles } from './fixtures/console'

/**
 * Accessibility floor for the migrated surfaces (PLAN-0016 S5).
 *
 * Scans fail only on serious and critical violations: those are the ones a
 * migration can introduce silently (missing names, broken landmarks, invalid
 * ARIA). Moderate/minor findings are listed by axe but tracked separately so a
 * stylistic preference cannot block the pipeline.
 */

const ROUTES = [
  { name: 'dashboard', path: '/', routes: async (page: import('@playwright/test').Page) => {
    await page.route('**/api/v1/operations/overview', (route) => route.fulfill({ json: { success: true, data: overview } }))
  } },
  { name: 'documents', path: '/documents', routes: async (page: import('@playwright/test').Page) => {
    await page.route('**/api/v1/documents?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: documents, next_cursor: null } } }))
  } },
  { name: 'roles', path: '/roles', routes: async (page: import('@playwright/test').Page) => {
    await page.route('**/api/v1/admin/roles', (route) => route.fulfill({ json: { success: true, data: roles } }))
  } },
  { name: 'audit', path: '/audit', routes: async (page: import('@playwright/test').Page) => {
    await page.route('**/api/v1/admin/audit-events?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: auditEvents, next_cursor: null } } }))
  } },
  { name: 'candidate', path: `/processing/${candidate.job_id}/candidate`, routes: async (page: import('@playwright/test').Page) => {
    await page.route('**/api/v1/processing-jobs/*/candidate', (route) => route.fulfill({ json: { success: true, data: candidate } }))
    await page.route('**/api/v1/processing-jobs/*', (route) => route.fulfill({ json: { success: true, data: jobs[0] } }))
  } },
  { name: 'findings', path: '/findings', routes: async (page: import('@playwright/test').Page) => {
    await page.route('**/api/v1/admin/integrity/findings?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: findings, next_cursor: null } } }))
  } },
] as const

for (const route of ROUTES) {
  test(`${route.name} has no serious or critical axe violations`, async ({ page }) => {
    await mockConsoleApi(page)
    await route.routes(page)
    await page.goto(route.path)
    await expect(page.locator('h1')).toBeVisible()
    await page.evaluate(() => document.fonts.ready)

    const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze()
    const blocking = results.violations.filter((violation) => violation.impact === 'serious' || violation.impact === 'critical')
    const summary = blocking.map((violation) => `${violation.id} (${violation.impact}): ${violation.nodes.length}× ${violation.help}`)
    expect(blocking, `axe serious/critical violations on ${route.path}:\n${summary.join('\n')}`).toEqual([])
  })
}
