import { expect, test } from '@playwright/test'

const envelope = <T>(data: T) => ({ success: true, data })

test('identity & authorization views render from mocked admin APIs', async ({ page }) => {
  await page.route('**/api/v1/admin/users*', async (route) => route.fulfill({ json: envelope({ items: [], next_cursor: null }) }))
  await page.route('**/api/v1/admin/tenant-memberships*', async (route) => route.fulfill({ json: envelope({ items: [], next_cursor: null }) }))
  await page.route('**/api/v1/admin/permissions', async (route) => route.fulfill({ json: envelope([{ key: 'identity.read', description: 'read identities', reserved: false, active: true }]) }))
  await page.route('**/api/v1/admin/roles', async (route) => route.fulfill({ json: envelope([{ role_id: '11111111-1111-7111-8111-111111111111', stable_key: 'contract-reviewer', display_name: 'Contract Reviewer', status: 'active', system: false, permission_keys: ['identity.read'], created_at: new Date().toISOString(), updated_at: new Date().toISOString(), version: 1 }]) }))
  await page.route('**/api/v1/admin/role-bindings*', async (route) => route.fulfill({ json: envelope([]) }))
  await page.route('**/api/v1/admin/organization-units*', async (route) => route.fulfill({ json: envelope([]) }))

  await page.goto('/identities')
  await expect(page.getByRole('heading', { name: 'Identities' })).toBeVisible()
  await expect(page.getByText('No memberships yet.')).toBeVisible()

  await page.goto('/roles')
  await expect(page.getByRole('heading', { name: 'Roles & permissions' })).toBeVisible()
  await expect(page.getByText('Contract Reviewer')).toBeVisible()

  await page.goto('/bindings')
  await expect(page.getByRole('heading', { name: 'Role bindings' })).toBeVisible()

  await page.goto('/organization')
  await expect(page.getByRole('heading', { name: 'Organization units' })).toBeVisible()

  await page.goto('/explain')
  await expect(page.getByRole('heading', { name: 'Explain a decision' })).toBeVisible()
})

test('deny explanations render the bounded reason without internals', async ({ page }) => {
  await page.route('**/api/v1/admin/authorization/explain', async (route) => route.fulfill({
    json: envelope({ allowed: false, reason: 'deny_no_binding', policy_reference: 'policy:v1', evaluations: [] }),
  }))
  await page.goto('/explain')
  await page.getByPlaceholder('User id').fill('22222222-2222-7222-8222-222222222222')
  await page.getByPlaceholder('permission.key').fill('audit.read')
  await page.getByRole('button', { name: 'Explain' }).click()
  await expect(page.getByText('deny_no_binding')).toBeVisible()
  await expect(page.getByText('policy:v1')).toBeVisible()
})
