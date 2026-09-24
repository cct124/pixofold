//! 公开批量API与真实PNG/隔离文件系统回归；不依赖GUI，也不以休眠驱动取消。

use pixofold_core::{
    batch::*,
    model::{ByteCount, OutputPolicy, PngMode, PngProcessing, ProcessingOutcome, QualityValue},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(30);
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}
fn source(directory: &Path, name: &str, sample: &str) -> (PathBuf, Vec<u8>) {
    let original = fixture(sample);
    let path = directory.join(name);
    fs::write(&path, &original).unwrap();
    (path, original)
}
fn item(source: &Path, target: &Path) -> BatchItem {
    BatchItem {
        source: source.to_owned(),
        output: OutputPolicy::Copy {
            destination: target.to_owned(),
        },
    }
}
fn request(items: Vec<BatchItem>) -> BatchRequest {
    BatchRequest {
        items,
        parameters: BatchParameters::default(),
    }
}

#[test]
fn real_mixed_batch_reports_only_actual_savings_and_keeps_overwrite_backup() {
    let dir = tempfile::tempdir().unwrap();
    let (rgb, original_rgb) = source(dir.path(), "rgb.png", "rgb8.png");
    let (alpha, original_alpha) = source(dir.path(), "alpha.png", "gradient-binary-alpha.png");
    let (tiny, original_tiny) = source(dir.path(), "tiny.png", "already-optimized.png");
    let (bad, original_bad) = source(dir.path(), "bad.png", "fake.png");
    let copy = dir.path().join("copy.png");
    let tiny_output = dir.path().join("tiny-output.png");
    let bad_output = dir.path().join("bad-output.png");
    let mut service = BatchService::new(BatchConfig {
        workers: 2,
        ..BatchConfig::default()
    })
    .unwrap();
    let mut request = request(vec![
        item(&rgb, &copy),
        BatchItem {
            source: alpha.clone(),
            output: OutputPolicy::Overwrite,
        },
        item(&tiny, &tiny_output),
        item(&bad, &bad_output),
    ]);
    request.parameters.mode = PngMode::Lossy {
        quality: QualityValue::default(),
    };
    let id = service.start(request).unwrap();
    let snapshot = service.wait(id, WAIT).unwrap();
    assert_eq!(snapshot.phase, BatchPhase::Finished);
    assert_eq!(
        (
            snapshot.summary.succeeded,
            snapshot.summary.no_gain,
            snapshot.summary.failed
        ),
        (2, 1, 1)
    );
    assert_eq!(
        (snapshot.summary.processed, snapshot.summary.terminal),
        (4, 4)
    );
    let input = [
        original_rgb.len(),
        original_alpha.len(),
        original_tiny.len(),
        original_bad.len(),
    ]
    .iter()
    .sum::<usize>() as u64;
    let current = fs::metadata(&copy).unwrap().len()
        + fs::metadata(&alpha).unwrap().len()
        + original_tiny.len() as u64
        + original_bad.len() as u64;
    assert_eq!(snapshot.summary.input_bytes, Some(ByteCount(input)));
    assert_eq!(snapshot.summary.current_bytes, Some(ByteCount(current)));
    assert_eq!(
        snapshot.summary.saved_bytes,
        Some(ByteCount(input - current))
    );
    assert_eq!(fs::read(rgb).unwrap(), original_rgb);
    assert_eq!(fs::read(tiny).unwrap(), original_tiny);
    assert_eq!(fs::read(bad).unwrap(), original_bad);
    assert!(!tiny_output.exists() && !bad_output.exists());
    let JobState::Succeeded(report) = &snapshot.jobs[1].state else {
        panic!("透明图片应真实量化成功");
    };
    assert!(matches!(report.processing, PngProcessing::Lossy { .. }));
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = &report.outcome
    else {
        panic!("覆盖必须保留备份");
    };
    assert_eq!(fs::read(backup).unwrap(), original_alpha);
    assert_eq!(snapshot.active_workers, 0);
    assert_eq!(snapshot.reserved_working_bytes, ByteCount(0));
    service.clear(id).unwrap();
    assert!(backup.exists(), "清除记录不删除恢复备份");
    service.shutdown().unwrap();
}

#[test]
fn duplicate_sources_and_hardlink_aliases_reject_whole_batch_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let (src, original) = source(dir.path(), "source.png", "rgb8.png");
    let alias = dir.path().join("alias.png");
    fs::hard_link(&src, &alias).unwrap();
    let service = BatchService::new(BatchConfig::default()).unwrap();
    for other in [&src, &alias] {
        let result = service.start(request(vec![
            item(&src, &dir.path().join("a.png")),
            item(other, &dir.path().join("b.png")),
        ]));
        assert!(matches!(
            result,
            Err(BatchError::PathConflict {
                kind: PathConflictKind::DuplicateSource,
                ..
            })
        ));
        assert!(service.snapshot().is_none());
        assert_eq!(fs::read(&src).unwrap(), original);
        assert!(!dir.path().join("a.png").exists());
    }
    // 拒绝后的准备门闩必须释放，不能永久Busy。
    let id = service
        .start(request(vec![item(&src, &dir.path().join("ok.png"))]))
        .unwrap();
    assert_eq!(service.wait(id, WAIT).unwrap().summary.succeeded, 1);
}

#[test]
fn overlapping_outputs_and_output_into_other_input_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (a, original_a) = source(dir.path(), "a.png", "rgb8.png");
    let (b, original_b) = source(dir.path(), "b.png", "rgba8.png");
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let target = dir.path().join("output.png");
    let service = BatchService::new(BatchConfig::default()).unwrap();
    let same = sub.join("..").join("output.png");
    let result = service.start(request(vec![item(&a, &target), item(&b, &same)]));
    assert!(matches!(
        result,
        Err(BatchError::PathConflict {
            kind: PathConflictKind::DuplicateOutput,
            ..
        })
    ));
    let result = service.start(request(vec![item(&a, &b), item(&b, &target)]));
    assert!(matches!(
        result,
        Err(BatchError::PathConflict {
            kind: PathConflictKind::OutputIsInput,
            ..
        })
    ));
    let alias = dir.path().join("alias.png");
    fs::hard_link(&b, &alias).unwrap();
    let result = service.start(request(vec![item(&a, &alias), item(&b, &target)]));
    assert!(matches!(
        result,
        Err(BatchError::PathConflict {
            kind: PathConflictKind::OutputIsInput,
            ..
        })
    ));
    assert_eq!(fs::read(a).unwrap(), original_a);
    assert_eq!(fs::read(b).unwrap(), original_b);
    assert!(!target.exists());
}

#[cfg(windows)]
#[test]
fn windows_unicode_case_aliases_are_conservatively_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = source(dir.path(), "a.png", "rgb8.png");
    let (b, _) = source(dir.path(), "b.png", "rgba8.png");
    let service = BatchService::new(BatchConfig::default()).unwrap();
    let result = service.start(request(vec![
        item(&a, &dir.path().join("résultat.png")),
        item(&b, &dir.path().join("RÉSULTAT.PNG")),
    ]));
    assert!(matches!(
        result,
        Err(BatchError::PathConflict {
            kind: PathConflictKind::DuplicateOutput,
            ..
        })
    ));
    assert!(service.snapshot().is_none());
}

#[test]
fn retry_rechecks_paths_retains_success_and_applies_current_quality_and_target() {
    let dir = tempfile::tempdir().unwrap();
    let (a, original_a) = source(dir.path(), "a.png", "rgb8.png");
    let (b, original_b) = source(dir.path(), "b.png", "gradient-rgb8.png");
    let first_output = dir.path().join("a-output.png");
    let conflict = dir.path().join("existing.png");
    fs::write(&conflict, b"unrelated file").unwrap();
    let service = BatchService::new(BatchConfig::default()).unwrap();
    let id = service
        .start(request(vec![item(&a, &first_output), item(&b, &conflict)]))
        .unwrap();
    let first = service.wait(id, WAIT).unwrap();
    assert_eq!((first.summary.succeeded, first.summary.failed), (1, 1));
    assert!(matches!(
        first.jobs[1].state,
        JobState::Failed(JobFailure {
            code: JobErrorCode::TargetConflict,
            ..
        })
    ));
    let before = fs::read(&first_output).unwrap();
    let mut retry = RetryRequest {
        parameters: BatchParameters {
            mode: PngMode::Lossy {
                quality: QualityValue::new(40).unwrap(),
            },
            ..BatchParameters::default()
        },
        jobs: vec![RetryJob {
            id: first.jobs[1].id,
            metadata: Default::default(),
            output: OutputPolicy::Copy {
                destination: first_output.clone(),
            },
        }],
    };
    assert!(matches!(
        service.retry(id, retry.clone()),
        Err(BatchError::PathConflict { .. })
    ));
    assert_eq!(service.snapshot().unwrap().revision, first.revision);
    retry.jobs.push(retry.jobs[0].clone());
    assert!(matches!(
        service.retry(id, retry.clone()),
        Err(BatchError::InvalidRetry)
    ));
    retry.jobs.pop();
    let second_output = dir.path().join("b-output.png");
    retry.jobs[0].output = OutputPolicy::Copy {
        destination: second_output.clone(),
    };
    service.retry(id, retry).unwrap();
    let finished = service.wait(id, WAIT).unwrap();
    assert_eq!(finished.summary.succeeded, 2);
    assert_eq!((finished.jobs[0].attempt, finished.jobs[1].attempt), (1, 2));
    assert_eq!(finished.jobs[0].request.mode, PngMode::Lossless);
    let JobState::Succeeded(report) = &finished.jobs[1].state else {
        panic!("重试应成功");
    };
    assert!(
        matches!(report.processing,PngProcessing::Lossy { mapping, .. } if mapping.target == 40)
    );
    assert_eq!(fs::read(&first_output).unwrap(), before);
    assert_eq!(fs::read(conflict).unwrap(), b"unrelated file");
    assert_eq!(fs::read(a).unwrap(), original_a);
    assert_eq!(fs::read(b).unwrap(), original_b);
    assert!(second_output.exists());
}

#[test]
fn missing_sources_and_decode_limits_are_per_job_failures_and_retryable() {
    let dir = tempfile::tempdir().unwrap();
    let (src, original) = source(dir.path(), "source.png", "rgb8.png");
    let missing = dir.path().join("missing.png");
    let service = BatchService::new(BatchConfig::default()).unwrap();
    let mut request = request(vec![
        item(&missing, &dir.path().join("missing-out.png")),
        item(&src, &dir.path().join("out.png")),
    ]);
    request.parameters.limits.max_dimension = 1;
    let id = service.start(request).unwrap();
    let failed = service.wait(id, WAIT).unwrap();
    assert_eq!(failed.summary.failed, 2);
    assert!(failed.jobs[0].input_bytes.is_none());
    assert!(matches!(
        failed.jobs[1].state,
        JobState::Failed(JobFailure {
            code: JobErrorCode::ResourceLimit,
            ..
        })
    ));
    fs::write(&missing, &original).unwrap();
    service
        .retry(
            id,
            RetryRequest {
                parameters: BatchParameters::default(),
                jobs: failed
                    .jobs
                    .iter()
                    .map(|j| RetryJob {
                        id: j.id,
                        metadata: Default::default(),
                        output: j.request.output.clone(),
                    })
                    .collect(),
            },
        )
        .unwrap();
    let finished = service.wait(id, WAIT).unwrap();
    assert_eq!(finished.summary.succeeded, 2);
    assert_eq!(
        finished.summary.input_bytes,
        Some(ByteCount(original.len() as u64 * 2))
    );
    assert_eq!(fs::read(src).unwrap(), original);
}

#[cfg(windows)]
#[test]
fn commit_failure_retains_recoverable_backup_in_job_error_and_retry_does_not_remove_it() {
    use pixofold_core::model::ProcessingError;
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (src, original) = source(dir.path(), "source.png", "rgb8.png");
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&src)
        .unwrap();
    let service = BatchService::new(BatchConfig::default()).unwrap();
    let id = service
        .start(request(vec![BatchItem {
            source: src.clone(),
            output: OutputPolicy::Overwrite,
        }]))
        .unwrap();
    let failed = service.wait(id, WAIT).unwrap();
    let JobState::Failed(failure) = &failed.jobs[0].state else {
        panic!("占用文件不应覆盖成功");
    };
    assert_eq!(failure.code, JobErrorCode::CommitFailed);
    let Some(error) = &failure.cause else {
        panic!("必须保留原始错误");
    };
    let ProcessingError::CommitFailed { backup, .. } = error.as_ref() else {
        panic!("必须提供恢复路径");
    };
    assert_eq!(fs::read(backup).unwrap(), original);
    assert_eq!(fs::read(&src).unwrap(), original);
    drop(held);
    service
        .retry(
            id,
            RetryRequest {
                parameters: BatchParameters::default(),
                jobs: vec![RetryJob {
                    id: failed.jobs[0].id,
                    metadata: Default::default(),
                    output: OutputPolicy::Overwrite,
                }],
            },
        )
        .unwrap();
    assert_eq!(service.wait(id, WAIT).unwrap().summary.succeeded, 1);
    assert_eq!(fs::read(backup).unwrap(), original, "重试不回收旧恢复备份");
}
