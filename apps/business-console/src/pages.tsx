import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useParams } from 'react-router-dom'
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
import { List, ListItem } from '@astryxdesign/core/List'
import { MetadataList, MetadataListItem } from '@astryxdesign/core/MetadataList'
import { Link as AstryxLink } from '@astryxdesign/core/Link'
import { StatusDot } from '@astryxdesign/core/StatusDot'
import { Table, proportional } from '@astryxdesign/core/Table'
import { Text } from '@astryxdesign/core/Text'
import { TextArea } from '@astryxdesign/core/TextArea'
import { VStack } from '@astryxdesign/core/VStack'
import { ApiError, getCandidate, getDocument, getJob, listAudit, listDocumentJobs, listDocuments, listFindings, listJobs, startProcessing, submitReview, uploadDocument } from './api'
import type { AuditEvent, Candidate, Document, IntegrityFinding, ProcessingJob } from './contracts'
import { Empty, ErrorState, IdLink, Loading, PageHeader, StatusPill } from './components'

const date = (value: string) => new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
const bytes = (value?: number | null) => value == null ? '—' : `${(value / 1024).toFixed(1)} KB`

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
  return <>
    <PageHeader
      eyebrow="DATA GOVERNANCE"
      title="Integrity findings"
      description="Findings are evidence of a consistency problem, not an instruction to bypass the owning context."
      action={<Button label="Repair controls" variant="secondary" href="/repairs" endContent={<Text as="span">→</Text>} />}
    />
    <Card padding={5}>
      <VStack gap={4}>
        <VStack gap={1}>
          <Heading level={2}>Open signals</Heading>
          <Text type="supporting">{query.data.items.length} bounded findings</Text>
        </VStack>
        <FindingsTable findings={query.data.items} />
      </VStack>
    </Card>
  </>
}

type FindingRow = IntegrityFinding & Record<string, unknown>

function FindingsTable({ findings }: { findings: IntegrityFinding[] }) {
  return (
    <Table<FindingRow>
      data={findings as FindingRow[]}
      idKey="id"
      density="compact"
      emptyState={<EmptyState isCompact title="Integrity is quiet in this tenant." />}
      columns={[
        {
          key: 'rule_id',
          header: 'Rule',
          width: proportional(2),
          renderCell: (row) => (
            <VStack gap={0.5}>
              <Text weight="semibold">{row.rule_id}</Text>
              <Text type="supporting">{row.bounded_context}</Text>
            </VStack>
          ),
        },
        { key: 'resource_id', header: 'Resource', renderCell: (row) => <Text type="code" as="span">{`${row.resource_type}:${row.resource_id.slice(0, 12)}`}</Text> },
        { key: 'severity', header: 'Severity', renderCell: (row) => <StatusPill value={row.severity} /> },
        { key: 'status', header: 'Status', renderCell: (row) => <StatusPill value={row.status} /> },
        { key: 'occurrence_count', header: 'Occurrences', renderCell: (row) => <Text as="span" hasTabularNumbers>{row.occurrence_count}</Text> },
        { key: 'last_detected_at', header: 'Last detected', renderCell: (row) => <Text as="span" type="supporting">{date(row.last_detected_at)}</Text> },
      ]}
    />
  )
}

export function RepairsPage() {
  const query = useQuery({ queryKey: ['findings', 'repair'], queryFn: () => listFindings(50) })
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  const candidates = query.data.items.filter((item) => item.status !== 'repaired')
  return <>
    <PageHeader eyebrow="CONTROLLED CHANGE" title="Repairs" description="Prepare → Preview → Confirm → Execute. This console never turns a finding into an unreviewed write." />
    <VStack gap={4}>
      <Banner
        status="warning"
        title="Safe operating boundary"
        description="Repair execution remains permissioned, version-bound, and owned by the Governance application service. Use the API dry-run endpoint for a preview before approval."
      />
      <Card padding={5}>
        <VStack gap={4}>
          <VStack gap={1}>
            <Heading level={2}>Repair candidates</Heading>
            <Text type="supporting">Signals currently eligible for an operator review.</Text>
          </VStack>
          {candidates.length
            ? <FindingsTable findings={candidates} />
            : <Empty>No repair candidates are present.</Empty>}
        </VStack>
      </Card>
    </VStack>
  </>
}

export function AuditPage() {
  const query = useQuery({ queryKey: ['audit'], queryFn: () => listAudit(50) })
  if (query.isPending) return <Loading />
  if (query.isError) return <ErrorState error={query.error} />
  return <>
    <PageHeader eyebrow="RUNTIME GOVERNANCE" title="Audit" description="Immutable, tenant-scoped evidence for business and operational transitions." />
    <Card padding={5}>
      <VStack gap={4}>
        <VStack gap={1}>
          <Heading level={2}>Event stream</Heading>
          <Text type="supporting">{query.data.items.length} recent events</Text>
        </VStack>
        <AuditList events={query.data.items} expanded />
      </VStack>
    </Card>
  </>
}

type JobRow = ProcessingJob & Record<string, unknown>

export function JobsTable({ jobs }: { jobs: ProcessingJob[] }) {
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

export function AuditList({ events, expanded = false }: { events: AuditEvent[]; expanded?: boolean }) {
  if (!events.length) return <Empty>No audit events yet.</Empty>
  return (
    <List density="compact" edge="flush">
      {events.map((event) => (
        <ListItem
          key={event.id}
          startContent={<StatusDot variant={event.result === 'failed' ? 'error' : 'success'} label={event.result} />}
          label={<Text weight="medium">{event.action.replaceAll('_', ' ')}</Text>}
          description={(
            <Text type="supporting">
              {`${event.resource_type} · ${event.resource_id.slice(0, 12)} · ${date(event.occurred_at)}`}
              {expanded && event.failure_code ? ` · Failure: ${event.failure_code}` : ''}
            </Text>
          )}
          endContent={<StatusPill value={event.result} />}
        />
      ))}
    </List>
  )
}
