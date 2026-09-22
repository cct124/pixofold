//! 调度测试使用可控执行器和通知，不以固定休眠猜测竞争时序；不测试图像编码。

use super::*;
use crate::model::{
    ImageInfo, OutputPolicy, PngColorType, PngMode, PngProcessing, ProcessingOutcome, QualityValue,
};
use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Gate {
    permits: Mutex<usize>,
    changed: Condvar,
    active: AtomicUsize,
    peak: AtomicUsize,
}
impl Gate {
    fn release(&self, count: usize) {
        *self.permits.lock().unwrap() += count;
        self.changed.notify_all();
    }
    fn enter(&self) {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
    }
    fn wait(&self) {
        let mut permits = self.permits.lock().unwrap();
        while *permits == 0 {
            permits = self.changed.wait(permits).unwrap();
        }
        *permits -= 1;
    }
}
struct Active(Arc<Gate>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}
struct ControlledRunner {
    gate: Arc<Gate>,
    started: mpsc::Sender<PngRequest>,
}
impl Runner for ControlledRunner {
    fn run(
        &self,
        request: &PngRequest,
        cancel: &CancellationToken,
        stage: &mut dyn FnMut(ProcessingStage),
    ) -> Result<ProcessingReport, ProcessingError> {
        self.gate.enter();
        let _active = Active(Arc::clone(&self.gate));
        stage(ProcessingStage::BeforeCommit);
        stage(ProcessingStage::Reading); // 旧/重复阶段不能让快照倒退。
        self.started.send(request.clone()).unwrap();
        self.gate.wait();
        let name = request.source.file_stem().unwrap().to_str().unwrap();
        if name == "panic" {
            panic!("确定性worker故障");
        }
        if name == "cleanup" {
            return Err(ProcessingError::CleanupFailed {
                original: Some(Box::new(ProcessingError::Cancelled)),
                source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                temporary: request.source.with_extension("tmp"),
            });
        }
        if name != "committed" {
            cancel.check()?;
        }
        if name == "fail" {
            return Err(ProcessingError::InvalidPng("确定性失败"));
        }
        let bytes = fs::metadata(&request.source).unwrap().len();
        let info = ImageInfo {
            width: 1,
            height: 1,
            bit_depth: 8,
            color_type: PngColorType::Rgb,
            interlaced: false,
        };
        Ok(ProcessingReport {
            image: info.clone(),
            output_image: info,
            processing: PngProcessing::Lossless,
            input_bytes: ByteCount(bytes),
            output_bytes: ByteCount(if name == "committed" { 0 } else { bytes }),
            elapsed: Duration::ZERO,
            outcome: if name == "committed" {
                ProcessingOutcome::Optimized {
                    output: request.source.clone(),
                    backup: None,
                }
            } else {
                ProcessingOutcome::NoGain
            },
        })
    }
}
struct Harness {
    service: BatchService,
    gate: Arc<Gate>,
    started: Receiver<PngRequest>,
    directory: tempfile::TempDir,
}
impl Harness {
    fn new(config: BatchConfig) -> Self {
        let gate = Arc::new(Gate::default());
        let (sender, started) = mpsc::channel();
        let runner = Arc::new(ControlledRunner {
            gate: Arc::clone(&gate),
            started: sender,
        });
        Self {
            service: BatchService::with_runner(config, runner).unwrap(),
            gate,
            started,
            directory: tempfile::tempdir().unwrap(),
        }
    }
    fn request(&self, names: &[&str]) -> BatchRequest {
        let items = names
            .iter()
            .map(|name| {
                let source = self.directory.path().join(format!("{name}.png"));
                fs::write(&source, b"synthetic runner input").unwrap();
                BatchItem {
                    source,
                    output: OutputPolicy::Overwrite,
                }
            })
            .collect();
        BatchRequest {
            items,
            parameters: BatchParameters::default(),
        }
    }
    fn started(&self) -> PngRequest {
        self.started
            .recv_timeout(WAIT)
            .expect("worker应已进入受控执行器")
    }
    fn finish(&self, id: BatchId) -> BatchSnapshot {
        self.gate.release(1000);
        self.service.wait(id, WAIT).unwrap()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.gate.release(100_000);
    } // 测试断言panic时也先释放gate，随后service安全join。
}

#[test]
fn concurrency_and_admission_bounds_hold_for_whole_run() {
    let h = Harness::new(BatchConfig {
        workers: 2,
        max_jobs: 4,
        ..BatchConfig::default()
    });
    let request = h.request(&["a", "b", "c", "d"]);
    let id = h.service.start(request.clone()).unwrap();
    h.started();
    h.started();
    let snapshot = h.service.snapshot().unwrap();
    assert_eq!((snapshot.active_workers, snapshot.summary.queued), (2, 2));
    assert_eq!(
        snapshot.reserved_working_bytes.0,
        estimate_working_set(request.parameters).unwrap().0 * 2
    );
    assert!(matches!(h.service.start(request), Err(BatchError::Busy)));
    assert!(matches!(h.service.clear(id), Err(BatchError::Busy)));
    assert!(matches!(
        h.service.start(h.request(&["e", "f", "g", "h", "i"])),
        Err(BatchError::TooManyJobs)
    ));
    let finished = h.finish(id);
    assert_eq!(finished.summary.no_gain, 4);
    assert_eq!(
        (finished.active_workers, finished.reserved_working_bytes.0),
        (0, 0)
    );
    assert_eq!(h.gate.peak.load(Ordering::SeqCst), 2);
}

#[test]
fn memory_reservation_serializes_work_and_oversized_jobs_fail_without_waiting() {
    let reserve = estimate_working_set(BatchParameters::default()).unwrap();
    let h = Harness::new(BatchConfig {
        workers: 2,
        working_set_budget: reserve,
        ..BatchConfig::default()
    });
    let id = h.service.start(h.request(&["a", "b", "c"])).unwrap();
    h.started();
    let snapshot = h.service.snapshot().unwrap();
    assert_eq!((snapshot.active_workers, snapshot.summary.queued), (1, 2));
    assert_eq!(h.finish(id).summary.no_gain, 3);
    assert_eq!(h.gate.peak.load(Ordering::SeqCst), 1);
    let h = Harness::new(BatchConfig {
        working_set_budget: ByteCount(reserve.0 - 1),
        ..BatchConfig::default()
    });
    let id = h.service.start(h.request(&["a"])).unwrap();
    let finished = h.service.wait(id, WAIT).unwrap();
    assert!(matches!(
        &finished.jobs[0].state,
        JobState::Failed(JobFailure {
            code: JobErrorCode::ResourceLimit,
            ..
        })
    ));
    assert_eq!(h.gate.peak.load(Ordering::SeqCst), 0);
}

#[test]
fn settings_are_copied_and_stage_revision_never_regresses() {
    let h = Harness::new(BatchConfig::default());
    let mut request = h.request(&["a", "b"]);
    request.parameters.mode = PngMode::Lossy {
        quality: QualityValue::new(73).unwrap(),
    };
    let original = request.parameters.mode;
    let id = h.service.start(request.clone()).unwrap();
    assert_eq!(h.started().mode, original);
    request.parameters.mode = PngMode::Lossless;
    let mut snapshot = h.service.snapshot().unwrap();
    let revision = snapshot.revision;
    snapshot.jobs[1].request.mode = PngMode::Lossless;
    assert!(matches!(
        snapshot.jobs[0].state,
        JobState::Running {
            stage: ProcessingStage::BeforeCommit,
            ..
        }
    ));
    h.gate.release(1);
    assert_eq!(h.started().mode, original);
    let finished = h.finish(id);
    assert!(finished.revision > revision);
    assert_eq!(finished.jobs[1].request.mode, original);
    assert_eq!(finished.jobs[1].mapping.unwrap().target, 73);
}

#[test]
fn cancel_wait_timeout_and_committed_success_have_distinct_semantics() {
    for (first, succeeded) in [("a", false), ("committed", true)] {
        let h = Harness::new(BatchConfig::default());
        let id = h.service.start(h.request(&[first, "b", "c"])).unwrap();
        h.started();
        assert!(matches!(
            h.service.wait(id, Duration::ZERO),
            Err(BatchError::TimedOut)
        ));
        assert_eq!(h.service.snapshot().unwrap().phase, BatchPhase::Running);
        h.service.cancel(id).unwrap();
        let cancelling = h.service.snapshot().unwrap();
        assert_eq!(cancelling.phase, BatchPhase::Cancelling);
        assert_eq!(
            (cancelling.summary.cancelled, cancelling.summary.processed),
            (2, 0)
        );
        assert!(matches!(
            cancelling.jobs[0].state,
            JobState::Running {
                cancel_requested: true,
                ..
            }
        ));
        assert_eq!(cancelling.active_workers, 1);
        assert!(matches!(h.service.clear(id), Err(BatchError::Busy)));
        assert!(matches!(
            h.service.retry(
                id,
                RetryRequest {
                    parameters: BatchParameters::default(),
                    jobs: vec![]
                }
            ),
            Err(BatchError::Busy)
        ));
        let finished = h.finish(id);
        assert_eq!(finished.phase, BatchPhase::Finished);
        assert_eq!(finished.summary.succeeded, usize::from(succeeded));
        assert_eq!(finished.summary.cancelled, if succeeded { 2 } else { 3 });
        assert_eq!(finished.summary.processed, usize::from(succeeded));
        assert_eq!(finished.summary.terminal, 3);
        let revision = finished.revision;
        h.service.cancel(id).unwrap();
        assert_eq!(h.service.snapshot().unwrap().revision, revision);
    }
}

#[test]
fn retry_preserves_completed_rows_and_freezes_new_parameters_and_destinations() {
    let h = Harness::new(BatchConfig::default());
    let id = h
        .service
        .start(h.request(&["committed", "b", "c"]))
        .unwrap();
    h.started();
    h.gate.release(1);
    h.started();
    h.service.cancel(id).unwrap();
    h.gate.release(1);
    let first = h.service.wait(id, WAIT).unwrap();
    assert_eq!(first.summary.succeeded, 1);
    let parameters = BatchParameters {
        mode: PngMode::Lossy {
            quality: QualityValue::new(40).unwrap(),
        },
        ..BatchParameters::default()
    };
    let targets: Vec<_> = first.jobs[1..]
        .iter()
        .map(|j| RetryJob {
            id: j.id,
            output: OutputPolicy::Copy {
                destination: h.directory.path().join(format!("retry-{}.png", j.id.get())),
            },
        })
        .collect();
    h.service
        .retry(
            id,
            RetryRequest {
                parameters,
                jobs: targets.clone(),
            },
        )
        .unwrap();
    let running = h.started();
    assert_eq!(running.mode, parameters.mode);
    assert!(matches!(running.output, OutputPolicy::Copy { .. }));
    let finished = h.finish(id);
    assert_eq!(finished.id, id);
    assert_eq!(
        finished.jobs.iter().map(|j| j.attempt).collect::<Vec<_>>(),
        [1, 2, 2]
    );
    assert_eq!(finished.jobs[0].request.mode, PngMode::Lossless);
    assert_eq!(finished.summary.succeeded, 1);
    assert_eq!(finished.summary.no_gain, 2);
    assert!(finished.revision > first.revision);
    let revision = finished.revision;
    assert!(matches!(
        h.service.retry(
            id,
            RetryRequest {
                parameters,
                jobs: targets
            }
        ),
        Err(BatchError::InvalidRetry)
    ));
    assert_eq!(h.service.snapshot().unwrap().revision, revision);
}

#[test]
fn per_job_errors_and_panics_release_permits_and_other_jobs_continue() {
    let h = Harness::new(BatchConfig::default());
    let id = h
        .service
        .start(h.request(&["fail", "panic", "ok"]))
        .unwrap();
    let finished = h.finish(id);
    assert_eq!((finished.summary.failed, finished.summary.no_gain), (2, 1));
    for (job, code) in finished
        .jobs
        .iter()
        .zip([JobErrorCode::InvalidInput, JobErrorCode::WorkerPanicked])
    {
        assert!(matches!(&job.state, JobState::Failed(failure) if failure.code == code));
    }
    assert_eq!(finished.reserved_working_bytes.0, 0);
    assert_eq!(h.gate.active.load(Ordering::SeqCst), 0);
    h.service.clear(id).unwrap();
    assert!(h.service.snapshot().is_none());
    let next = h.service.start(h.request(&["next"])).unwrap();
    assert_ne!(id, next);
    assert!(matches!(h.service.cancel(id), Err(BatchError::NoSuchBatch)));
    assert_eq!(h.finish(next).summary.no_gain, 1);
}

#[test]
fn shutdown_cancels_and_joins_and_drop_releases_ownership() {
    let mut h = Harness::new(BatchConfig::default());
    let id = h.service.start(h.request(&["a", "b"])).unwrap();
    h.started();
    // 请求取消后由gate确定性允许执行器返回；shutdown不依赖UI销毁回调。
    h.service.cancel(id).unwrap();
    h.gate.release(1);
    h.service.shutdown().unwrap();
    h.service.shutdown().unwrap();
    let finished = h.service.snapshot().unwrap();
    assert_eq!(finished.summary.cancelled, 2);
    assert_eq!(h.gate.active.load(Ordering::SeqCst), 0);
    assert!(matches!(
        h.service.start(h.request(&["new"])),
        Err(BatchError::Closed)
    ));
    let weak = Arc::downgrade(&h.service.shared);
    drop(h);
    assert!(weak.upgrade().is_none(), "worker不能形成所有权环");
}

#[test]
fn shutdown_and_drop_wait_for_active_work_without_prior_cancel() {
    // 观察线程在服务进入关闭后才释放执行器，证明shutdown/Drop本身请求取消并等待。
    struct Release(Arc<Gate>);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.release(1000);
        }
    }
    for explicit in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let gate = Arc::new(Gate::default());
        let (sender, started) = mpsc::channel();
        let mut service = BatchService::with_runner(
            BatchConfig::default(),
            Arc::new(ControlledRunner {
                gate: Arc::clone(&gate),
                started: sender,
            }),
        )
        .unwrap();
        let _release_on_failure = Release(Arc::clone(&gate));
        let items = ["active", "queued"].map(|name| {
            let source = directory.path().join(format!("{name}.png"));
            fs::write(&source, b"controlled input").unwrap();
            BatchItem {
                source,
                output: OutputPolicy::Overwrite,
            }
        });
        service
            .start(BatchRequest {
                items: items.into(),
                parameters: BatchParameters::default(),
            })
            .unwrap();
        started.recv_timeout(WAIT).unwrap();
        let shared = Arc::clone(&service.shared);
        std::thread::scope(|scope| {
            let observer = scope.spawn(|| {
                let _release = Release(Arc::clone(&gate));
                let mut state = shared.lock();
                let began = Instant::now();
                while !state.closed {
                    let remaining = WAIT.checked_sub(began.elapsed()).expect("服务必须进入关闭");
                    state = shared.changed.wait_timeout(state, remaining).unwrap().0;
                }
                let snapshot = state.batch.as_ref().unwrap().snapshot();
                drop(state);
                assert_eq!(snapshot.phase, BatchPhase::Cancelling);
                assert_eq!(snapshot.active_workers, 1);
                assert!(snapshot.reserved_working_bytes.0 > 0);
                assert_eq!(snapshot.summary.cancelled, 1);
                assert!(matches!(
                    snapshot.jobs[0].state,
                    JobState::Running {
                        cancel_requested: true,
                        ..
                    }
                ));
                assert_eq!(gate.active.load(Ordering::SeqCst), 1);
            });
            if explicit {
                service.shutdown().unwrap();
            }
            drop(service);
            observer.join().unwrap();
        });
        let state = shared.lock();
        let finished = state.batch.as_ref().unwrap().snapshot();
        assert_eq!(finished.summary.cancelled, 2);
        assert_eq!(finished.phase, BatchPhase::Finished);
        assert_eq!(finished.reserved_working_bytes.0, 0);
        assert_eq!(gate.active.load(Ordering::SeqCst), 0);
        drop(state);
        let weak = Arc::downgrade(&shared);
        drop(shared);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn poisoned_state_fails_closed_but_still_joins_and_preserves_snapshots() {
    let mut h = Harness::new(BatchConfig::default());
    let id = h.service.start(h.request(&["active", "queued"])).unwrap();
    h.started();
    let shared = Arc::clone(&h.service.shared);
    assert!(
        std::panic::catch_unwind(|| {
            let _guard = shared.state.lock().unwrap();
            panic!("确定性状态锁故障");
        })
        .is_err()
    );
    let snapshot = h.service.snapshot().unwrap();
    assert_eq!(snapshot.phase, BatchPhase::Cancelling);
    assert!(matches!(
        snapshot.jobs[1].state,
        JobState::Failed(JobFailure {
            code: JobErrorCode::ServiceFault,
            ..
        })
    ));
    assert!(matches!(
        h.service.wait(id, WAIT),
        Err(BatchError::ServiceFault)
    ));
    assert!(matches!(
        h.service.start(h.request(&["new"])),
        Err(BatchError::ServiceFault)
    ));
    h.gate.release(1);
    assert!(matches!(
        h.service.shutdown(),
        Err(BatchError::ServiceFault)
    ));
    let finished = h.service.snapshot().unwrap();
    assert_eq!(finished.phase, BatchPhase::Finished);
    assert_eq!(finished.summary.terminal, 2);
    assert_eq!(finished.reserved_working_bytes.0, 0);
    assert_eq!(h.gate.active.load(Ordering::SeqCst), 0);
}

#[test]
fn cleanup_failure_after_cancel_remains_a_failure_with_recovery_context() {
    let h = Harness::new(BatchConfig::default());
    let id = h.service.start(h.request(&["cleanup", "queued"])).unwrap();
    let request = h.started();
    h.service.cancel(id).unwrap();
    let finished = h.finish(id);
    assert_eq!(
        (finished.summary.failed, finished.summary.cancelled),
        (1, 1)
    );
    let JobState::Failed(failure) = &finished.jobs[0].state else {
        panic!("清理失败不能伪装为已完成取消");
    };
    assert_eq!(failure.code, JobErrorCode::CleanupFailed);
    let Some(ProcessingError::CleanupFailed {
        original,
        source,
        temporary,
    }) = failure.cause.as_deref()
    else {
        panic!("必须保留原始错误与残留文件路径");
    };
    assert!(matches!(
        original.as_deref(),
        Some(ProcessingError::Cancelled)
    ));
    assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(temporary, &request.source.with_extension("tmp"));
    assert_eq!(finished.reserved_working_bytes.0, 0);
}

#[test]
fn config_empty_input_and_estimate_overflow_are_rejected() {
    for config in [
        BatchConfig {
            workers: 0,
            ..BatchConfig::default()
        },
        BatchConfig {
            workers: 33,
            ..BatchConfig::default()
        },
        BatchConfig {
            max_jobs: 0,
            ..BatchConfig::default()
        },
        BatchConfig {
            max_jobs: 100_001,
            ..BatchConfig::default()
        },
        BatchConfig {
            working_set_budget: ByteCount(0),
            ..BatchConfig::default()
        },
    ] {
        assert!(matches!(
            BatchService::new(config),
            Err(BatchError::InvalidConfig)
        ));
    }
    let h = Harness::new(BatchConfig::default());
    assert!(matches!(
        h.service.start(h.request(&[])),
        Err(BatchError::EmptyBatch)
    ));
    let mut params = BatchParameters {
        mode: PngMode::Lossy {
            quality: QualityValue::default(),
        },
        ..BatchParameters::default()
    };
    params.limits.max_pixels = u64::MAX;
    assert!(estimate_working_set(params).is_err());
    params.limits.max_input_bytes = ByteCount(0);
    assert!(matches!(
        estimate_working_set(params),
        Err(BatchError::InvalidParameters(_))
    ));
}

#[test]
fn missing_size_and_sum_overflow_remain_unknown_in_summary() {
    let h = Harness::new(BatchConfig::default());
    let mut request = h.request(&["a", "b"]);
    request.items[0].source = Path::new("missing-parent").join("source.png");
    let id = h.service.start(request).unwrap();
    let mut finished = h.finish(id);
    assert_eq!(finished.summary.failed, 1);
    assert_eq!(finished.summary.input_bytes, None);
    assert_eq!(finished.summary.current_bytes, None);
    assert_eq!(finished.summary.saved_bytes, Some(ByteCount(0)));
    for job in &mut finished.jobs {
        job.input_bytes = Some(ByteCount(u64::MAX));
    }
    assert_eq!(BatchSummary::from_jobs(&finished.jobs).input_bytes, None);
}
