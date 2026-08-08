use document_processing::ports::ProcessingJobDetail;
use public_api_contracts as contracts;

pub fn processing_job(detail: &ProcessingJobDetail) -> contracts::ProcessingJob {
    let job = &detail.job;
    contracts::ProcessingJob {
        job_id: job.id(),
        document_id: job.document_id(),
        content_revision: job.document_content_revision(),
        revision_id: job.document_revision_id(),
        status: job.status().as_str().to_string(),
        current_step: job.current_step().as_str().to_string(),
        attempt_count: job.attempt_count(),
        failure_code: job
            .failure_code()
            .map(document_processing_contracts::safe_failure_code)
            .map(ToOwned::to_owned),
        cancel_requested: job.cancel_requested_at().is_some(),
        candidate_available: detail.candidate.is_some(),
        review_available: detail.review.is_some(),
        created_at: job.created_at(),
        updated_at: job.updated_at(),
    }
}

pub fn document(document: document::query::DocumentDetailView) -> contracts::Document {
    contracts::Document {
        id: document.id,
        original_filename: document.original_filename,
        content_type: document.content_type,
        status: document.status.as_str().to_string(),
        version: document.version,
        content_revision: document.content_revision,
        revision_id: Some(document.revision_id),
        revision_no: Some(document.revision_no),
        is_current: Some(document.is_current),
        size_bytes: document.size_bytes,
        created_at: document.created_at,
        updated_at: document.updated_at,
    }
}

pub fn document_list_item(document: document::query::DocumentListItem) -> contracts::Document {
    contracts::Document {
        id: document.id,
        original_filename: document.original_filename,
        content_type: document.content_type,
        status: document.status.as_str().to_string(),
        version: document.version,
        content_revision: document.content_revision,
        revision_id: Some(document.revision_id),
        revision_no: Some(document.revision_no),
        is_current: Some(document.is_current),
        size_bytes: document.size_bytes,
        created_at: document.created_at,
        updated_at: document.updated_at,
    }
}

pub fn audit(event: &audit::AuditEvent) -> contracts::AuditEvent {
    contracts::AuditEvent {
        id: event.id(),
        action: event.action().as_str().to_string(),
        resource_type: event.resource().resource_type.clone(),
        resource_id: event.resource().resource_id.clone(),
        result: serde_json::to_value(event.result())
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| format!("{:?}", event.result()).to_lowercase()),
        failure_code: event.failure_code().map(ToOwned::to_owned),
        trace_id: event.trace_id().map(ToOwned::to_owned),
        occurred_at: event.occurred_at(),
        stream_sequence: event.stream_sequence(),
        schema_version: event.schema_version().to_string(),
        details: Some(audit::sanitize_details_for_read(event.details().clone())),
    }
}

pub fn finding(finding: &data_integrity::IntegrityFinding) -> contracts::IntegrityFinding {
    contracts::IntegrityFinding {
        id: finding.id(),
        rule_id: finding.rule_id().to_string(),
        bounded_context: finding.bounded_context().to_string(),
        resource_type: finding.resource_type().to_string(),
        resource_id: finding.resource_id().to_string(),
        severity: serde_json::to_value(finding.severity())
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| format!("{:?}", finding.severity()).to_lowercase()),
        status: data_integrity::finding_status_name(finding.status()).to_string(),
        repairability: finding.repairability().to_string(),
        first_detected_at: finding.first_detected_at(),
        last_detected_at: finding.last_detected_at(),
        occurrence_count: i64::try_from(finding.occurrence_count()).unwrap_or(i64::MAX),
        version: finding.version(),
    }
}

pub fn candidate(candidate: document_processing::ExtractionCandidate) -> contracts::Candidate {
    let version = candidate.version();
    let created_at = candidate.created_at();
    contracts::Candidate {
        candidate_id: candidate.id(),
        job_id: candidate.job_id(),
        content_revision: candidate.content_revision(),
        schema_version: candidate.schema_version,
        payload: serde_json::to_value(candidate.payload).unwrap_or_else(|_| serde_json::json!({})),
        evidence: candidate
            .evidence
            .into_iter()
            .filter_map(|evidence| serde_json::to_value(evidence).ok())
            .collect(),
        provider: candidate.provider,
        model: candidate.model,
        prompt_version: candidate.prompt_version,
        version,
        created_at,
    }
}

pub fn review(review: document_processing::CandidateReview) -> contracts::Review {
    contracts::Review {
        id: review.id,
        candidate_id: review.candidate_id,
        decision: review.decision.as_str().to_string(),
        patch: review.patch,
        comment: review.comment,
        candidate_version: review.candidate_version,
        created_at: review.created_at,
    }
}
