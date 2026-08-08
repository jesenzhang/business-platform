import { describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../src/api'

describe('public REST client', () => {
  it('unwraps the stable API envelope', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ success: true, data: { ok: true } }), { status: 200 })))
    await expect(apiFetch<{ ok: boolean }>('/api/v1/operations/overview')).resolves.toEqual({ ok: true })
    expect(fetch).toHaveBeenCalledWith(expect.stringContaining('/api/v1/operations/overview'), expect.objectContaining({ headers: expect.objectContaining({ Authorization: 'Bearer dev-only-secret' }) }))
  })

  it('maps bounded API errors without exposing raw transport details', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ code: 'unauthorized', message: 'authentication required' }), { status: 401 })))
    await expect(apiFetch('/api/v1/documents')).rejects.toEqual(expect.objectContaining({ status: 401, code: 'unauthorized', message: 'authentication required' }))
  })
})
