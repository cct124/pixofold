//! 扫描诊断只输出有界类别计数；路径与底层错误仍留在扫描报告供受控UI查询。
use pixofold_core::{
    batch::{ImageError, JobErrorCode},
    import::{ImportIssueKind, ImportScan},
    jpeg::JpegError,
};
use std::collections::BTreeMap;

fn category(issue: &ImportIssueKind) -> &'static str {
    match issue {
        ImportIssueKind::Duplicate { .. } => "duplicate",
        ImportIssueKind::GeneratedArtifact => "generated_artifact",
        ImportIssueKind::Unsupported(_) => "unsupported_format",
        ImportIssueKind::Failure(failure) => {
            if matches!(
                failure.cause.as_deref(),
                Some(ImageError::Jpeg(JpegError::ProtectedMetadata(0xeb)))
            ) {
                return "unsupported_jpeg_credentials";
            }
            match failure.code {
                JobErrorCode::InvalidInput => "invalid_input",
                JobErrorCode::UnsupportedFormat => "unsupported_format",
                JobErrorCode::UnsupportedAnimation => "unsupported_animation",
                JobErrorCode::UnsupportedContentCredentials => "unsupported_content_credentials",
                JobErrorCode::UnsupportedMetadata => "unsupported_metadata",
                JobErrorCode::ResourceLimit => "resource_limit",
                JobErrorCode::Decode => "decode",
                JobErrorCode::Encode => "encode",
                JobErrorCode::Validation => "validation",
                JobErrorCode::TargetConflict => "target_conflict",
                JobErrorCode::SourceChanged => "source_changed",
                JobErrorCode::Io => "io",
                JobErrorCode::CommitFailed => "commit_failed",
                JobErrorCode::CleanupFailed => "cleanup_failed",
                JobErrorCode::WorkerPanicked => "worker_panicked",
                JobErrorCode::ServiceFault => "service_fault",
                JobErrorCode::ToolIdentity => "tool_identity",
                JobErrorCode::ToolIo => "tool_io",
                JobErrorCode::ToolExit => "tool_exit",
                JobErrorCode::TimedOut => "timed_out",
            }
        }
    }
}

pub(crate) fn record_scan(scan: &ImportScan) {
    let progress = scan.progress();
    tracing::info!(target: "pixofold", event = "import_scan_finished",
        count = scan.files().len(), result = ?progress.status,
        accepted = progress.accepted, rejected = progress.rejected,
        excluded = progress.excluded, duplicates = progress.duplicates);
    // 键仅来自上述穷举枚举，日志数量不会随输入文件数增长。
    let mut counts = BTreeMap::<&'static str, usize>::new();
    for issue in scan.issues() {
        *counts.entry(category(&issue.kind)).or_default() += 1;
    }
    for (error_code, count) in counts {
        tracing::info!(target: "pixofold", event = "import_scan_issue_summary", error_code, count);
    }
}
