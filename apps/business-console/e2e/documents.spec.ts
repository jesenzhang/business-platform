import { expect, test } from '@playwright/test'
import { documents, mockConsoleApi } from './fixtures/console'

/**
 * Documents list + upload (PLAN-0016 S4-A1).
 *
 * The upload contract is the risky part of this page: multipart body,
 * Idempotency-Key, and a full navigation to the new document on success.
 */

const document = documents[0]

test('lists the register and keeps upload disabled until a file is chosen', async ({ page }) => {
  await mockConsoleApi(page)
  await page.goto('/documents')

  await expect(page.getByRole('heading', { level: 1, name: 'Documents' })).toBeVisible()
  await expect(page.getByRole('table')).toBeVisible()
  await expect(page.getByRole('link', { name: document.original_filename })).toBeVisible()
  await expect(page.getByText('master-services-agreement.pdf')).toBeVisible()
  await expect(page.getByText('supplier-onboarding.docx')).toBeVisible()

  const upload = page.getByRole('button', { name: 'Upload document' })
  await expect(upload).toBeDisabled()

  await page.locator('input[type="file"]').setInputFiles({ name: 'contract.pdf', mimeType: 'application/pdf', buffer: Buffer.from('%PDF-1.4 test') })
  await expect(upload).toBeEnabled()
})

test('uploads with the multipart + idempotency contract and opens the new document', async ({ page }) => {
  await mockConsoleApi(page)
  let captured: { idempotency: string | undefined; body: string; contentType: string | undefined } | null = null
  await page.route('**/api/v1/documents/upload', async (route) => {
    const request = route.request()
    captured = {
      idempotency: request.headers()['idempotency-key'],
      body: request.postDataBuffer()?.toString('latin1') ?? '',
      contentType: request.headers()['content-type'],
    }
    await route.fulfill({ json: { success: true, data: document } })
  })
  // The app navigates to the new document with a full page load.
  await page.route(`**/api/v1/documents/${document.id}/processing-jobs`, (route) => route.fulfill({ json: { success: true, data: [] } }))
  await page.route(`**/api/v1/documents/${document.id}`, (route) => route.fulfill({ json: { success: true, data: document } }))

  await page.goto('/documents')
  await page.locator('input[type="file"]').setInputFiles({ name: 'contract.pdf', mimeType: 'application/pdf', buffer: Buffer.from('%PDF-1.4 test') })
  await page.getByRole('button', { name: 'Upload document' }).click()

  await expect(page).toHaveURL(new RegExp(`/documents/${document.id}$`))
  await expect(page.getByRole('heading', { level: 1, name: document.original_filename })).toBeVisible()

  expect(captured).not.toBeNull()
  expect(captured!.idempotency).toMatch(/^[0-9a-f-]{36}$/)
  expect(captured!.contentType).toContain('multipart/form-data')
  expect(captured!.body).toContain('contract.pdf')
})

test('surfaces a bounded upload error and keeps the page usable', async ({ page }) => {
  await mockConsoleApi(page)
  await page.route('**/api/v1/documents/upload', (route) => route.fulfill({
    status: 500,
    json: { code: 'internal_error', message: 'storage unavailable' },
  }))

  await page.goto('/documents')
  await page.locator('input[type="file"]').setInputFiles({ name: 'contract.pdf', mimeType: 'application/pdf', buffer: Buffer.from('%PDF-1.4 test') })
  await page.getByRole('button', { name: 'Upload document' }).click()

  await expect(page.getByText('Upload failed')).toBeVisible()
  await expect(page.getByText('storage unavailable')).toBeVisible()
  await expect(page.getByRole('link', { name: document.original_filename })).toBeVisible()
})

test('empty register explains the next step', async ({ page }) => {
  await mockConsoleApi(page, { documents: [] })
  await page.goto('/documents')
  await expect(page.getByText('No documents yet')).toBeVisible()
  await expect(page.getByText('Upload the first document to make the pipeline visible.')).toBeVisible()
})
