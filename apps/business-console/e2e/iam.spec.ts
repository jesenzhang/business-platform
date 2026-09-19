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

test('editing role permissions keeps existing keys in the set-replacement', async ({ page }) => {
  const roleId = '33333333-3333-7333-8333-333333333333'
  const role = {
    role_id: roleId, stable_key: 'contract-reviewer', display_name: 'Contract Reviewer',
    status: 'active', system: false, permission_keys: ['identity.read', 'audit.read'],
    created_at: new Date().toISOString(), updated_at: new Date().toISOString(), version: 4,
  }
  const permission = (key: string) => ({ key, description: key, reserved: false, active: true })
  await page.route('**/api/v1/admin/permissions', async (route) => route.fulfill({
    json: envelope([permission('identity.read'), permission('audit.read'), permission('policy.explain')]),
  }))
  await page.route('**/api/v1/admin/roles', async (route) => route.fulfill({ json: envelope([role]) }))
  const saves: string[] = []
  await page.route(`**/api/v1/admin/roles/${roleId}/permissions`, async (route) => {
    const body = route.request().postData() ?? '{}'
    saves.push(body)
    await route.fulfill({ json: envelope({ ...role, permission_keys: JSON.parse(body).permission_keys ?? [], version: role.version + 1 }) })
  })

  await page.goto('/roles')
  await page.getByRole('button', { name: 'Edit permissions' }).click()
  const editPanel = page.locator('.nested-panel')
  await expect(editPanel.getByRole('heading', { name: 'Permissions for Contract Reviewer' })).toBeVisible()
  // The role's current keys must render as checked when the panel opens.
  await expect(editPanel.locator('label.chip', { hasText: 'identity.read' }).locator('input')).toBeChecked()
  await expect(editPanel.locator('label.chip', { hasText: 'audit.read' }).locator('input')).toBeChecked()

  // Adding a key must send the full replacement: existing keys plus the new one.
  await editPanel.locator('label.chip', { hasText: 'policy.explain' }).click()
  await page.getByRole('button', { name: 'Save permissions' }).click()
  await expect.poll(() => saves.length).toBe(1)
  expect(JSON.parse(saves[0]).permission_keys.sort()).toEqual(['audit.read', 'identity.read', 'policy.explain'])
  expect(JSON.parse(saves[0]).expected_version).toBe(4)

  // Re-opening and saving without touching anything must round-trip the
  // current set intact — never an empty replacement that wipes the role.
  await page.getByRole('button', { name: 'Edit permissions' }).click()
  await page.getByRole('button', { name: 'Save permissions' }).click()
  await expect.poll(() => saves.length).toBe(2)
  expect(JSON.parse(saves[1]).permission_keys.sort()).toEqual(['audit.read', 'identity.read'])
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
