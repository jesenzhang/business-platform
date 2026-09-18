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

// --- PLAN-0013 Identity & Authorization management surface (Stage 9) ---

export type AdminUser = {
  user_id: string
  status: string
  created_at: string
  updated_at: string
  version: number
}

export type MembershipView = {
  membership_id: string
  user_id: string
  status: string
  joined_at: string
  suspended_at?: string | null
  source: string
  version: number
}

export type PermissionView = {
  key: string
  description: string
  reserved: boolean
  active: boolean
}

export type RoleView = {
  role_id: string
  tenant_id?: string | null
  stable_key: string
  display_name: string
  status: string
  system: boolean
  permission_keys: string[]
  created_at: string
  updated_at: string
  version: number
}

export type ScopeView =
  | { scope: 'tenant' }
  | { scope: 'org_unit'; org_unit_id: string; include_subtree: boolean }
  | { scope: 'resource_type'; resource_kind: string }
  | { scope: 'resource'; resource_kind: string; resource_id: string }

export type RoleBindingView = {
  binding_id: string
  user_id: string
  role_id: string
  scope: ScopeView
  status: string
  effective_at: string
  expires_at?: string | null
  created_at: string
  updated_at: string
  version: number
}

export type OrganizationUnitView = {
  unit_id: string
  parent_id?: string | null
  unit_type: string
  name: string
  status: string
  created_at: string
  updated_at: string
  version: number
}

export type OrganizationMemberView = {
  membership_id: string
  user_id: string
  unit_id: string
  membership_type: string
  status: string
  joined_at: string
  deactivated_at?: string | null
  version: number
}

export type ExplainEvaluation = {
  binding_id: string
  role_id: string
  binding_active: boolean
  within_validity: boolean
  role_grants_permission: boolean
  outcome: string
}

export type ExplainView = {
  allowed: boolean
  reason: string
  matched_binding?: string | null
  matched_permission?: string | null
  matched_scope?: ScopeView | null
  policy_reference: string
  evaluations: ExplainEvaluation[]
}
