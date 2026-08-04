//! Allow-listed, typed processing repairs.
//!
//! The governance context only coordinates a repair.  The processing context
//! owns the implementation of [`ProcessingRepairPort`], so no handler can
//! accept arbitrary SQL, table names, or JSON patches.

use async_trait::async_trait;
use chrono::Utc;
use data_repair::{
    RepairCommand, RepairDescriptor, RepairError, RepairExecutionContext, RepairHandler,
    RepairHandlerRegistry, RepairPreview, RepairResult, RepairRiskLevel, RepairVerification,
};
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait ProcessingRepairPort: Send + Sync {
    async fn preview_reconcile_processing_job(
        &self,
        _command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn preview_requeue_missing_ai_task(
        &self,
        _command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn preview_clear_terminal_job_lease(
        &self,
        _command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn preview_rebuild_processing_step_projection(
        &self,
        _command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn preview_reconcile_ai_completion(
        &self,
        _command: &RepairCommand,
    ) -> Result<RepairPreview, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn verify_repair(
        &self,
        _command: &RepairCommand,
        result: &RepairResult,
    ) -> Result<RepairVerification, RepairError>;

    async fn reconcile_processing_job(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError>;

    async fn requeue_missing_ai_task(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError>;

    async fn clear_terminal_job_lease(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError>;

    async fn rebuild_processing_step_projection(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError>;

    async fn reconcile_ai_completion(
        &self,
        command: &RepairCommand,
        context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError>;
}

/// Fail-closed default used when the composition root has no processing
/// repair adapter.  Dry-run remains available; execution cannot mutate data.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableProcessingRepairPort;

#[async_trait]
impl ProcessingRepairPort for UnavailableProcessingRepairPort {
    async fn verify_repair(
        &self,
        _command: &RepairCommand,
        _result: &RepairResult,
    ) -> Result<RepairVerification, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn reconcile_processing_job(
        &self,
        _command: &RepairCommand,
        _context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn requeue_missing_ai_task(
        &self,
        _command: &RepairCommand,
        _context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn clear_terminal_job_lease(
        &self,
        _command: &RepairCommand,
        _context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn rebuild_processing_step_projection(
        &self,
        _command: &RepairCommand,
        _context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        Err(RepairError::Unavailable)
    }

    async fn reconcile_ai_completion(
        &self,
        _command: &RepairCommand,
        _context: &RepairExecutionContext,
    ) -> Result<RepairResult, RepairError> {
        Err(RepairError::Unavailable)
    }
}

fn verify_context(context: &RepairExecutionContext) -> Result<(), RepairError> {
    if context.run_id.is_nil()
        || context.step_id.is_nil()
        || context.worker_id.trim().is_empty()
        || context.lease_token.trim().is_empty()
        || context.fence_version < 0
        || context.lease_expires_at <= Utc::now()
    {
        return Err(RepairError::LeaseLost);
    }
    Ok(())
}

macro_rules! typed_handler {
    ($name:ident, $method:ident, $preview_method:ident, $repair_type:literal, $risk:expr, $approval:expr, $automatic:expr) => {
        pub struct $name<P> {
            port: Arc<P>,
        }

        impl<P> $name<P> {
            fn new(port: Arc<P>) -> Self {
                Self { port }
            }
        }

        #[async_trait]
        impl<P: ProcessingRepairPort + 'static> RepairHandler for $name<P> {
            fn descriptor(&self) -> RepairDescriptor {
                RepairDescriptor {
                    repair_type: $repair_type.to_string(),
                    version: 1,
                    bounded_context: "document-processing".to_string(),
                    risk_level: $risk,
                    requires_approval: $approval,
                    supports_automatic_execution: $automatic,
                }
            }

            async fn dry_run(&self, command: &RepairCommand) -> Result<RepairPreview, RepairError> {
                command.validate()?;
                let descriptor = self.descriptor();
                descriptor.validate()?;
                let mut preview = self.port.$preview_method(command).await?;
                preview.command_id = Uuid::now_v7();
                preview.descriptor = descriptor;
                preview.finding_id = command.integrity_finding_id;
                preview.resource_type = command.target.resource_type.clone();
                preview.resource_id = command.target.resource_id.clone();
                Ok(preview)
            }

            async fn execute(
                &self,
                command: &RepairCommand,
                context: &RepairExecutionContext,
            ) -> Result<RepairResult, RepairError> {
                command.validate()?;
                verify_context(context)?;
                self.port.$method(command, context).await
            }

            async fn verify(
                &self,
                result: &RepairResult,
            ) -> Result<RepairVerification, RepairError> {
                Ok(RepairVerification {
                    valid: matches!(
                        result.outcome,
                        data_repair::RepairOutcome::Succeeded | data_repair::RepairOutcome::Noop
                    ),
                    message: "typed processing repair returned a fenced result".to_string(),
                })
            }

            async fn verify_after_repair(
                &self,
                command: &RepairCommand,
                result: &RepairResult,
            ) -> Result<RepairVerification, RepairError> {
                self.port.verify_repair(command, result).await
            }
        }
    };
}

typed_handler!(
    ReconcileProcessingJobHandler,
    reconcile_processing_job,
    preview_reconcile_processing_job,
    "reconcile_processing_job.v1",
    RepairRiskLevel::Medium,
    true,
    false
);
typed_handler!(
    RequeueMissingAiTaskHandler,
    requeue_missing_ai_task,
    preview_requeue_missing_ai_task,
    "requeue_missing_ai_task.v1",
    RepairRiskLevel::Low,
    false,
    true
);
typed_handler!(
    ClearTerminalJobLeaseHandler,
    clear_terminal_job_lease,
    preview_clear_terminal_job_lease,
    "clear_terminal_job_lease.v1",
    RepairRiskLevel::Low,
    false,
    true
);
typed_handler!(
    RebuildProcessingStepProjectionHandler,
    rebuild_processing_step_projection,
    preview_rebuild_processing_step_projection,
    "rebuild_processing_step_projection.v1",
    RepairRiskLevel::Medium,
    true,
    false
);
typed_handler!(
    ReconcileAiCompletionHandler,
    reconcile_ai_completion,
    preview_reconcile_ai_completion,
    "reconcile_ai_completion.v1",
    RepairRiskLevel::Medium,
    true,
    false
);

/// Registry containing only the five deterministic processing repairs.
pub struct ProcessingRepairRegistry<P> {
    port: Arc<P>,
}

impl<P> ProcessingRepairRegistry<P> {
    #[must_use]
    pub fn new(port: Arc<P>) -> Self {
        Self { port }
    }
}

#[async_trait]
impl<P: ProcessingRepairPort + 'static> RepairHandlerRegistry for ProcessingRepairRegistry<P> {
    async fn get(&self, repair_type: &str, version: u32) -> Option<Box<dyn RepairHandler>> {
        if version != 1 {
            return None;
        }
        let handler: Box<dyn RepairHandler> = match repair_type {
            "reconcile_processing_job.v1" => {
                Box::new(ReconcileProcessingJobHandler::new(Arc::clone(&self.port)))
            }
            "requeue_missing_ai_task.v1" => {
                Box::new(RequeueMissingAiTaskHandler::new(Arc::clone(&self.port)))
            }
            "clear_terminal_job_lease.v1" => {
                Box::new(ClearTerminalJobLeaseHandler::new(Arc::clone(&self.port)))
            }
            "rebuild_processing_step_projection.v1" => Box::new(
                RebuildProcessingStepProjectionHandler::new(Arc::clone(&self.port)),
            ),
            "reconcile_ai_completion.v1" => {
                Box::new(ReconcileAiCompletionHandler::new(Arc::clone(&self.port)))
            }
            _ => return None,
        };
        Some(handler)
    }
}
