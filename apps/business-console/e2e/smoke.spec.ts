import { expect, test } from '@playwright/test'

test('console shell exposes the primary business views', async ({ page }) => {
  await page.route('**/api/v1/operations/overview', async (route) => route.fulfill({ json: { success: true, data: { document_total: 0, document_created_today: 0, processing_by_status: {}, review_pending: 0, unresolved_findings: 0, recent_jobs: [], recent_audit_events: [] } } }))
  await page.goto('/')
  await expect(page.getByText('Operations Console')).toBeVisible()
  await expect(page.getByRole('link', { name: 'Documents' })).toBeVisible()
})
