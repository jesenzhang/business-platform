import { beforeEach, describe, expect, it, vi } from 'vitest'

const fetchMock = () => vi.mocked(fetch)
import { createRoleBinding, explainDecision, listAdminUsers, removeUnitMember, suspendMembership } from '../src/api'

describe('identity & authorization REST client', () => {
  beforeEach(() => {
    vi.unstubAllGlobals()
  })

  it('sends bounded page queries for admin users', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ success: true, data: { items: [], next_cursor: null } }), { status: 200 })))
    await expect(listAdminUsers(50)).resolves.toEqual({ items: [], next_cursor: null })
    expect(fetch).toHaveBeenCalledWith(expect.stringContaining('/api/v1/admin/users?limit=50'), expect.anything())
  })

  it('sends a fresh Idempotency-Key with every management write', async () => {
    const first = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ success: true, data: { binding_id: 'b' } }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ success: true, data: { binding_id: 'b' } }), { status: 200 }))
    vi.stubGlobal('fetch', first)
    await createRoleBinding({ user_id: 'u', role_id: 'r', scope: { scope: 'tenant' } })
    await createRoleBinding({ user_id: 'u', role_id: 'r', scope: { scope: 'tenant' } })
    const keys = fetchMock().mock.calls.map((call) => new Headers((call[1] as RequestInit).headers).get('Idempotency-Key'))
    expect(keys[0]).toMatch(/^[0-9a-f-]{36}$/)
    expect(keys[1]).toMatch(/^[0-9a-f-]{36}$/)
    expect(keys[0]).not.toBe(keys[1])
  })

  it('carries optimistic versions into membership status changes', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ success: true, data: {} }), { status: 200 })))
    await suspendMembership('user-1', 7)
    const [url, init] = fetchMock().mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toContain('/api/v1/admin/tenant-memberships/user-1/suspend')
    expect(init.method).toBe('POST')
    expect(JSON.parse(String(init.body))).toEqual({ expected_version: 7 })
  })

  it('passes version and membership type through DELETE query parameters', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ success: true, data: {} }), { status: 200 })))
    await removeUnitMember('unit-1', 'user-1', 'leader', 3)
    const [url, init] = fetchMock().mock.calls[0] as unknown as [string, RequestInit]
    expect(init.method).toBe('DELETE')
    expect(url).toContain('membership_type=leader')
    expect(url).toContain('expected_version=3')
  })

  it('explains decisions through the bounded explain contract', async () => {
    const view = { allowed: false, reason: 'deny_no_binding', policy_reference: 'policy:v1', evaluations: [] }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ success: true, data: view }), { status: 200 })))
    await expect(explainDecision({ user_id: 'u', permission: 'audit.read' })).resolves.toEqual(view)
    const [url, init] = fetchMock().mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toContain('/api/v1/admin/authorization/explain')
    expect(JSON.parse(String(init.body))).toEqual({ user_id: 'u', permission: 'audit.read' })
  })
})
