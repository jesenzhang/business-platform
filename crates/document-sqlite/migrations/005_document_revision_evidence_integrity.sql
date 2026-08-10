-- PLAN-0008 R1 integrity fences. SQLite remains local single-process.

CREATE UNIQUE INDEX document_processing_artifacts_kind_unique
    ON document_processing_artifacts (tenant_id, processing_run_id, kind);

CREATE UNIQUE INDEX document_processing_evidence_identity_unique
    ON document_processing_evidence (
        tenant_id,
        document_revision_id,
        processing_run_id,
        artifact_id,
        location_json,
        source_checksum
    );

CREATE TRIGGER document_revisions_immutable_update
BEFORE UPDATE ON document_revisions
BEGIN
    SELECT RAISE(ABORT, 'document revisions are immutable');
END;

CREATE TRIGGER document_revisions_immutable_delete
BEFORE DELETE ON document_revisions
BEGIN
    SELECT RAISE(ABORT, 'document revisions are immutable');
END;

CREATE TRIGGER document_processing_evidence_revision_binding
BEFORE INSERT ON document_processing_evidence
WHEN NOT EXISTS (
    SELECT 1
    FROM document_processing_runs
    WHERE tenant_id = NEW.tenant_id
      AND id = NEW.processing_run_id
      AND document_revision_id = NEW.document_revision_id
)
BEGIN
    SELECT RAISE(ABORT, 'processing evidence revision binding is invalid');
END;

CREATE TRIGGER document_processing_evidence_artifact_binding
BEFORE INSERT ON document_processing_evidence
WHEN NOT EXISTS (
    SELECT 1
    FROM document_processing_artifacts
    WHERE tenant_id = NEW.tenant_id
      AND id = NEW.artifact_id
      AND processing_run_id = NEW.processing_run_id
)
BEGIN
    SELECT RAISE(ABORT, 'processing evidence artifact binding is invalid');
END;

CREATE TRIGGER document_processing_evidence_revision_binding_update
BEFORE UPDATE ON document_processing_evidence
WHEN NOT EXISTS (
    SELECT 1
    FROM document_processing_runs
    WHERE tenant_id = NEW.tenant_id
      AND id = NEW.processing_run_id
      AND document_revision_id = NEW.document_revision_id
)
BEGIN
    SELECT RAISE(ABORT, 'processing evidence revision binding is invalid');
END;

CREATE TRIGGER document_processing_evidence_artifact_binding_update
BEFORE UPDATE ON document_processing_evidence
WHEN NOT EXISTS (
    SELECT 1
    FROM document_processing_artifacts
    WHERE tenant_id = NEW.tenant_id
      AND id = NEW.artifact_id
      AND processing_run_id = NEW.processing_run_id
)
BEGIN
    SELECT RAISE(ABORT, 'processing evidence artifact binding is invalid');
END;
