import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useParams } from 'react-router-dom'
import ReactECharts from 'echarts-for-react'
import { Badge } from '@astryxdesign/core/Badge'
import { Banner } from '@astryxdesign/core/Banner'
import { Button } from '@astryxdesign/core/Button'
import { Card } from '@astryxdesign/core/Card'
import { CodeBlock } from '@astryxdesign/core/CodeBlock'
import { EmptyState } from '@astryxdesign/core/EmptyState'
import { FileInput } from '@astryxdesign/core/FileInput'
import { Grid } from '@astryxdesign/core/Grid'
import { HStack } from '@astryxdesign/core/HStack'
import { Heading } from '@astryxdesign/core/Heading'
import { MetadataList, MetadataListItem } from '@astryxdesign/core/MetadataList'
import { Link as AstryxLink } from '@astryxdesign/core/Link'
import { StatusDot } from '@astryxdesign/core/StatusDot'
import { Table, proportional } from '@astryxdesign/core/Table'
import { Text } from '@astryxdesign/core/Text'
import { TextArea } from '@astryxdesign/core/TextArea'
import { VStack } from '@astryxdesign/core/VStack'
import { ApiError, getCandidate, getDocument, getJob, getOverview, listAudit, listDocumentJobs, listDocuments, listFindings, listJobs, startProcessing, submitReview, uploadDocument } from './api'
import type { AuditEvent, Candidate, Document, IntegrityFinding, ProcessingJob } from './contracts'
import { Empty, ErrorState, IdLink, Loading, PageHeader, StatusPill } from './components'
import { useConsoleStore } from './store'

const date = (value: string) => new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
const bytes = (value?: number | null) => value == null ? '—' : `${(value / 1024).toFixed(1)} KB`

/**
 * ECharts paints on a canvas, so it cannot read CSS custom properties: each
 * colour has to be a concrete value. Resolve the design-system tokens through a
 * hidden probe (which lets the browser evaluate `light-dark()` for the active
 * scheme) instead of hard-coding light and dark palettes in the page.
 */
function resolveChartColors() {
  const probe = document.createElement('span')
  probe.style.cssText = 'position:absolute;visibility:hidden;display:block'
  document.body.appendChild(probe)
  const read = (token: string, fallback: string) => {
    probe.style.color = `var(${token})`
    const value = getComputedStyle(probe).color
    return value && value !== 'rgba(0, 0, 0, 0)' ? value : fallback
  }
  const colors = {
    axis: read('--color-text-secondary', '#6e6e73'),
    grid: read('--color-border', 'rgba(0, 0, 0, 0.08)'),
  }
  probe.remove()
  return colors
}

export function DashboardPage() {
  const query = useQuery({ queryKey: ['overview'], queryFn: getOverview, refetchInterval: 10_000 })
  const theme = useConsoleStore((state) => state.theme)
  const [chartColors, setChartColors] = useState(resolveChartColors)
  useEffect(() => { setChartColors(resolveChartColors()) }, [theme])
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  const overview = query.data
  const statuses = Object.entries(overview.processing_by_status)
  const metrics = [
    { label: 'Documents', value: overview.document_total, detail: `+${overview.document_created_today} in the last day` },
    { label: 'Review queue', value: overview.review_pending, detail: 'Candidates awaiting a decision' },
    { label: 'Open findings', value: overview.unresolved_findings, detail: 'Integrity signals needing attention' },
    { label: 'Succeeded jobs', value: overview.processing_by_status.succeeded ?? 0, detail: 'Durable execution history' },
  ]
  const chartOption = {
    // Categorical status colours stay stable across modes: they carry the
    // chart's business meaning. Only the chrome follows the theme tokens.
    color: ['#0071e3', '#34c759', '#ff9f0a', '#af52de', '#ff375f'],
    tooltip: { trigger: 'axis' },
    grid: { left: 8, right: 16, top: 12, bottom: 24, containLabel: true },
    xAxis: { type: 'category', data: statuses.map(([key]) => key.replaceAll('_', ' ')), axisLabel: { color: chartColors.axis }, axisLine: { lineStyle: { color: chartColors.grid } } },
    yAxis: { type: 'value', minInterval: 1, splitLine: { lineStyle: { color: chartColors.grid } }, axisLabel: { color: chartColors.axis } },
    series: [{ type: 'bar', barWidth: 24, data: statuses.map(([, count]) => count), itemStyle: { borderRadius: [6, 6, 0, 0] } }],
  }
  return <>
    <PageHeader eyebrow="LIVE OPERATIONS" title="Good morning, operator." description="A calm view of document intelligence, processing health, and governance signals." action={<Badge variant="green" label="Live" />} />
    <VStack gap={4}>
      <Grid columns={{ minWidth: 150, max: 4 }} gap={3}>
        {metrics.map((metric) => (
          <Card key={metric.label} padding={5}>
            <VStack gap={2}>
              <Text type="supporting">{metric.label}</Text>
              <Text type="display-2" hasTabularNumbers>{metric.value}</Text>
              <Text type="supporting">{metric.detail}</Text>
            </VStack>
          </Card>
        ))}
      </Grid>
      <Grid columns={{ minWidth: 380, max: 2 }} gap={3}>
        <Card padding={5}>
          <VStack gap={4}>
            <HStack gap={4} hAlign="between" vAlign="start">
              <VStack gap={1}>
                <Heading level={2}>Processing flow</Heading>
                <Text type="supporting">Current durable job states</Text>
              </VStack>
              <Link className="text-button" to="/processing">View jobs →</Link>
            </HStack>
            {statuses.length
              ? <ReactECharts option={chartOption} style={{ height: 260 }} />
              : <Empty>No processing jobs in this scope yet.</Empty>}
          </VStack>
        </Card>
        <Card padding={5}>
          <VStack gap={4}>
            <HStack gap={4} hAlign="between" vAlign="start">
              <VStack gap={1}>
                <Heading level={2}>Recent activity</Heading>
                <Text type="supporting">Tenant-scoped audit trail</Text>
              </VStack>
              <Link className="text-button" to="/audit">Open audit →</Link>
            </HStack>
            <AuditList events={overview.recent_audit_events} />
          </VStack>
        </Card>
      </Grid>
      <Card padding={5}>
        <VStack gap={4}>
          <HStack gap={4} hAlign="between" vAlign="start">
            <VStack gap={1}>
              <Heading level={2}>Recently moving</Heading>
              <Text type="supporting">Jobs that changed state most recently</Text>
            </VStack>
            <Link className="text-button" to="/processing">All processing →</Link>
          </HStack>
          <JobsTable jobs={overview.recent_jobs} />
        </VStack>
      </Card>
    </VStack>
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
    <VStack gap={4}>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Bring a document into the pipeline</Heading>
            <Text type="supporting">PDF, TXT, DOC, and DOCX up to 10 MiB. Storage keys are generated server-side.</Text>
          </VStack>
          <HStack gap={4} vAlign="end" wrap="wrap">
            <FileInput
              label="Document file"
              value={selectedFile}
              onChange={(files) => setSelectedFile(files instanceof File ? files : null)}
              accept=".pdf,.txt,.doc,.docx,application/pdf,text/plain,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
              maxSize={10 * 1024 * 1024}
              placeholder="Choose a file"
              description="Accepted formats: PDF, TXT, DOC, DOCX up to 10 MiB."
            />
            <Button
              label={upload.isPending ? 'Uploading…' : 'Upload document'}
              variant="primary"
              isDisabled={!selectedFile || upload.isPending}
              isLoading={upload.isPending}
              onClick={() => selectedFile && upload.mutate(selectedFile)}
            />
          </HStack>
          {upload.isError && (
            <Banner
              status="error"
              title="Upload failed"
              description={upload.error instanceof ApiError ? upload.error.message : 'Upload failed. Check the API connection.'}
            />
          )}
        </VStack>
      </Card>
      <Card padding={5}>
        <VStack gap={4}>
          <HStack gap={4} hAlign="between" vAlign="start" wrap="wrap">
            <VStack gap={1}>
              <Heading level={2}>Document register</Heading>
              <Text type="supporting">{query.data.items.length} records in this page</Text>
            </VStack>
            <Text type="supporting">All records are tenant-scoped</Text>
          </HStack>
          <DocumentTable documents={query.data.items} />
        </VStack>
      </Card>
    </VStack>
  </>
}

type DocumentRow = Document & Record<string, unknown>

function DocumentTable({ documents }: { documents: Document[] }) {
  return (
    <Table<DocumentRow>
      data={documents as DocumentRow[]}
      idKey="id"
      density="compact"
      emptyState={<EmptyState isCompact title="No documents yet" description="Upload the first document to make the pipeline visible." />}
      columns={[
        {
          key: 'original_filename',
          header: 'Document',
          width: proportional(2),
          renderCell: (row) => (
            <VStack gap={0.5}>
              <AstryxLink href={`/documents/${row.id}`}>{row.original_filename}</AstryxLink>
              <Text type="supporting" maxLines={1}>{row.content_type}</Text>
            </VStack>
          ),
        },
        { key: 'status', header: 'Status', renderCell: (row) => <StatusPill value={row.status} /> },
        { key: 'content_revision', header: 'Revision' },
        { key: 'size_bytes', header: 'Size', renderCell: (row) => <Text as="span" hasTabularNumbers>{bytes(row.size_bytes)}</Text> },
        { key: 'updated_at', header: 'Updated', renderCell: (row) => <Text as="span" type="supporting">{date(row.updated_at)}</Text> },
      ]}
    />
  )
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
  return <>
    <PageHeader
      eyebrow="DOCUMENT DETAIL"
      title={document.original_filename}
      description={`${document.content_type} · revision ${document.content_revision}`}
      action={(
        <Button
          label={start.isPending ? 'Starting…' : 'Start processing'}
          variant="primary"
          isLoading={start.isPending}
          isDisabled={start.isPending}
          onClick={() => start.mutate()}
        />
      )}
    />
    <Grid columns={{ minWidth: 360, max: 2 }} gap={4}>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Text type="label" color="secondary">DOCUMENT</Text>
            <Heading level={2}>{document.original_filename}</Heading>
            <HStack gap={2}><StatusPill value={document.status} /></HStack>
          </VStack>
          <MetadataList>
            <MetadataListItem label="Document ID"><Text type="code" as="span">{document.id}</Text></MetadataListItem>
            <MetadataListItem label="Size">{bytes(document.size_bytes)}</MetadataListItem>
            <MetadataListItem label="Created">{date(document.created_at)}</MetadataListItem>
            <MetadataListItem label="Updated">{date(document.updated_at)}</MetadataListItem>
          </MetadataList>
        </VStack>
      </Card>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Processing history</Heading>
            <Text type="supporting">Each revision creates its own durable execution trail.</Text>
          </VStack>
          {jobsQuery.data?.length
            ? <JobsTable jobs={jobsQuery.data} />
            : <Empty>No processing job has been started for this document.</Empty>}
        </VStack>
      </Card>
    </Grid>
    {start.isError && <Banner status="error" title="Couldn’t start processing" description={start.error.message} />}
  </>
}

export function ProcessingPage() {
  const { jobId } = useParams()
  const query = useQuery<ProcessingJob | { items: ProcessingJob[] }>({ queryKey: ['jobs', jobId ?? 'all'], queryFn: () => jobId ? getJob(jobId) : listJobs(50) })
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  if (jobId) return <JobDetail job={query.data as ProcessingJob} />
  const page = query.data as { items: ProcessingJob[] }
  return <>
    <PageHeader eyebrow="DURABLE EXECUTION" title="Processing" description="Leases, retries, checkpoints, and review boundaries stay in the processing context." />
    <Card padding={5}>
      <VStack gap={4}>
        <VStack gap={1}>
          <Heading level={2}>Job queue</Heading>
          <Text type="supporting">{page.items.length} recent jobs</Text>
        </VStack>
        <JobsTable jobs={page.items} />
      </VStack>
    </Card>
  </>
}

const PIPELINE_STEPS = ['validate_source', 'detect_type', 'extract_text', 'extract_fields', 'validate_candidate', 'await_review'] as const

function JobDetail({ job }: { job: ProcessingJob }) {
  return <>
    <PageHeader
      eyebrow="PROCESSING JOB"
      title={job.current_step.replaceAll('_', ' ')}
      description={<Text type="code" as="span">{job.job_id}</Text>}
      action={<StatusPill value={job.status} />}
    />
    <Grid columns={{ minWidth: 360, max: 2 }} gap={4}>
      <Card padding={5}>
        <VStack gap={4}>
          <Text type="label" color="secondary">EXECUTION STATE</Text>
          <MetadataList>
            <MetadataListItem label="Document"><IdLink id={job.document_id} to={`/documents/${job.document_id}`} /></MetadataListItem>
            <MetadataListItem label="Content revision">{job.content_revision}</MetadataListItem>
            <MetadataListItem label="Current step">{job.current_step.replaceAll('_', ' ')}</MetadataListItem>
            <MetadataListItem label="Attempts">{job.attempt_count}</MetadataListItem>
            <MetadataListItem label="Updated">{date(job.updated_at)}</MetadataListItem>
          </MetadataList>
        </VStack>
      </Card>
      <Card padding={5}>
        <VStack gap={4}>
          <Text type="label" color="secondary">FIXED PIPELINE</Text>
          <VStack gap={2}>
            {PIPELINE_STEPS.map((step, index) => {
              const state = step === job.current_step ? 'current' : index < 2 ? 'done' : 'pending'
              return (
                <HStack key={step} gap={2} vAlign="center" padding={2} hAlign="between">
                  <HStack gap={2} vAlign="center">
                    <StatusDot
                      variant={state === 'current' ? 'accent' : state === 'done' ? 'success' : 'neutral'}
                      label={`${step.replaceAll('_', ' ')} ${state}`}
                    />
                    <Text type={state === 'pending' ? 'supporting' : 'body'} weight={state === 'current' ? 'semibold' : 'normal'}>
                      {index + 1}. {step.replaceAll('_', ' ')}
                    </Text>
                  </HStack>
                  {state === 'current' && <Badge variant="blue" label="current" />}
                  {state === 'done' && <Badge variant="green" label="done" />}
                </HStack>
              )
            })}
          </VStack>
          {job.candidate_available && (
            <Button
              label="Review candidate"
              variant="primary"
              width="100%"
              href={`/processing/${job.job_id}/candidate`}
              endContent={<Text as="span">→</Text>}
            />
          )}
        </VStack>
      </Card>
    </Grid>
  </>
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
  const isLocked = mutation.isPending || Boolean(submitted)
  return <>
    <PageHeader
      eyebrow="HUMAN REVIEW"
      title="Candidate review"
      description="Review a bounded candidate with evidence before it becomes a business decision."
      action={<StatusPill value={submitted ?? 'waiting_for_review'} />}
    />
    <Grid columns={{ minWidth: 360, max: 2 }} gap={4}>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Extracted fields</Heading>
            <Text type="supporting">Schema {candidate.schema_version} · candidate v{candidate.version}</Text>
          </VStack>
          <CodeBlock
            code={JSON.stringify(candidate.payload, null, 2)}
            language="json"
            title="candidate payload"
            hasLineNumbers
            isCollapsible
            maxHeight={420}
            width="100%"
          />
        </VStack>
      </Card>
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Decision</Heading>
            <Text type="supporting">Provider metadata is bounded and read-only.</Text>
          </VStack>
          <MetadataList>
            <MetadataListItem label="Provider">{candidate.provider}</MetadataListItem>
            <MetadataListItem label="Model">{candidate.model}</MetadataListItem>
            <MetadataListItem label="Evidence">{candidate.evidence.length} references</MetadataListItem>
          </MetadataList>
          <TextArea
            label="Comment"
            value={comment}
            onChange={setComment}
            placeholder="Optional reviewer context"
            rows={4}
            isDisabled={isLocked}
          />
          <HStack gap={2} hAlign="end">
            <Button label="Reject" variant="secondary" isDisabled={isLocked} onClick={() => mutation.mutate('rejected')} />
            <Button label="Accept candidate" variant="primary" isDisabled={isLocked} isLoading={mutation.isPending} onClick={() => mutation.mutate('accepted')} />
          </HStack>
          {mutation.isError && <Banner status="error" title="Decision not recorded" description={mutation.error.message} />}
          {submitted && <Banner status="success" title="Decision recorded" description="The application service handled versioning, idempotency, and audit." />}
        </VStack>
      </Card>
    </Grid>
  </>
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

type JobRow = ProcessingJob & Record<string, unknown>

function JobsTable({ jobs }: { jobs: ProcessingJob[] }) {
  return (
    <Table<JobRow>
      data={jobs as JobRow[]}
      idKey="job_id"
      density="compact"
      emptyState={<EmptyState isCompact title="No processing jobs in this scope." />}
      columns={[
        { key: 'job_id', header: 'Job', renderCell: (row) => <IdLink id={row.job_id} to={`/processing/${row.job_id}`} /> },
        { key: 'document_id', header: 'Document', renderCell: (row) => <IdLink id={row.document_id} to={`/documents/${row.document_id}`} /> },
        { key: 'current_step', header: 'Step', renderCell: (row) => row.current_step.replaceAll('_', ' ') },
        { key: 'status', header: 'Status', renderCell: (row) => <StatusPill value={row.status} /> },
        { key: 'attempt_count', header: 'Attempts', renderCell: (row) => <Text as="span" hasTabularNumbers>{row.attempt_count}</Text> },
        { key: 'updated_at', header: 'Updated', renderCell: (row) => <Text as="span" type="supporting">{date(row.updated_at)}</Text> },
      ]}
    />
  )
}

function AuditList({ events, expanded = false }: { events: AuditEvent[]; expanded?: boolean }) {
  if (!events.length) return <Empty>No audit events yet.</Empty>
  return <div className="audit-list">{events.map((event) => <div className="audit-item" key={event.id}><div className="audit-marker" /><div className="audit-main"><div><strong>{event.action.replaceAll('_', ' ')}</strong><StatusPill value={event.result} /></div><span className="table-secondary">{event.resource_type} · {event.resource_id.slice(0, 12)} · {date(event.occurred_at)}</span>{expanded && event.failure_code && <span className="table-secondary">Failure: {event.failure_code}</span>}</div></div>)}</div>
}
