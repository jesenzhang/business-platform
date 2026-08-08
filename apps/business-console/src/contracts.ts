export type ApiEnvelope<T> = { success: boolean; data?: T; message?: string }
export type Page<T> = { items: T[]; next_cursor?: string | null }

export type Document = {
  id: string
  original_filename: string
  content_type: string
  status: string
  version: number
  content_revision: number
  size_bytes?: number | null
  created_at: string
  updated_at: string
}

export type ProcessingJob = {
  job_id: string
  document_id: string
  content_revision: number
  status: string
  current_step: string
  attempt_count: number
  failure_code?: string | null
  cancel_requested: boolean
  candidate_available: boolean
  review_available: boolean
  created_at: string
  updated_at: string
}

export type Candidate = {
  candidate_id: string
  job_id: string
  content_revision: number
  schema_version: string
  payload: Record<string, unknown>
  evidence: Array<Record<string, unknown>>
  provider: string
  model: string
  prompt_version: string
  version: number
  created_at: string
}

export type Review = {
  id: string
  candidate_id: string
  decision: string
  patch?: Record<string, unknown> | null
  comment?: string | null
  candidate_version: number
  created_at: string
}

export type ReviewResult = { review: Review; replayed: boolean }

export type IntegrityFinding = {
  id: string
  rule_id: string
  bounded_context: string
  resource_type: string
  resource_id: string
  severity: string
  status: string
  repairability: string
  first_detected_at: string
  last_detected_at: string
  occurrence_count: number
  version: number
}

export type AuditEvent = {
  id: string
  action: string
  resource_type: string
  resource_id: string
  result: string
  failure_code?: string | null
  trace_id?: string | null
  occurred_at: string
  stream_sequence: number
  schema_version: string
  details?: Record<string, unknown> | null
}

export type OperationsOverview = {
  document_total: number
  document_created_today: number
  processing_by_status: Record<string, number>
  review_pending: number
  unresolved_findings: number
  recent_jobs: ProcessingJob[]
  recent_audit_events: AuditEvent[]
}

export type ReviewDecision = 'accepted' | 'rejected'
