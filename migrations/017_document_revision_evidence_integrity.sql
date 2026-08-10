-- PLAN-0008 R1 integrity fences. PostgreSQL is the multi-worker authority.

ALTER TABLE document_processing_runs
    ADD CONSTRAINT document_processing_runs_revision_identity_unique
        UNIQUE (tenant_id, document_revision_id, id);

ALTER TABLE document_processing_artifacts
    ADD CONSTRAINT document_processing_artifacts_run_identity_unique
        UNIQUE (tenant_id, processing_run_id, id),
    ADD CONSTRAINT document_processing_artifacts_kind_unique
        UNIQUE (tenant_id, processing_run_id, kind);

ALTER TABLE document_processing_evidence
    ADD CONSTRAINT document_processing_evidence_run_revision_fk
        FOREIGN KEY (tenant_id, document_revision_id, processing_run_id)
        REFERENCES document_processing_runs
            (tenant_id, document_revision_id, id),
    ADD CONSTRAINT document_processing_evidence_run_artifact_fk
        FOREIGN KEY (tenant_id, processing_run_id, artifact_id)
        REFERENCES document_processing_artifacts
            (tenant_id, processing_run_id, id),
    ADD CONSTRAINT document_processing_evidence_identity_unique
        UNIQUE (
            tenant_id,
            document_revision_id,
            processing_run_id,
            artifact_id,
            location_json,
            source_checksum
        );

CREATE OR REPLACE FUNCTION reject_document_revision_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'document revisions are immutable' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER document_revisions_immutable
BEFORE UPDATE OR DELETE ON document_revisions
FOR EACH ROW EXECUTE FUNCTION reject_document_revision_mutation();
