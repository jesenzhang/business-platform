import { expect, test } from '@playwright/test'
import { candidate, jobs, mockConsoleApi } from './fixtures/console'

/**
 * Processing list/detail and candidate review (PLAN-0016 S4-B1/B2).
 *
 * The review is a write path: the decision, the candidate version and the
 * comment must reach the API unchanged, and the idempotency key must be fresh.
 */

const job = jobs[0]

test('job queue lists the durable jobs with links', async ({ page }) => {
  await mockConsoleApi(page)
  await page.route('**/api/v1/processing-jobs?limit=*', (route) => route.fulfill({ json: { success: true, data: { items: jobs, next_cursor: null } } }))
  await page.goto('/processing')

  await expect(page.getByRole('heading', { level: 1, name: 'Processing' })).toBeVisible()
  await expect(page.getByRole('table')).toBeVisible()
  await expect(page.getByText('await review').first()).toBeVisible()
  await expect(page.getByRole('link', { name: /a1000000/ }).first()).toBeVisible()
})

test('job detail shows the execution state and the fixed pipeline', async ({ page }) => {
  await mockConsoleApi(page)
  await page.route('**/api/v1/processing-jobs/*/candidate', (route) => route.fulfill({ json: { success: true, data: candidate } }))
  await page.route('**/api/v1/processing-jobs/*', (route) => route.fulfill({ json: { success: true, data: job } }))
  await page.goto(`/processing/${job.job_id}`)

  await expect(page.getByText('EXECUTION STATE')).toBeVisible()
  await expect(page.getByText('FIXED PIPELINE')).toBeVisible()
  await expect(page.getByText('validate source')).toBeVisible()
  await expect(page.getByText('await review', { exact: true }).first()).toBeVisible()
  await expect(page.getByRole('link', { name: 'Review candidate' })).toBeVisible()
})

test('candidate review renders the payload and submits the exact decision contract', async ({ page }) => {
  await mockConsoleApi(page)
  let body: Record<string, unknown> | null = null
  let idempotency: string | undefined
  await page.route('**/api/v1/processing-jobs/*/candidate', (route) => route.fulfill({ json: { success: true, data: candidate } }))
  await page.route('**/api/v1/processing-jobs/*/review', async (route) => {
    body = route.request().postDataJSON() as Record<string, unknown>
    idempotency = route.request().headers()['idempotency-key']
    await route.fulfill({ json: { success: true, data: { review: { id: 'r1', candidate_id: candidate.candidate_id, decision: body.decision, candidate_version: body.candidate_version, created_at: '2026-11-16T08:30:00.000Z' }, replayed: false } } })
  })

  await page.goto(`/processing/${job.job_id}/candidate`)
  await expect(page.getByRole('heading', { level: 1, name: 'Candidate review' })).toBeVisible()
  await expect(page.getByText('Master Services Agreement')).toBeVisible()
  await expect(page.getByText('local-vllm')).toBeVisible()

  await page.getByLabel('Comment').fill('checked against the signed copy')
  await page.getByRole('button', { name: 'Accept candidate' }).click()

  await expect(page.getByText('Decision recorded')).toBeVisible()
  expect(body).not.toBeNull()
  expect(body).toEqual({ decision: 'accepted', candidate_version: candidate.version, comment: 'checked against the signed copy' })
  expect(idempotency).toMatch(/^[0-9a-f-]{36}$/)
  await expect(page.getByRole('button', { name: 'Accept candidate' })).toBeDisabled()
  await expect(page.getByRole('button', { name: 'Reject' })).toBeDisabled()
})

test('candidate review keeps a failed decision bounded and retryable', async ({ page }) => {
  await mockConsoleApi(page)
  await page.route('**/api/v1/processing-jobs/*/candidate', (route) => route.fulfill({ json: { success: true, data: candidate } }))
  await page.route('**/api/v1/processing-jobs/*/review', (route) => route.fulfill({
    status: 409,
    json: { code: 'version_conflict', message: 'candidate version is stale' },
  }))

  await page.goto(`/processing/${job.job_id}/candidate`)
  await page.getByRole('button', { name: 'Reject' }).click()

  await expect(page.getByText('Decision not recorded')).toBeVisible()
  await expect(page.getByText('candidate version is stale')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Reject' })).toBeEnabled()
})
