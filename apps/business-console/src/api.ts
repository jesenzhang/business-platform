import type { ApiEnvelope, AuditEvent, Candidate, Document, IntegrityFinding, OperationsOverview, Page, ProcessingJob, ReviewDecision, ReviewResult } from './contracts'

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
