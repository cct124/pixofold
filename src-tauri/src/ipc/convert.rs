//! 纯转换与分页，不序列化领域对象，不执行I/O；每个响应只处理选中的最多100行。

use super::dto::*;
use crate::tasks::{TaskError, TaskSnapshot};
use pixofold_core::{batch::*, import::*, model::*};
use std::path::Path;

type Result<T> = std::result::Result<T, QueryError>;

fn count(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| QueryError::InvalidSnapshot)
}
fn bytes(value: Option<ByteCount>) -> Option<DecimalU64> {
    value.map(|n| DecimalU64(n.0))
}
fn name(path: &Path) -> DisplayName {
    // 根目录也不能回退到完整路径；非UTF-8与截断必须显式说明，名称不用于身份比较。
    let Some(part) = path.file_name() else {
        return DisplayName {
            text: String::from("<root>"),
            truncated: false,
            lossy: false,
            sanitized: false,
        };
    };
    let text = part.to_string_lossy();
    let mut chars = text.chars();
    let mut sanitized = false;
    let bounded = chars
        .by_ref()
        .take(MAX_DISPLAY_CHARS)
        .map(|c| {
            if c.is_control() {
                sanitized = true;
                '\u{fffd}'
            } else {
                c
            }
        })
        .collect();
    DisplayName {
        text: bounded,
        truncated: chars.next().is_some(),
        lossy: part.to_str().is_none(),
        sanitized,
    }
}

fn recovery_cause(error: &ProcessingError) -> RecoveryCauseDto {
    let code = match error {
        ProcessingError::InvalidLimits
        | ProcessingError::InvalidPath
        | ProcessingError::InvalidPng(_) => JobErrorDto::InvalidInput,
        ProcessingError::UnsupportedFormat => JobErrorDto::UnsupportedFormat,
        ProcessingError::UnsupportedAnimation => JobErrorDto::UnsupportedAnimation,
        ProcessingError::UnsupportedMetadata([b'c', b'a', b'B', b'X'])
        | ProcessingError::ContentCredentialsRequireConsent(_) => {
            JobErrorDto::UnsupportedContentCredentials
        }
        ProcessingError::UnsupportedMetadata(_) => JobErrorDto::UnsupportedMetadata,
        ProcessingError::ResourceLimit(_) => JobErrorDto::ResourceLimit,
        ProcessingError::Decode(_) => JobErrorDto::Decode,
        ProcessingError::Encode(_) => JobErrorDto::Encode,
        ProcessingError::ValidationFailed(_) => JobErrorDto::Validation,
        ProcessingError::TargetConflict => JobErrorDto::TargetConflict,
        ProcessingError::SourceChanged => JobErrorDto::SourceChanged,
        ProcessingError::Cancelled => return RecoveryCauseDto::Cancelled,
        ProcessingError::Io { .. } => JobErrorDto::Io,
        ProcessingError::CommitFailed { .. } => JobErrorDto::CommitFailed,
        ProcessingError::CleanupFailed { .. } => JobErrorDto::CleanupFailed,
    };
    RecoveryCauseDto::Failed { code }
}
fn failure(value: &JobFailure) -> JobFailureDto {
    let mut recovery = RecoveryDto {
        backup_name: None,
        temporary_name: None,
        original_error: None,
    };
    let mut cause = value.cause.as_deref();
    while let Some(error) = cause {
        match error {
            ProcessingError::CommitFailed { backup, .. } => {
                recovery.backup_name = Some(name(backup));
                break;
            }
            ProcessingError::CleanupFailed {
                temporary,
                original,
                ..
            } => {
                if recovery.temporary_name.is_none() {
                    recovery.temporary_name = Some(name(temporary));
                }
                recovery.original_error = original.as_deref().map(recovery_cause);
                cause = original.as_deref();
            }
            _ => break,
        }
    }
    JobFailureDto {
        code: value.code.into(),
        recovery: (recovery.backup_name.is_some() || recovery.temporary_name.is_some())
            .then_some(recovery),
    }
}
fn batch_error(error: &BatchError) -> Result<TaskFailureDto> {
    Ok(match error {
        BatchError::InvalidConfig => TaskFailureDto::InvalidConfig,
        BatchError::EmptyBatch => TaskFailureDto::EmptyBatch,
        BatchError::TooManyJobs => TaskFailureDto::TooManyJobs,
        BatchError::Busy => TaskFailureDto::Busy,
        BatchError::Closed => TaskFailureDto::Closed,
        BatchError::ServiceFault => TaskFailureDto::ServiceFault,
        BatchError::NoSuchBatch => TaskFailureDto::NoSuchBatch,
        BatchError::InvalidRetry => TaskFailureDto::InvalidRetry,
        BatchError::IdExhausted => TaskFailureDto::IdExhausted,
        BatchError::TimedOut => TaskFailureDto::TimedOut,
        BatchError::Cancelled => TaskFailureDto::Cancelled,
        BatchError::InvalidParameters(_) => TaskFailureDto::InvalidParameters,
        BatchError::WorkerStart(_) => TaskFailureDto::WorkerStart,
        BatchError::PathConflict {
            first,
            second,
            kind,
        } => TaskFailureDto::PathConflict {
            first: count(first.get())?,
            second: count(second.get())?,
            kind: (*kind).into(),
        },
    })
}
pub(crate) fn task_error(error: &TaskError) -> Result<TaskFailureDto> {
    Ok(match error {
        TaskError::Busy => TaskFailureDto::Busy,
        TaskError::Closed => TaskFailureDto::Closed,
        TaskError::StaleSelection => TaskFailureDto::StaleSelection,
        TaskError::StaleBatch => TaskFailureDto::StaleBatch,
        TaskError::NotReady => TaskFailureDto::NotReady,
        TaskError::TooManyRoots => TaskFailureDto::TooManyRoots,
        TaskError::IdExhausted => TaskFailureDto::IdExhausted,
        TaskError::TimedOut => TaskFailureDto::TimedOut,
        TaskError::ServiceFault => TaskFailureDto::ServiceFault,
        TaskError::WorkerPanicked => TaskFailureDto::WorkerPanicked,
        TaskError::WorkerStart(_) => TaskFailureDto::WorkerStart,
        TaskError::Batch(e) => return batch_error(e),
        TaskError::Import(e) => match e.as_ref() {
            ImportError::InvalidOptions => TaskFailureDto::InvalidOptions,
            ImportError::TooManyRoots => TaskFailureDto::TooManyRoots,
            ImportError::IncompleteScan => TaskFailureDto::IncompleteScan,
            ImportError::NoFiles => TaskFailureDto::NoFiles,
            ImportError::RootNameConflict { .. } => TaskFailureDto::RootNameConflict,
            ImportError::Batch(e) => return batch_error(e),
            ImportError::File { index, failure: f } => TaskFailureDto::File {
                index: count(*index)?,
                failure: failure(f),
            },
        },
    })
}

fn scan(value: ScanProgress) -> Result<ScanProgressDto> {
    Ok(ScanProgressDto {
        status: match value.status {
            ScanStatus::Scanning => ScanStatusDto::Scanning,
            ScanStatus::Complete => ScanStatusDto::Complete,
            ScanStatus::Cancelled => ScanStatusDto::Cancelled,
            ScanStatus::Limited(limit) => ScanStatusDto::Limited {
                limit: limit.into(),
            },
        },
        discovered: count(value.discovered)?,
        examined: count(value.examined)?,
        accepted: count(value.accepted)?,
        duplicates: count(value.duplicates)?,
        excluded: count(value.excluded)?,
        rejected: count(value.rejected)?,
        read_bytes: DecimalU64(value.read_bytes.0),
    })
}
fn summary(s: &BatchSummary) -> Result<BatchSummaryDto> {
    Ok(BatchSummaryDto {
        total: count(s.total)?,
        queued: count(s.queued)?,
        running: count(s.running)?,
        succeeded: count(s.succeeded)?,
        no_gain: count(s.no_gain)?,
        failed: count(s.failed)?,
        cancelled: count(s.cancelled)?,
        processed: count(s.processed)?,
        terminal: count(s.terminal)?,
        input_bytes: bytes(s.input_bytes),
        current_bytes: bytes(s.current_bytes),
        saved_bytes: bytes(s.saved_bytes),
    })
}
fn processing(value: PngProcessing) -> ProcessingDto {
    match value {
        PngProcessing::Lossless => ProcessingDto::Lossless,
        PngProcessing::Lossy {
            mapping,
            measured_quality,
        } => ProcessingDto::Lossy {
            mapping_version: mapping.version,
            measured_quality,
        },
        PngProcessing::LosslessFallback { mapping, reason } => ProcessingDto::LosslessFallback {
            mapping_version: mapping.version,
            reason: match reason {
                LossyFallbackReason::HighBitDepth => FallbackDto::HighBitDepth,
                LossyFallbackReason::ColorMetadata => FallbackDto::ColorMetadata,
                LossyFallbackReason::RepresentationMetadata => FallbackDto::RepresentationMetadata,
                LossyFallbackReason::QualityBelowTarget { measured } => {
                    FallbackDto::QualityBelowTarget { measured }
                }
                LossyFallbackReason::TransparencyGuard => FallbackDto::TransparencyGuard,
                LossyFallbackReason::NoSizeBenefit => FallbackDto::NoSizeBenefit,
            },
        },
    }
}
fn report(value: &ProcessingReport) -> Result<ReportDto> {
    let (output_name, backup_name) = match &value.outcome {
        ProcessingOutcome::Optimized { output, backup } => {
            (Some(name(output)), backup.as_deref().map(name))
        }
        ProcessingOutcome::NoGain => (None, None),
    };
    Ok(ReportDto {
        input_bytes: DecimalU64(value.input_bytes.0),
        output_bytes: DecimalU64(value.output_bytes.0),
        // 毫秒向下取整；异常超范围返回契约错误，不截断/钳制成另一个耗时。
        elapsed_ms: DecimalU64(
            u64::try_from(value.elapsed.as_millis()).map_err(|_| QueryError::InvalidSnapshot)?,
        ),
        processing: processing(value.processing),
        output_name,
        backup_name,
        content_credentials_removed: value.content_credentials_removed,
    })
}
fn job(value: &JobSnapshot) -> Result<JobDto> {
    Ok(JobDto {
        id: count(value.id.get())?,
        attempt: value.attempt,
        source_name: name(&value.request.source),
        mode: value.request.mode,
        input_bytes: bytes(value.input_bytes),
        state: match &value.state {
            JobState::Queued => JobStateDto::Queued,
            JobState::Running {
                stage,
                cancel_requested,
            } => JobStateDto::Running {
                stage: (*stage).into(),
                cancel_requested: *cancel_requested,
            },
            JobState::Succeeded(r) => JobStateDto::Succeeded { report: report(r)? },
            JobState::NoGain(r) => JobStateDto::NoGain { report: report(r)? },
            JobState::Failed(f) => JobStateDto::Failed {
                failure: failure(f),
            },
            JobState::Cancelled => JobStateDto::Cancelled,
        },
    })
}

fn range(total: usize, request: &TaskPageRequest) -> Result<std::ops::Range<usize>> {
    let offset = usize::try_from(request.offset).map_err(|_| QueryError::InvalidPage)?;
    if offset > total {
        return Err(QueryError::InvalidPage);
    }
    Ok(offset..offset.saturating_add(usize::from(request.limit)).min(total))
}

pub(super) fn snapshot(value: &TaskSnapshot, request: &TaskPageRequest) -> Result<TaskSnapshotDto> {
    request.validate()?;
    if request
        .expected_revision
        .is_some_and(|r| r.0 != value.revision)
    {
        return Err(QueryError::StaleSnapshot {
            current_revision: DecimalU64(value.revision),
        });
    }
    let offset = request.offset;
    let page = match request.collection {
        TaskCollection::Confirmations => {
            let eligible = || {
                value
                    .batch
                    .iter()
                    .flat_map(|b| &b.jobs)
                    .filter(|job| job.content_credentials_source().is_some())
            };
            let total = eligible().count();
            let selected = range(total, request)?;
            let items = eligible()
                .skip(selected.start)
                .take(selected.len())
                .map(|job| {
                    Ok(ConfirmationDto {
                        id: count(job.id.get())?,
                        source_name: name(&job.request.source),
                        source_label: name(job.request.source.parent().unwrap_or(Path::new(""))),
                        input_bytes: bytes(job.input_bytes),
                    })
                })
                .collect::<Result<_>>()?;
            TaskPageDto::Confirmations {
                offset,
                total: count(total)?,
                items,
            }
        }
        TaskCollection::Jobs => {
            let jobs = value.batch.as_ref().map_or(&[][..], |b| b.jobs.as_slice());
            let items = jobs[range(jobs.len(), request)?]
                .iter()
                .map(job)
                .collect::<Result<_>>()?;
            TaskPageDto::Jobs {
                offset,
                total: count(jobs.len())?,
                items,
            }
        }
        TaskCollection::Candidates => {
            let files = value.import.as_ref().map_or(&[][..], |s| s.files());
            let items = range(files.len(), request)?
                .map(|index| {
                    let file = &files[index];
                    Ok(CandidateDto {
                        index: count(index)?,
                        source_name: name(&file.source),
                        input_bytes: DecimalU64(file.input_bytes.0),
                        width: file.image.width,
                        height: file.image.height,
                    })
                })
                .collect::<Result<_>>()?;
            TaskPageDto::Candidates {
                offset,
                total: count(files.len())?,
                items,
            }
        }
        TaskCollection::Issues => {
            let issues = value.import.as_ref().map_or(&[][..], |s| s.issues());
            let items = range(issues.len(), request)?
                .map(|index| {
                    let issue = &issues[index];
                    Ok(IssueDto {
                        index: count(index)?,
                        source_name: name(&issue.path),
                        issue: match &issue.kind {
                            ImportIssueKind::Failure(f) => IssueKindDto::Failure {
                                failure: failure(f),
                            },
                            ImportIssueKind::Unsupported(format) => IssueKindDto::Unsupported {
                                format: (*format).into(),
                            },
                            ImportIssueKind::Duplicate { first } => IssueKindDto::Duplicate {
                                first_name: name(first),
                            },
                            ImportIssueKind::GeneratedArtifact => IssueKindDto::GeneratedArtifact,
                        },
                    })
                })
                .collect::<Result<_>>()?;
            TaskPageDto::Issues {
                offset,
                total: count(issues.len())?,
                items,
            }
        }
    };
    Ok(TaskSnapshotDto {
        protocol_version: TASK_PROTOCOL_VERSION,
        revision: DecimalU64(value.revision),
        selection_id: value.selection.map(|s| DecimalU64(s.get())),
        phase: value.phase.into(),
        scan: value.scan_progress.map(scan).transpose()?,
        error: value.error.as_ref().map(task_error).transpose()?,
        batch: value
            .batch
            .as_ref()
            .map(|batch| -> Result<_> {
                Ok(BatchOverviewDto {
                    id: DecimalU64(batch.id.get()),
                    revision: DecimalU64(batch.revision),
                    phase: batch.phase.into(),
                    mode: batch.parameters.mode,
                    summary: summary(&batch.summary)?,
                    confirmation_count: count(
                        batch
                            .jobs
                            .iter()
                            .filter(|job| job.content_credentials_source().is_some())
                            .count(),
                    )?,
                })
            })
            .transpose()?,
        page,
    })
}
