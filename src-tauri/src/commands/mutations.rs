//! 接纳参数转换；selection/批次版本仍由TaskControl在自身锁内复验，不使用路径DTO。

use crate::ipc::{
    self, ConfirmContentCredentials, CredentialsConsent, CredentialsOutput, DecimalU64, ImportTask,
    MAX_RETRY_JOBS, MutationError, RetryTask, SelectTask, StartTask, TaskMutation,
    TaskMutationAccepted, TaskMutationRequest, TaskOutput, TaskSettingsDto,
};
use crate::{
    ingress::NativeImports,
    tasks::{SelectionId, TaskControl, TaskError, TaskPhase, TaskSettings},
};
use pixofold_core::{
    batch::{BatchParameters, RetryJob, RetryRequest},
    import::{ImportOutput, copy_beside},
    model::{OutputPolicy, PngMetadataPolicy},
};
use std::collections::BTreeSet;

fn failure(error: TaskError) -> MutationError {
    match ipc::task_error(&error) {
        Ok(error) => MutationError::Task { error },
        Err(_) => MutationError::ServiceFault,
    }
}
fn settings(value: TaskSettingsDto) -> TaskSettings {
    TaskSettings {
        parameters: BatchParameters {
            mode: value.mode,
            ..Default::default()
        },
        output: match value.output {
            TaskOutput::Overwrite => ImportOutput::Overwrite,
            TaskOutput::CopyBeside => ImportOutput::CopyBeside,
        },
    }
}
fn selection(control: &TaskControl, id: DecimalU64) -> Result<SelectionId, MutationError> {
    control
        .snapshot()
        .selection
        .filter(|selection| selection.get() == id.0)
        .ok_or_else(|| failure(TaskError::StaleSelection))
}
pub(crate) fn ensure_can_import(control: &TaskControl) -> Result<(), MutationError> {
    match control.snapshot().phase {
        TaskPhase::Idle | TaskPhase::Finished | TaskPhase::Cancelled | TaskPhase::Rejected => {
            Ok(())
        }
        TaskPhase::Closing | TaskPhase::Closed => Err(failure(TaskError::Closed)),
        _ => Err(failure(TaskError::Busy)),
    }
}

/// 调用方必须在SubscriptionControl::with_ready内执行，以免旧会话在重载后提交写操作。
pub(crate) fn mutate(
    control: &TaskControl,
    imports: &NativeImports,
    request: TaskMutationRequest,
) -> Result<TaskMutationAccepted, MutationError> {
    let id = match request.operation {
        TaskMutation::Import(ImportTask {
            grant_id,
            settings: value,
        }) => imports
            .consume(request.subscription_id, grant_id, |roots| {
                control.import(roots, value.map(settings)).map_err(failure)
            })?
            .get(),
        TaskMutation::Start(StartTask {
            selection_id,
            settings: value,
        }) => {
            control
                .start(selection(control, selection_id)?, settings(value))
                .map_err(failure)?;
            selection_id.0
        }
        TaskMutation::Clear(SelectTask { selection_id }) => {
            // 清除只删除任务记录，不删除原图、结果或备份；恢复信息须由UI在清除前展示。
            control
                .clear(selection(control, selection_id)?)
                .map_err(failure)?;
            selection_id.0
        }
        TaskMutation::Retry(RetryTask {
            selection_id,
            expected_batch_revision,
            job_ids,
            mode,
        }) => {
            if job_ids.is_empty() || job_ids.len() > MAX_RETRY_JOBS {
                return Err(MutationError::InvalidRetry);
            }
            let snapshot = control.snapshot();
            let id = snapshot
                .selection
                .filter(|id| id.get() == selection_id.0)
                .ok_or_else(|| failure(TaskError::StaleSelection))?;
            let batch = snapshot
                .batch
                .as_ref()
                .ok_or_else(|| failure(TaskError::NotReady))?;
            if batch.revision != expected_batch_revision.0 {
                return Err(failure(TaskError::StaleBatch));
            }
            let selected: BTreeSet<_> = job_ids.iter().map(|id| *id as usize).collect();
            if selected.len() != job_ids.len() {
                return Err(MutationError::InvalidRetry);
            }
            let mut jobs = Vec::with_capacity(job_ids.len());
            // ID不是数组索引；单次扫描固定快照，也避免逐ID遍历造成平方复杂度。
            for job in batch
                .jobs
                .iter()
                .filter(|job| selected.contains(&job.id.get()))
            {
                if !job.state.can_retry() {
                    return Err(MutationError::InvalidRetry);
                }
                jobs.push(RetryJob {
                    id: job.id,
                    // 普通重试不继承上一次显式放弃备份的选择。
                    output: match &job.request.output {
                        OutputPolicy::OverwriteWithoutBackup => OutputPolicy::Overwrite,
                        output => output.clone(),
                    },
                    metadata: Default::default(),
                });
            }
            if jobs.len() != selected.len() {
                return Err(MutationError::InvalidRetry);
            }
            control
                .retry(
                    id,
                    expected_batch_revision.0,
                    RetryRequest {
                        jobs,
                        parameters: BatchParameters {
                            mode,
                            limits: batch.parameters.limits,
                        },
                    },
                )
                .map_err(failure)?;
            selection_id.0
        }
        TaskMutation::ConfirmContentCredentials(ConfirmContentCredentials {
            selection_id,
            expected_batch_revision,
            job_ids,
            mode,
            output,
            consent: CredentialsConsent::RemoveContentCredentials,
        }) => {
            if job_ids.is_empty() || job_ids.len() > MAX_RETRY_JOBS {
                return Err(MutationError::InvalidRetry);
            }
            let snapshot = control.snapshot();
            let id = snapshot
                .selection
                .filter(|id| id.get() == selection_id.0)
                .ok_or_else(|| failure(TaskError::StaleSelection))?;
            if snapshot.phase != TaskPhase::Finished {
                return Err(failure(TaskError::NotReady));
            }
            let batch = snapshot
                .batch
                .as_ref()
                .ok_or_else(|| failure(TaskError::NotReady))?;
            if batch.revision != expected_batch_revision.0 {
                return Err(failure(TaskError::StaleBatch));
            }
            let selected: BTreeSet<_> = job_ids.iter().map(|id| *id as usize).collect();
            if selected.len() != job_ids.len() {
                return Err(MutationError::InvalidRetry);
            }
            let mut jobs = Vec::with_capacity(selected.len());
            for job in batch
                .jobs
                .iter()
                .filter(|job| selected.contains(&job.id.get()))
            {
                let source = job
                    .content_credentials_source()
                    .ok_or(MutationError::InvalidRetry)?;
                jobs.push(RetryJob {
                    id: job.id,
                    output: match output {
                        CredentialsOutput::OverwriteWithBackup => OutputPolicy::Overwrite,
                        CredentialsOutput::OverwriteWithoutBackup => {
                            OutputPolicy::OverwriteWithoutBackup
                        }
                        CredentialsOutput::CopyBeside => copy_beside(&job.request.source)
                            .map_err(|_| MutationError::InvalidRetry)?,
                    },
                    metadata: PngMetadataPolicy::RemoveContentCredentials(source.clone()),
                });
            }
            if jobs.len() != selected.len() {
                return Err(MutationError::InvalidRetry);
            }
            control
                .retry(
                    id,
                    expected_batch_revision.0,
                    RetryRequest {
                        jobs,
                        parameters: BatchParameters {
                            mode,
                            limits: batch.parameters.limits,
                        },
                    },
                )
                .map_err(failure)?;
            selection_id.0
        }
    };
    Ok(TaskMutationAccepted {
        selection_id: DecimalU64(id),
    })
}
