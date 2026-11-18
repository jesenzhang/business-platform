import type { Page } from '@playwright/test'
import type {
  AuditEvent,
  Document,
  OperationsOverview,
  Page as ApiPage,
  PermissionView,
  ProcessingJob,
  RoleView,
} from '../../src/contracts'

/**
 * Deterministic synthetic fixtures for browser verification.
 *
 * Rules:
 * - no current-time values: every timestamp is a fixed literal;
 * - no tenant data, credentials, or production URLs;
 * - every payload keeps the real API envelope and DTO shape.
 */

const envelope = <T,>(data: T) => ({ success: true, data })

const job = (overrides: Partial<ProcessingJob>): ProcessingJob => ({
  job_id: 'a1000000-0000-4000-8000-000000000001',
  document_id: 'd1000000-0000-4000-8000-000000000001',
  content_revision: 1,
  status: 'running',
  current_step: 'extract_text',
  attempt_count: 1,
  failure_code: null,
  cancel_requested: false,
  candidate_available: false,
  review_available: false,
  created_at: '2026-11-16T08:00:00.000Z',
  updated_at: '2026-11-16T08:04:00.000Z',
  ...overrides,
})

const auditEvent = (overrides: Partial<AuditEvent>): AuditEvent => ({
  id: 'e1000000-0000-4000-8000-000000000001',
  action: 'document_uploaded',
  resource_type: 'document',
  resource_id: 'd1000000-0000-4000-8000-000000000001',
  result: 'succeeded',
  failure_code: null,
  trace_id: null,
  occurred_at: '2026-11-16T07:59:00.000Z',
  stream_sequence: 41,
  schema_version: 'v1',
  details: null,
  ...overrides,
})

export const overview: OperationsOverview = {
  document_total: 128,
  document_created_today: 6,
  processing_by_status: { queued: 3, running: 2, succeeded: 41, failed: 2, waiting_for_review: 4 },
  review_pending: 4,
  unresolved_findings: 3,
  recent_jobs: [
    job({ job_id: 'a1000000-0000-4000-8000-000000000001', status: 'succeeded', current_step: 'await_review', attempt_count: 1, candidate_available: true, review_available: true, updated_at: '2026-11-16T08:21:00.000Z' }),
    job({ job_id: 'a1000000-0000-4000-8000-000000000002', document_id: 'd1000000-0000-4000-8000-000000000002', status: 'running', current_step: 'extract_text', updated_at: '2026-11-16T08:04:00.000Z' }),
    job({ job_id: 'a1000000-0000-4000-8000-000000000003', document_id: 'd1000000-0000-4000-8000-000000000003', status: 'queued', current_step: 'validate_source', attempt_count: 0, updated_at: '2026-11-16T08:02:00.000Z' }),
  ],
  recent_audit_events: [
    auditEvent({ id: 'e1000000-0000-4000-8000-000000000001', action: 'document_uploaded', occurred_at: '2026-11-16T07:59:00.000Z' }),
    auditEvent({ id: 'e1000000-0000-4000-8000-000000000002', action: 'processing_job_started', result: 'succeeded', occurred_at: '2026-11-16T08:01:00.000Z' }),
    auditEvent({ id: 'e1000000-0000-4000-8000-000000000003', action: 'candidate_reviewed', result: 'accepted', occurred_at: '2026-11-16T08:22:00.000Z' }),
  ],
}

export const emptyOverview: OperationsOverview = {
  document_total: 0,
  document_created_today: 0,
  processing_by_status: {},
  review_pending: 0,
  unresolved_findings: 0,
  recent_jobs: [],
  recent_audit_events: [],
}

export const documents: Document[] = [
  {
    id: 'd1000000-0000-4000-8000-000000000001',
    original_filename: 'master-services-agreement.pdf',
    content_type: 'application/pdf',
    status: 'succeeded',
    version: 3,
    content_revision: 2,
    size_bytes: 245760,
    created_at: '2026-11-15T09:10:00.000Z',
    updated_at: '2026-11-16T08:21:00.000Z',
  },
  {
    id: 'd1000000-0000-4000-8000-000000000002',
    original_filename: 'supplier-onboarding.docx',
    content_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    status: 'running',
    version: 1,
    content_revision: 1,
    size_bytes: 98304,
    created_at: '2026-11-15T14:32:00.000Z',
    updated_at: '2026-11-16T08:04:00.000Z',
  },
  {
    id: 'd1000000-0000-4000-8000-000000000003',
    original_filename: 'retention-policy.txt',
    content_type: 'text/plain',
    status: 'queued',
    version: 1,
    content_revision: 1,
    size_bytes: 4096,
    created_at: '2026-11-16T07:58:00.000Z',
    updated_at: '2026-11-16T08:02:00.000Z',
  },
]

export const permissions: PermissionView[] = [
  { key: 'identity.read', description: 'Read identities', reserved: false, active: true },
  { key: 'audit.read', description: 'Read audit events', reserved: false, active: true },
  { key: 'document.read', description: 'Read documents', reserved: false, active: true },
  { key: 'policy.explain', description: 'Explain policy decisions', reserved: false, active: true },
  { key: 'iam.manage', description: 'Manage identity and authorization', reserved: true, active: true },
]

export const roles: RoleView[] = [
  {
    role_id: 'r1000000-0000-4000-8000-000000000001',
    tenant_id: 't1000000-0000-4000-8000-000000000001',
    stable_key: 'contract-reviewer',
    display_name: 'Contract Reviewer',
    status: 'active',
    system: false,
    permission_keys: ['document.read', 'audit.read'],
    created_at: '2026-11-01T10:00:00.000Z',
    updated_at: '2026-11-14T11:30:00.000Z',
    version: 4,
  },
  {
    role_id: 'r1000000-0000-4000-8000-000000000002',
    tenant_id: null,
    stable_key: 'tenant-administrator',
    display_name: 'Tenant Administrator',
    status: 'active',
    system: true,
    permission_keys: ['iam.manage', 'identity.read', 'audit.read'],
    created_at: '2026-09-01T00:00:00.000Z',
    updated_at: '2026-09-01T00:00:00.000Z',
    version: 1,
  },
  {
    role_id: 'r1000000-0000-4000-8000-000000000003',
    tenant_id: 't1000000-0000-4000-8000-000000000001',
    stable_key: 'policy-auditor',
    display_name: 'Policy Auditor',
    status: 'disabled',
    system: false,
    permission_keys: [],
    created_at: '2026-11-05T09:00:00.000Z',
    updated_at: '2026-11-12T16:45:00.000Z',
    version: 2,
  },
]

export type MockConsoleOptions = {
  overview?: OperationsOverview
  documents?: Document[]
  roles?: RoleView[]
  permissions?: PermissionView[]
  overviewStatus?: number
  documentsStatus?: number
  rolesStatus?: number
}

/**
 * Route every console API call to deterministic fixtures. A catch-all is
 * registered first so an un-stubbed endpoint returns an empty page instead of
 * reaching the network; specific routes are registered afterwards and win.
 */
export async function mockConsoleApi(page: Page, options: MockConsoleOptions = {}) {
  const emptyPage: ApiPage<never> = { items: [], next_cursor: null }
  await page.route('**/api/v1/**', (route) => route.fulfill({ json: envelope(emptyPage) }))

  const overviewStatus = options.overviewStatus ?? 200
  await page.route('**/api/v1/operations/overview', (route) => route.fulfill({
    status: overviewStatus,
    json: overviewStatus === 200
      ? envelope(options.overview ?? overview)
      : { code: 'internal_error', message: 'Operations overview is temporarily unavailable' },
  }))

  const documentsStatus = options.documentsStatus ?? 200
  await page.route('**/api/v1/documents?limit=*', (route) => route.fulfill({
    status: documentsStatus,
    json: documentsStatus === 200
      ? envelope({ items: options.documents ?? documents, next_cursor: null })
      : { code: 'internal_error', message: 'Document register is temporarily unavailable' },
  }))

  const rolesStatus = options.rolesStatus ?? 200
  await page.route('**/api/v1/admin/roles', (route) => route.fulfill({
    status: rolesStatus,
    json: rolesStatus === 200
      ? envelope(options.roles ?? roles)
      : { code: 'internal_error', message: 'Role catalog is temporarily unavailable' },
  }))
  await page.route('**/api/v1/admin/permissions', (route) => route.fulfill({
    json: envelope(options.permissions ?? permissions),
  }))
}

/** Deterministic theme selection that mirrors the console's own persistence key. */
export async function useConsoleTheme(page: Page, theme: 'light' | 'dark') {
  await page.addInitScript((value) => {
    window.localStorage.setItem('business-console-theme', value)
  }, theme)
}
