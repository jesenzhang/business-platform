import type { AdminUser, ApiEnvelope, AuditEvent, Candidate, Document, ExplainView, IntegrityFinding, MembershipView, OperationsOverview, OrganizationMemberView, OrganizationUnitView, Page, PermissionView, ProcessingJob, ReviewDecision, ReviewResult, RoleBindingView, RoleView, ScopeView } from './contracts'

const baseUrl = (import.meta.env.VITE_BUSINESS_API_BASE_URL ?? 'http://localhost:3000').replace(/\/$/, '')
const defaultToken = import.meta.env.VITE_BUSINESS_API_TOKEN ?? 'dev-only-secret'

export class ApiError extends Error {
  constructor(public readonly status: number, public readonly code: string, message: string) {
    super(message)
    this.name = 'ApiError'
  }
}

export type RequestOptions = RequestInit & { token?: string }

export async function apiFetch<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { token = defaultToken, headers, ...init } = options
  const response = await fetch(`${baseUrl}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: 'application/json',
      'X-Request-ID': crypto.randomUUID(),
      ...headers,
    },
  })
  const body = await response.json().catch(() => null) as ApiEnvelope<T> | { code?: string; message?: string } | null
  if (!response.ok) {
    const errorBody = body as { code?: string; message?: string } | null
    throw new ApiError(response.status, errorBody?.code ?? 'upstream_error', errorBody?.message ?? 'Business API request failed')
  }
  const envelope = body as ApiEnvelope<T>
  if (envelope && 'success' in envelope && envelope.success === false) throw new ApiError(response.status, 'business_error', envelope.message ?? 'Business API request failed')
  return envelope && 'data' in envelope && envelope.data !== undefined ? envelope.data : body as T
}

export const getOverview = () => apiFetch<OperationsOverview>('/api/v1/operations/overview')
export const listDocuments = (limit = 50) => apiFetch<Page<Document>>(`/api/v1/documents?limit=${limit}`)
export const getDocument = (id: string) => apiFetch<Document>(`/api/v1/documents/${id}`)
export const listJobs = (limit = 50) => apiFetch<Page<ProcessingJob>>(`/api/v1/processing-jobs?limit=${limit}`)
export const listDocumentJobs = (id: string) => apiFetch<ProcessingJob[]>(`/api/v1/documents/${id}/processing-jobs`)
export const getJob = (id: string) => apiFetch<ProcessingJob>(`/api/v1/processing-jobs/${id}`)
export const getCandidate = (id: string) => apiFetch<Candidate>(`/api/v1/processing-jobs/${id}/candidate`)
export const listFindings = (limit = 50) => apiFetch<Page<IntegrityFinding>>(`/api/v1/admin/integrity/findings?limit=${limit}`)
export const listAudit = (limit = 50) => apiFetch<Page<AuditEvent>>(`/api/v1/admin/audit-events?limit=${limit}`)

export async function uploadDocument(file: File): Promise<Document> {
  const form = new FormData()
  form.append('file', file)
  return apiFetch<Document>('/api/v1/documents/upload', {
    method: 'POST',
    body: form,
    headers: { 'Idempotency-Key': crypto.randomUUID() },
  })
}

export const startProcessing = (documentId: string, contentRevision: number) => apiFetch<ProcessingJob>(`/api/v1/documents/${documentId}/processing-jobs`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json', 'Idempotency-Key': crypto.randomUUID() },
  body: JSON.stringify({ content_revision: contentRevision }),
})

export const submitReview = (jobId: string, decision: ReviewDecision, candidateVersion: number, comment?: string) => apiFetch<ReviewResult>(`/api/v1/processing-jobs/${jobId}/review`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json', 'Idempotency-Key': crypto.randomUUID() },
  body: JSON.stringify({ decision, candidate_version: candidateVersion, comment }),
})

// --- PLAN-0013 Identity & Authorization management (Stage 9) ---
const idempotencyHeaders = (): Record<string, string> => ({
  'Content-Type': 'application/json',
  'Idempotency-Key': crypto.randomUUID(),
})

const postJson = <T>(path: string, body: unknown) =>
  apiFetch<T>(path, { method: 'POST', headers: idempotencyHeaders(), body: JSON.stringify(body) })

export const listAdminUsers = (limit = 50, cursor?: string | null) =>
  apiFetch<Page<AdminUser>>(`/api/v1/admin/users?limit=${limit}${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`)
export const getAdminUser = (userId: string) => apiFetch<AdminUser>(`/api/v1/admin/users/${userId}`)

export const listTenantMemberships = (limit = 50, cursor?: string | null) =>
  apiFetch<Page<MembershipView>>(`/api/v1/admin/tenant-memberships?limit=${limit}${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`)
export const createTenantMembership = (body: { user_id?: string; issuer?: string; subject?: string; reason?: string }) =>
  postJson<MembershipView>('/api/v1/admin/tenant-memberships', body)
export const suspendMembership = (userId: string, expectedVersion: number) =>
  postJson<MembershipView>(`/api/v1/admin/tenant-memberships/${userId}/suspend`, { expected_version: expectedVersion })
export const reactivateMembership = (userId: string, expectedVersion: number) =>
  postJson<MembershipView>(`/api/v1/admin/tenant-memberships/${userId}/reactivate`, { expected_version: expectedVersion })

export const listPermissions = () => apiFetch<PermissionView[]>('/api/v1/admin/permissions')
export const listRoles = () => apiFetch<RoleView[]>('/api/v1/admin/roles')
export const getRole = (roleId: string) => apiFetch<RoleView>(`/api/v1/admin/roles/${roleId}`)
export const createRole = (body: { stable_key: string; display_name: string; permission_keys: string[] }) => postJson<RoleView>('/api/v1/admin/roles', body)
export const updateRole = (roleId: string, body: { display_name?: string; status?: string; expected_version: number }) =>
  apiFetch<RoleView>(`/api/v1/admin/roles/${roleId}`, { method: 'PATCH', headers: idempotencyHeaders(), body: JSON.stringify(body) })
export const setRolePermissions = (roleId: string, permissionKeys: string[], expectedVersion: number) =>
  apiFetch<RoleView>(`/api/v1/admin/roles/${roleId}/permissions`, { method: 'PUT', headers: idempotencyHeaders(), body: JSON.stringify({ permission_keys: permissionKeys, expected_version: expectedVersion }) })

export const listRoleBindings = (userId?: string) =>
  apiFetch<RoleBindingView[]>(`/api/v1/admin/role-bindings${userId ? `?user_id=${userId}` : ''}`)
export const createRoleBinding = (body: { user_id: string; role_id: string; scope: ScopeView; effective_at?: string; expires_at?: string }) =>
  postJson<RoleBindingView>('/api/v1/admin/role-bindings', body)
export const revokeRoleBinding = (bindingId: string, expectedVersion: number) =>
  postJson<RoleBindingView>(`/api/v1/admin/role-bindings/${bindingId}/revoke`, { expected_version: expectedVersion })

export const listOrganizationUnits = () => apiFetch<OrganizationUnitView[]>('/api/v1/admin/organization-units')
export const listUnitMembers = (unitId: string) => apiFetch<OrganizationMemberView[]>(`/api/v1/admin/organization-units/${unitId}/members`)
export const createOrganizationUnit = (body: { parent_id?: string | null; unit_type: string; name: string }) =>
  postJson<OrganizationUnitView>('/api/v1/admin/organization-units', body)
export const updateOrganizationUnit = (unitId: string, body: { name?: string; status?: string; expected_version: number }) =>
  apiFetch<OrganizationUnitView>(`/api/v1/admin/organization-units/${unitId}`, { method: 'PATCH', headers: idempotencyHeaders(), body: JSON.stringify(body) })
export const moveOrganizationUnit = (unitId: string, newParentId: string | null, expectedVersion: number) =>
  postJson<OrganizationUnitView>(`/api/v1/admin/organization-units/${unitId}/move`, { new_parent_id: newParentId, expected_version: expectedVersion })
export const addUnitMember = (unitId: string, userId: string, membershipType: string) =>
  postJson<OrganizationMemberView>(`/api/v1/admin/organization-units/${unitId}/members/${userId}`, { membership_type: membershipType })
export const removeUnitMember = (unitId: string, userId: string, membershipType: string, expectedVersion: number) =>
  apiFetch<OrganizationMemberView>(`/api/v1/admin/organization-units/${unitId}/members/${userId}?membership_type=${membershipType}&expected_version=${expectedVersion}`, { method: 'DELETE', headers: idempotencyHeaders() })

export const explainDecision = (body: { user_id: string; permission: string; resource?: { kind?: string; resource_id?: string; org_unit_id?: string } }) =>
  postJson<ExplainView>('/api/v1/admin/authorization/explain', body)
