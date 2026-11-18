import { expect, test } from '@playwright/test'
import { mockConsoleApi } from './fixtures/console'

/**
 * IAM write wiring (PLAN-0016 S4-D1..D4).
 *
 * The API client unit tests already pin the request shapes; these cases pin the
 * page wiring: the optimistic version and scope the UI actually sends.
 */

const envelope = <T,>(data: T) => ({ success: true, data })

const role = {
  role_id: 'r1000000-0000-4000-8000-000000000001',
  stable_key: 'contract-reviewer',
  display_name: 'Contract Reviewer',
  status: 'active',
  system: false,
  permission_keys: ['identity.read'],
  created_at: '2026-11-01T10:00:00.000Z',
  updated_at: '2026-11-14T11:30:00.000Z',
  version: 4,
}

test('suspending a membership sends the optimistic version', async ({ page }) => {
  await mockConsoleApi(page)
  let body: Record<string, unknown> | null = null
  await page.route('**/api/v1/admin/tenant-memberships*', (route) => route.fulfill({
    json: envelope({ items: [{
      membership_id: 'm1', user_id: 'u1', status: 'active', joined_at: '2026-11-01T10:00:00.000Z',
      source: 'invite', version: 7,
    }], next_cursor: null }),
  }))
  await page.route('**/api/v1/admin/users*', (route) => route.fulfill({ json: envelope({ items: [], next_cursor: null }) }))
  await page.route('**/api/v1/admin/tenant-memberships/u1/suspend', async (route) => {
    body = route.request().postDataJSON() as Record<string, unknown>
    await route.fulfill({ json: envelope({}) })
  })

  await page.goto('/identities')
  await page.getByRole('button', { name: 'Suspend' }).click()
  await expect.poll(() => body).not.toBeNull()
  expect(body).toEqual({ expected_version: 7 })
})

test('creating a binding sends the selected scope shape', async ({ page }) => {
  await mockConsoleApi(page)
  let body: Record<string, unknown> | null = null
  await page.route('**/api/v1/admin/role-bindings', async (route) => {
    if (route.request().method() === 'POST') {
      body = route.request().postDataJSON() as Record<string, unknown>
      await route.fulfill({ json: envelope({}) })
      return
    }
    await route.fulfill({ json: envelope([]) })
  })
  await page.route('**/api/v1/admin/roles', (route) => route.fulfill({ json: envelope([role]) }))

  await page.goto('/bindings')
  await page.getByLabel('User id').fill('u-1')
  await page.getByRole('combobox', { name: 'Role' }).click()
  await page.getByText('Contract Reviewer', { exact: true }).click()
  await page.getByRole('button', { name: 'Bind role' }).click()

  await expect.poll(() => body).not.toBeNull()
  expect(body).toEqual({ user_id: 'u-1', role_id: role.role_id, scope: { scope: 'tenant' } })
})

test('revoking a binding and removing a unit member carry the resource version', async ({ page }) => {
  await mockConsoleApi(page)
  let revokeBody: Record<string, unknown> | null = null
  await page.route('**/api/v1/admin/role-bindings', (route) => route.fulfill({
    json: envelope([{
      binding_id: 'b1', user_id: 'u1', role_id: role.role_id, scope: { scope: 'tenant' },
      status: 'active', effective_at: '2026-11-01T10:00:00.000Z', created_at: '2026-11-01T10:00:00.000Z',
      updated_at: '2026-11-01T10:00:00.000Z', version: 3,
    }]),
  }))
  await page.route('**/api/v1/admin/role-bindings/**', async (route) => {
    if (route.request().url().includes('/revoke')) {
      revokeBody = route.request().postDataJSON() as Record<string, unknown>
      await route.fulfill({ json: envelope({}) })
      return
    }
    await route.fulfill({ json: envelope([]) })
  })
  await page.route('**/api/v1/admin/roles', (route) => route.fulfill({ json: envelope([role]) }))

  await page.goto('/bindings')
  await page.getByRole('button', { name: 'Revoke' }).click()
  await expect.poll(() => revokeBody).not.toBeNull()
  expect(revokeBody).toEqual({ expected_version: 3 })

  let deleteUrl = ''
  await page.route('**/api/v1/admin/organization-units', (route) => route.fulfill({
    json: envelope([{ unit_id: 'unit-1', parent_id: null, unit_type: 'department', name: 'Legal', status: 'active', created_at: '2026-11-01T10:00:00.000Z', updated_at: '2026-11-01T10:00:00.000Z', version: 2 }]),
  }))
  await page.route('**/api/v1/admin/organization-units/**', (route) => {
    if (route.request().method() === 'DELETE') {
      deleteUrl = route.request().url()
      return route.fulfill({ json: envelope({}) })
    }
    return route.fulfill({ json: envelope([{ membership_id: 'm1', user_id: 'u1', unit_id: 'unit-1', membership_type: 'leader', status: 'active', joined_at: '2026-11-01T10:00:00.000Z', version: 5 }]) })
  })

  await page.goto('/organization')
  await page.getByRole('button', { name: 'Members' }).click()
  await page.getByRole('button', { name: 'Remove' }).click()
  await expect.poll(() => deleteUrl).not.toBe('')
  expect(deleteUrl).toContain('membership_type=leader')
  expect(deleteUrl).toContain('expected_version=5')
})

test('explain renders the allow decision and the evaluated bindings', async ({ page }) => {
  await mockConsoleApi(page)
  await page.route('**/api/v1/admin/authorization/explain', (route) => route.fulfill({
    json: envelope({
      allowed: true,
      reason: 'allow_binding',
      matched_binding: 'b1',
      matched_permission: 'audit.read',
      matched_scope: { scope: 'tenant' },
      policy_reference: 'policy:v1',
      evaluations: [{ binding_id: 'b1', role_id: role.role_id, binding_active: true, within_validity: true, role_grants_permission: true, outcome: 'allow' }],
    }),
  }))

  await page.goto('/explain')
  await page.getByPlaceholder('User id').fill('u-1')
  await page.getByRole('button', { name: 'Explain' }).click()

  await expect(page.getByText('allow_binding')).toBeVisible()
  await expect(page.getByText('policy:v1')).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Bindings considered' })).toBeVisible()
  await expect(page.getByText('1 candidate evaluations')).toBeVisible()
})
