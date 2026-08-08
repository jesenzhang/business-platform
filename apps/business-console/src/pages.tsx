import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useParams } from 'react-router-dom'
import ReactECharts from 'echarts-for-react'
import { flexRender, getCoreRowModel, useReactTable, type ColumnDef } from '@tanstack/react-table'
import { ApiError, getCandidate, getDocument, getJob, getOverview, listAudit, listDocumentJobs, listDocuments, listFindings, listJobs, startProcessing, submitReview, uploadDocument } from './api'
import type { AuditEvent, Candidate, Document, IntegrityFinding, ProcessingJob } from './contracts'
import { Empty, ErrorState, IdLink, Loading, PageHeader, StatCard, StatusPill } from './components'

const date = (value: string) => new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
const bytes = (value?: number | null) => value == null ? '—' : `${(value / 1024).toFixed(1)} KB`

export function DashboardPage() {
  const query = useQuery({ queryKey: ['overview'], queryFn: getOverview, refetchInterval: 10_000 })
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  const overview = query.data
  const chartOption = {
    color: ['#0071e3', '#34c759', '#ff9f0a', '#af52de', '#ff375f'],
    tooltip: { trigger: 'axis' },
    grid: { left: 8, right: 16, top: 12, bottom: 24, containLabel: true },
    xAxis: { type: 'category', data: Object.keys(overview.processing_by_status).map((key) => key.replaceAll('_', ' ')), axisLabel: { color: '#6e6e73' } },
    yAxis: { type: 'value', minInterval: 1, splitLine: { lineStyle: { color: '#e5e5ea' } }, axisLabel: { color: '#6e6e73' } },
    series: [{ type: 'bar', barWidth: 24, data: Object.values(overview.processing_by_status), itemStyle: { borderRadius: [6, 6, 0, 0] } }],
  }
  return <>
    <PageHeader eyebrow="LIVE OPERATIONS" title="Good morning, operator." description="A calm view of document intelligence, processing health, and governance signals." action={<span className="live-badge"><span className="status-dot" /> Live</span>} />
    <div className="stats-grid">
      <StatCard label="Documents" value={overview.document_total} detail={`+${overview.document_created_today} in the last day`} />
      <StatCard label="Review queue" value={overview.review_pending} detail="Candidates awaiting a decision" tone="orange" />
      <StatCard label="Open findings" value={overview.unresolved_findings} detail="Integrity signals needing attention" tone="purple" />
      <StatCard label="Succeeded jobs" value={overview.processing_by_status.succeeded ?? 0} detail="Durable execution history" tone="green" />
    </div>
    <div className="dashboard-grid">
      <article className="panel chart-panel"><div className="panel-heading"><div><h2>Processing flow</h2><p>Current durable job states</p></div><Link className="text-button" to="/processing">View jobs →</Link></div><ReactECharts option={chartOption} style={{ height: 260 }} /></article>
      <article className="panel"><div className="panel-heading"><div><h2>Recent activity</h2><p>Tenant-scoped audit trail</p></div><Link className="text-button" to="/audit">Open audit →</Link></div><AuditList events={overview.recent_audit_events} /></article>
    </div>
    <article className="panel"><div className="panel-heading"><div><h2>Recently moving</h2><p>Jobs that changed state most recently</p></div><Link className="text-button" to="/processing">All processing →</Link></div><JobsTable jobs={overview.recent_jobs} /></article>
  </>
}

export function DocumentsPage() {
  const [selectedFile, setSelectedFile] = useState<File | null>(null)
  const queryClient = useQueryClient()
  const query = useQuery({ queryKey: ['documents'], queryFn: () => listDocuments(50) })
  const upload = useMutation({ mutationFn: (file: File) => uploadDocument(file), onSuccess: (document) => { setSelectedFile(null); void queryClient.invalidateQueries({ queryKey: ['documents'] }); void queryClient.invalidateQueries({ queryKey: ['overview'] }); window.location.href = `/documents/${document.id}` } })
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  return <>
    <PageHeader eyebrow="DOCUMENT MANAGEMENT" title="Documents" description="Content revisions are owned by Document Management. Processing reads them through a durable job." />
    <div className="upload-panel panel"><div><h2>Bring a document into the pipeline</h2><p>PDF, TXT, DOC, and DOCX up to 10 MiB. Storage keys are generated server-side.</p></div><div className="upload-control"><label className="file-input"><input type="file" accept=".pdf,.txt,.doc,.docx,application/pdf,text/plain,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document" onChange={(event) => setSelectedFile(event.target.files?.[0] ?? null)} /><span>{selectedFile ? selectedFile.name : 'Choose a file'}</span></label><button className="primary-button" disabled={!selectedFile || upload.isPending} onClick={() => selectedFile && upload.mutate(selectedFile)}>{upload.isPending ? 'Uploading…' : 'Upload document'}</button></div>{upload.isError && <div className="inline-error">{upload.error instanceof ApiError ? upload.error.message : 'Upload failed. Check the API connection.'}</div>}</div>
    <article className="panel"><div className="panel-heading"><div><h2>Document register</h2><p>{query.data.items.length} records in this page</p></div><span className="subtle">All records are tenant-scoped</span></div><DocumentTable documents={query.data.items} /></article>
  </>
}

function DocumentTable({ documents }: { documents: Document[] }) {
  const columns = useMemo<ColumnDef<Document>[]>(() => [
    { accessorKey: 'original_filename', header: 'Document', cell: ({ row }) => <div><Link className="table-primary" to={`/documents/${row.original.id}`}>{row.original.original_filename}</Link><span className="table-secondary">{row.original.content_type}</span></div> },
    { accessorKey: 'status', header: 'Status', cell: ({ row }) => <StatusPill value={row.original.status} /> },
    { accessorKey: 'content_revision', header: 'Revision' },
    { accessorKey: 'size_bytes', header: 'Size', cell: ({ row }) => bytes(row.original.size_bytes) },
    { accessorKey: 'updated_at', header: 'Updated', cell: ({ row }) => date(row.original.updated_at) },
  ], [])
  const table = useReactTable({ data: documents, columns, getCoreRowModel: getCoreRowModel() })
  if (!documents.length) return <Empty>Upload the first document to make the pipeline visible.</Empty>
  return <div className="table-wrap"><table><thead>{table.getHeaderGroups().map((group) => <tr key={group.id}>{group.headers.map((header) => <th key={header.id}>{flexRender(header.column.columnDef.header, header.getContext())}</th>)}</tr>)}</thead><tbody>{table.getRowModel().rows.map((row) => <tr key={row.id}>{row.getVisibleCells().map((cell) => <td key={cell.id}>{flexRender(cell.column.columnDef.cell, cell.getContext())}</td>)}</tr>)}</tbody></table></div>
}

export function DocumentDetailPage() {
  const { documentId = '' } = useParams()
  const queryClient = useQueryClient()
  const documentQuery = useQuery({ queryKey: ['document', documentId], queryFn: () => getDocument(documentId), enabled: Boolean(documentId) })
  const jobsQuery = useQuery({ queryKey: ['document-jobs', documentId], queryFn: () => listDocumentJobs(documentId), enabled: Boolean(documentId) })
  const start = useMutation({ mutationFn: () => startProcessing(documentId, documentQuery.data?.content_revision ?? 0), onSuccess: (job) => { void queryClient.invalidateQueries({ queryKey: ['document-jobs', documentId] }); void queryClient.invalidateQueries({ queryKey: ['overview'] }); window.location.href = `/processing/${job.job_id}` } })
  if (documentQuery.isPending || jobsQuery.isPending) return <Loading />
  if (documentQuery.isError) return <ErrorState error={documentQuery.error} />
  const document = documentQuery.data
  return <><PageHeader eyebrow="DOCUMENT DETAIL" title={document.original_filename} description={`${document.content_type} · revision ${document.content_revision}`} action={<button className="primary-button" disabled={start.isPending} onClick={() => start.mutate()}>{start.isPending ? 'Starting…' : 'Start processing'}</button>} /><div className="detail-grid"><article className="panel detail-card"><span className="eyebrow">DOCUMENT</span><h2>{document.original_filename}</h2><StatusPill value={document.status} /><dl><dt>Document ID</dt><dd className="mono">{document.id}</dd><dt>Size</dt><dd>{bytes(document.size_bytes)}</dd><dt>Created</dt><dd>{date(document.created_at)}</dd><dt>Updated</dt><dd>{date(document.updated_at)}</dd></dl></article><article className="panel"><div className="panel-heading"><div><h2>Processing history</h2><p>Each revision creates its own durable execution trail.</p></div></div>{jobsQuery.data?.length ? <JobsTable jobs={jobsQuery.data} /> : <Empty>No processing job has been started for this document.</Empty>}</article></div>{start.isError && <div className="inline-error">{start.error.message}</div>}</>
}

export function ProcessingPage() {
  const { jobId } = useParams()
  const query = useQuery<ProcessingJob | { items: ProcessingJob[] }>({ queryKey: ['jobs', jobId ?? 'all'], queryFn: () => jobId ? getJob(jobId) : listJobs(50) })
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  if (jobId) return <JobDetail job={query.data as ProcessingJob} />
  const page = query.data as { items: ProcessingJob[] }
  return <><PageHeader eyebrow="DURABLE EXECUTION" title="Processing" description="Leases, retries, checkpoints, and review boundaries stay in the processing context." /><article className="panel"><div className="panel-heading"><div><h2>Job queue</h2><p>{page.items.length} recent jobs</p></div></div><JobsTable jobs={page.items} /></article></>
}

function JobDetail({ job }: { job: ProcessingJob }) {
  return <><PageHeader eyebrow="PROCESSING JOB" title={job.current_step.replaceAll('_', ' ')} description={<span className="mono">{job.job_id}</span>} action={<StatusPill value={job.status} />} /><div className="detail-grid"><article className="panel detail-card"><span className="eyebrow">EXECUTION STATE</span><dl><dt>Document</dt><dd><IdLink id={job.document_id} to={`/documents/${job.document_id}`} /></dd><dt>Content revision</dt><dd>{job.content_revision}</dd><dt>Current step</dt><dd>{job.current_step.replaceAll('_', ' ')}</dd><dt>Attempts</dt><dd>{job.attempt_count}</dd><dt>Updated</dt><dd>{date(job.updated_at)}</dd></dl></article><article className="panel pipeline-card"><span className="eyebrow">FIXED PIPELINE</span><div className="pipeline">{['validate_source', 'detect_type', 'extract_text', 'extract_fields', 'validate_candidate', 'await_review'].map((step, index) => <div key={step} className={step === job.current_step ? 'pipeline-step current' : index < 2 ? 'pipeline-step done' : 'pipeline-step'}><span>{index + 1}</span>{step.replaceAll('_', ' ')}</div>)}</div>{job.candidate_available && <Link className="primary-button block-button" to={`/processing/${job.job_id}/candidate`}>Review candidate →</Link>}</article></div></>
}

export function CandidatePage() {
  const { jobId = '' } = useParams()
  const candidateQuery = useQuery({ queryKey: ['candidate', jobId], queryFn: () => getCandidate(jobId), enabled: Boolean(jobId) })
  if (candidateQuery.isPending) return <Loading />
  if (candidateQuery.isError) return <ErrorState error={candidateQuery.error} />
  return <CandidateReview candidate={candidateQuery.data} />
}

function CandidateReview({ candidate }: { candidate: Candidate }) {
  const [comment, setComment] = useState('')
  const [submitted, setSubmitted] = useState<string | null>(null)
  const mutation = useMutation({ mutationFn: (decision: 'accepted' | 'rejected') => submitReview(candidate.job_id, decision, candidate.version, comment), onSuccess: (_, decision) => setSubmitted(decision) })
  return <><PageHeader eyebrow="HUMAN REVIEW" title="Candidate review" description="Review a bounded candidate with evidence before it becomes a business decision." action={<StatusPill value={submitted ?? 'waiting_for_review'} />} /><div className="review-grid"><article className="panel"><div className="panel-heading"><div><h2>Extracted fields</h2><p>Schema {candidate.schema_version} · candidate v{candidate.version}</p></div></div><pre className="json-view">{JSON.stringify(candidate.payload, null, 2)}</pre></article><article className="panel review-sidebar"><div className="panel-heading"><div><h2>Decision</h2><p>Provider metadata is bounded and read-only.</p></div></div><dl><dt>Provider</dt><dd>{candidate.provider}</dd><dt>Model</dt><dd>{candidate.model}</dd><dt>Evidence</dt><dd>{candidate.evidence.length} references</dd></dl><label className="field-label" htmlFor="review-comment">Comment</label><textarea id="review-comment" rows={4} value={comment} onChange={(event) => setComment(event.target.value)} placeholder="Optional reviewer context" /><div className="button-row"><button className="secondary-button" disabled={mutation.isPending || Boolean(submitted)} onClick={() => mutation.mutate('rejected')}>Reject</button><button className="primary-button" disabled={mutation.isPending || Boolean(submitted)} onClick={() => mutation.mutate('accepted')}>Accept candidate</button></div>{mutation.isError && <div className="inline-error">{mutation.error.message}</div>}{submitted && <div className="success-note">Decision recorded. The application service handled versioning, idempotency, and audit.</div>}</article></div></>
}

export function FindingsPage() {
  const query = useQuery({ queryKey: ['findings'], queryFn: () => listFindings(50) })
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  return <><PageHeader eyebrow="DATA GOVERNANCE" title="Integrity findings" description="Findings are evidence of a consistency problem, not an instruction to bypass the owning context." action={<Link className="secondary-button" to="/repairs">Repair controls →</Link>} /><article className="panel"><div className="panel-heading"><div><h2>Open signals</h2><p>{query.data.items.length} bounded findings</p></div></div><FindingsTable findings={query.data.items} /></article></>
}

function FindingsTable({ findings }: { findings: IntegrityFinding[] }) {
  if (!findings.length) return <Empty>Integrity is quiet in this tenant.</Empty>
  return <div className="table-wrap"><table><thead><tr><th>Rule</th><th>Resource</th><th>Severity</th><th>Status</th><th>Occurrences</th><th>Last detected</th></tr></thead><tbody>{findings.map((finding) => <tr key={finding.id}><td><span className="table-primary">{finding.rule_id}</span><span className="table-secondary">{finding.bounded_context}</span></td><td className="mono">{finding.resource_type}:{finding.resource_id.slice(0, 12)}</td><td><StatusPill value={finding.severity} /></td><td><StatusPill value={finding.status} /></td><td>{finding.occurrence_count}</td><td>{date(finding.last_detected_at)}</td></tr>)}</tbody></table></div>
}

export function RepairsPage() {
  const query = useQuery({ queryKey: ['findings', 'repair'], queryFn: () => listFindings(50) })
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  return <><PageHeader eyebrow="CONTROLLED CHANGE" title="Repairs" description="Prepare → Preview → Confirm → Execute. This console never turns a finding into an unreviewed write." /><div className="callout"><strong>Safe operating boundary</strong><span>Repair execution remains permissioned, version-bound, and owned by the Governance application service. Use the API dry-run endpoint for a preview before approval.</span></div><article className="panel"><div className="panel-heading"><div><h2>Repair candidates</h2><p>Signals currently eligible for an operator review.</p></div></div>{query.data.items.length ? <FindingsTable findings={query.data.items.filter((item) => item.status !== 'repaired')} /> : <Empty>No repair candidates are present.</Empty>}</article></>
}

export function AuditPage() {
  const query = useQuery({ queryKey: ['audit'], queryFn: () => listAudit(50) })
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  return <><PageHeader eyebrow="RUNTIME GOVERNANCE" title="Audit" description="Immutable, tenant-scoped evidence for business and operational transitions." /><article className="panel"><div className="panel-heading"><div><h2>Event stream</h2><p>{query.data.items.length} recent events</p></div></div><AuditList events={query.data.items} expanded /></article></>
}

function JobsTable({ jobs }: { jobs: ProcessingJob[] }) {
  if (!jobs.length) return <Empty>No processing jobs in this scope.</Empty>
  return <div className="table-wrap"><table><thead><tr><th>Job</th><th>Document</th><th>Step</th><th>Status</th><th>Attempts</th><th>Updated</th></tr></thead><tbody>{jobs.map((job) => <tr key={job.job_id}><td><IdLink id={job.job_id} to={`/processing/${job.job_id}`} /></td><td><IdLink id={job.document_id} to={`/documents/${job.document_id}`} /></td><td>{job.current_step.replaceAll('_', ' ')}</td><td><StatusPill value={job.status} /></td><td>{job.attempt_count}</td><td>{date(job.updated_at)}</td></tr>)}</tbody></table></div>
}

function AuditList({ events, expanded = false }: { events: AuditEvent[]; expanded?: boolean }) {
  if (!events.length) return <Empty>No audit events yet.</Empty>
  return <div className="audit-list">{events.map((event) => <div className="audit-item" key={event.id}><div className="audit-marker" /><div className="audit-main"><div><strong>{event.action.replaceAll('_', ' ')}</strong><StatusPill value={event.result} /></div><span className="table-secondary">{event.resource_type} · {event.resource_id.slice(0, 12)} · {date(event.occurred_at)}</span>{expanded && event.failure_code && <span className="table-secondary">Failure: {event.failure_code}</span>}</div></div>)}</div>
}
