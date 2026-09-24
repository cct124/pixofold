//! 应用协调公开行为回归：真实隔离文件+可控只读导入门闩，不用固定sleep猜测时序。

use super::*;
use pixofold_core::{
    batch::{BatchError, BatchParameters, JobState, RetryJob},
    import::{ImportError, ImportOutput, ScanOptions, ScanProgress},
    model::{OutputPolicy, PngMode, ProcessingOutcome},
};
use std::{fs, path::Path, sync::mpsc};

const WAIT: Duration = Duration::from_secs(20);
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}
fn sample(dir: &Path, name: &str, image: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, fixture(image)).unwrap();
    path
}
fn copy_settings() -> TaskSettings {
    TaskSettings {
        parameters: BatchParameters::default(),
        output: ImportOutput::CopyBeside,
    }
}
fn phase(control: &TaskControl, target: TaskPhase) -> TaskSnapshot {
    let deadline = Instant::now() + WAIT;
    let mut snapshot = control.snapshot();
    while snapshot.phase != target {
        assert_ne!(
            snapshot.phase,
            TaskPhase::Closed,
            "意外关闭: {:?}",
            snapshot.error
        );
        snapshot = control
            .wait_for_change(
                snapshot.revision,
                deadline.saturating_duration_since(Instant::now()),
            )
            .unwrap();
    }
    snapshot
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Point {
    Scan,
    Candidate,
    Plan,
}
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
    entered: mpsc::Sender<()>,
}
impl Gate {
    fn stop(&self) {
        let mut open = self.open.lock().unwrap();
        if !*open {
            self.entered.send(()).unwrap();
        }
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
struct PausedImport {
    gate: Arc<Gate>,
    point: Point,
}
impl worker::ImportBackend for PausedImport {
    fn scan(
        &self,
        roots: &[PathBuf],
        options: ScanOptions,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(&ScanProgress),
    ) -> Result<ImportScan, ImportError> {
        if self.point == Point::Scan {
            self.gate.stop();
        }
        worker::NativeImport.scan(roots, options, cancel, &mut |p| {
            progress(p);
            if self.point == Point::Candidate && p.accepted == 1 {
                self.gate.stop();
            }
        })
    }
    fn plan(
        &self,
        scan: &ImportScan,
        settings: &TaskSettings,
    ) -> Result<pixofold_core::batch::BatchRequest, ImportError> {
        if self.point == Point::Plan {
            self.gate.stop();
        }
        worker::NativeImport.plan(scan, settings)
    }
}
struct Paused {
    runtime: TaskRuntime,
    gate: Arc<Gate>,
    entered: mpsc::Receiver<()>,
}
impl Paused {
    fn new(point: Point) -> Self {
        let (entered, receiver) = mpsc::channel();
        let gate = Arc::new(Gate {
            open: Mutex::new(false),
            changed: Condvar::new(),
            entered,
        });
        let runtime = TaskRuntime::with_backend(
            TaskConfig::default(),
            Arc::new(PausedImport {
                gate: gate.clone(),
                point,
            }),
        )
        .unwrap();
        Self {
            runtime,
            gate,
            entered: receiver,
        }
    }
    fn entered(&self) {
        self.entered.recv_timeout(WAIT).unwrap();
    }
}
impl Drop for Paused {
    fn drop(&mut self) {
        self.gate.release();
    }
}

#[test]
fn real_auto_import_freezes_settings_and_does_not_need_a_window() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "source.wrong", "rgb8.png");
    let mut runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    let mut settings = copy_settings();
    let id = control
        .import(vec![source.clone()], Some(settings.clone()))
        .unwrap();
    settings.parameters.mode = TaskSettings::default().parameters.mode;
    assert!(matches!(settings.parameters.mode, PngMode::Lossy { .. }));
    let finished = phase(&control, TaskPhase::Finished);
    let batch = finished.batch.unwrap();
    assert_eq!(batch.summary.succeeded, 1);
    assert_eq!(batch.jobs[0].request.mode, PngMode::Lossless);
    assert!(dir.path().join("source_compressed.png").exists());
    assert_eq!(fs::read(source).unwrap(), fixture("rgb8.png"));
    assert!(matches!(
        control.start(id, settings),
        Err(TaskError::NotReady)
    ));
    runtime.shutdown().unwrap();
    assert_eq!(control.snapshot().phase, TaskPhase::Closed);
}

#[test]
fn settings_errors_retain_identical_frozen_scan_and_allow_one_corrected_start() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "photo.png", "rgb8.png");
    let paused = Paused::new(Point::Plan);
    let control = paused.runtime.control();
    let id = control.import(vec![source], None).unwrap();
    let ready = phase(&control, TaskPhase::Ready);
    let mut invalid = copy_settings();
    invalid.parameters.limits.max_input_bytes.0 = 0;
    control.start(id, invalid).unwrap();
    paused.entered();
    assert!(matches!(
        control.start(id, copy_settings()),
        Err(TaskError::NotReady)
    ));
    assert!(matches!(control.import(vec![], None), Err(TaskError::Busy)));
    paused.gate.release();
    let failed = phase(&control, TaskPhase::Ready);
    assert!(failed.error.is_some());
    assert!(Arc::ptr_eq(
        ready.import.as_ref().unwrap(),
        failed.import.as_ref().unwrap()
    ));
    assert!(!dir.path().join("photo_compressed.png").exists());
    control.start(id, copy_settings()).unwrap();
    assert_eq!(
        phase(&control, TaskPhase::Finished)
            .batch
            .unwrap()
            .summary
            .succeeded,
        1
    );
}

#[test]
fn output_conflict_can_be_corrected_without_rescanning() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "photo.png", "rgb8.png");
    let occupied = dir.path().join("photo_compressed.png");
    fs::write(&occupied, b"keep").unwrap();
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    let id = control.import(vec![source], Some(copy_settings())).unwrap();
    let ready = phase(&control, TaskPhase::Ready);
    assert!(ready.error.is_some());
    fs::remove_file(&occupied).unwrap();
    control.start(id, copy_settings()).unwrap();
    let finished = phase(&control, TaskPhase::Finished);
    assert!(Arc::ptr_eq(
        ready.import.as_ref().unwrap(),
        finished.import.as_ref().unwrap()
    ));
    assert!(finished.error.is_none());
}

#[test]
fn empty_and_limited_imports_do_not_start_partial_batches() {
    let dir = tempfile::tempdir().unwrap();
    let a = sample(dir.path(), "a.png", "rgb8.png");
    let b = sample(dir.path(), "b.png", "rgba8.png");
    let runtime = TaskRuntime::new(TaskConfig {
        scan: ScanOptions {
            max_files: 1,
            ..ScanOptions::default()
        },
        ..TaskConfig::default()
    })
    .unwrap();
    let control = runtime.control();
    let first = control.import(vec![a, b], Some(copy_settings())).unwrap();
    let rejected = phase(&control, TaskPhase::Rejected);
    assert_eq!(rejected.import.unwrap().files().len(), 1);
    assert!(rejected.batch.is_none());
    assert!(matches!(
        control.start(first, copy_settings()),
        Err(TaskError::NotReady)
    ));
    let second = control.import(vec![], Some(copy_settings())).unwrap();
    assert_ne!(first, second);
    assert!(phase(&control, TaskPhase::Rejected).batch.is_none());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    let revision = control.snapshot().revision;
    assert!(matches!(
        control.import(vec![PathBuf::new(); 1001], None),
        Err(TaskError::TooManyRoots)
    ));
    assert_eq!(control.snapshot().revision, revision);
}

#[test]
fn scan_cancel_keeps_partial_feedback_and_timeout_does_not_release_gate() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "photo.png", "rgb8.png");
    let paused = Paused::new(Point::Candidate);
    let control = paused.runtime.control();
    let id = control
        .import(vec![source.clone()], Some(copy_settings()))
        .unwrap();
    paused.entered();
    assert!(matches!(control.import(vec![], None), Err(TaskError::Busy)));
    assert!(matches!(control.clear(id), Err(TaskError::Busy)));
    control.cancel(id).unwrap();
    let cancelling = control.snapshot();
    assert_eq!(cancelling.phase, TaskPhase::Cancelling);
    control.cancel(id).unwrap();
    assert_eq!(control.snapshot().revision, cancelling.revision);
    assert!(matches!(
        control.wait_for_change(cancelling.revision, Duration::ZERO),
        Err(TaskError::TimedOut)
    ));
    paused.gate.release();
    let cancelled = phase(&control, TaskPhase::Cancelled);
    assert_eq!(cancelled.import.unwrap().files().len(), 1);
    assert!(cancelled.batch.is_none());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    control.clear(id).unwrap();
    phase(&control, TaskPhase::Idle);
    let next = control.import(vec![source], None).unwrap();
    assert_ne!(id, next);
    assert!(matches!(control.cancel(id), Err(TaskError::StaleSelection)));
    assert_eq!(phase(&control, TaskPhase::Ready).selection, Some(next));
}

#[test]
fn cancelling_or_closing_during_planning_never_admits_output() {
    for close in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let source = sample(dir.path(), "photo.png", "rgb8.png");
        let mut paused = Paused::new(Point::Plan);
        let control = paused.runtime.control();
        let id = control
            .import(vec![source.clone()], Some(copy_settings()))
            .unwrap();
        paused.entered();
        if close {
            control.request_close();
        } else {
            control.cancel(id).unwrap();
        }
        assert!(matches!(
            control.snapshot().phase,
            TaskPhase::Closing | TaskPhase::Cancelling
        ));
        paused.gate.release();
        if close {
            paused.runtime.shutdown().unwrap();
            assert_eq!(control.snapshot().phase, TaskPhase::Closed);
        } else {
            phase(&control, TaskPhase::Cancelled);
        }
        assert!(control.snapshot().batch.is_none());
        assert_eq!(fs::read(source).unwrap(), fixture("rgb8.png"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

#[test]
fn close_while_scanning_blocks_all_handles_and_waits_for_real_return() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "photo.png", "rgb8.png");
    let mut paused = Paused::new(Point::Scan);
    let control = paused.runtime.control();
    control.import(vec![source], Some(copy_settings())).unwrap();
    paused.entered();
    control.request_close();
    let clone = paused.runtime.control();
    assert!(matches!(clone.import(vec![], None), Err(TaskError::Closed)));
    assert_eq!(clone.snapshot().phase, TaskPhase::Closing);
    paused.gate.release();
    paused.runtime.shutdown().unwrap();
    paused.runtime.shutdown().unwrap();
    assert_eq!(clone.snapshot().phase, TaskPhase::Closed);
    assert!(clone.snapshot().batch.is_none());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn real_retry_preserves_success_backups_and_rejects_stale_batch_revision() {
    let dir = tempfile::tempdir().unwrap();
    let a = sample(dir.path(), "a.png", "rgb8.png");
    let b = sample(dir.path(), "b.png", "bad-deflate.png");
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    let id = control
        .import(
            vec![a.clone(), b.clone()],
            Some(TaskSettings {
                parameters: BatchParameters::default(),
                output: ImportOutput::Overwrite,
            }),
        )
        .unwrap();
    let finished = phase(&control, TaskPhase::Finished);
    let batch = finished.batch.unwrap();
    assert_eq!((batch.summary.succeeded, batch.summary.failed), (1, 1));
    let JobState::Succeeded(report) = &batch.jobs[0].state else {
        panic!("a应成功")
    };
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = &report.outcome
    else {
        panic!("应保留备份")
    };
    let before = fs::read(&a).unwrap();
    fs::write(&b, fixture("rgba8.png")).unwrap();
    let retry = RetryRequest {
        parameters: BatchParameters::default(),
        jobs: vec![RetryJob {
            id: batch.jobs[1].id,
            metadata: Default::default(),
            output: OutputPolicy::Overwrite,
        }],
    };
    assert!(matches!(
        control.retry(id, batch.revision - 1, retry.clone()),
        Err(TaskError::StaleBatch)
    ));
    control.retry(id, batch.revision, retry.clone()).unwrap();
    let second = phase(&control, TaskPhase::Finished);
    let again = second.batch.as_ref().unwrap();
    assert_eq!(again.summary.succeeded, 2);
    assert_eq!((again.jobs[0].attempt, again.jobs[1].attempt), (1, 2));
    assert!(matches!(
        control.retry(id, batch.revision, retry),
        Err(TaskError::StaleBatch)
    ));
    assert_eq!(fs::read(a).unwrap(), before);
    assert_eq!(fs::read(backup).unwrap(), fixture("rgb8.png"));
    let retained = control.clear(id).unwrap();
    phase(&control, TaskPhase::Idle);
    assert_eq!(retained.batch.unwrap().summary.succeeded, 2);
    assert!(backup.exists());
    assert!(control.snapshot().batch.is_none());
}

#[test]
fn invalid_retry_does_not_change_attempt_or_discard_results() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "photo.png", "rgb8.png");
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    let id = control.import(vec![source], Some(copy_settings())).unwrap();
    let first = phase(&control, TaskPhase::Finished).batch.unwrap();
    control
        .retry(
            id,
            first.revision,
            RetryRequest {
                parameters: BatchParameters::default(),
                jobs: vec![RetryJob {
                    id: first.jobs[0].id,
                    metadata: Default::default(),
                    output: first.jobs[0].request.output.clone(),
                }],
            },
        )
        .unwrap();
    let next = phase(&control, TaskPhase::Finished);
    assert!(
        matches!(next.error, Some(TaskError::Batch(e)) if matches!(e.as_ref(), BatchError::InvalidRetry))
    );
    assert_eq!(next.batch.unwrap().revision, first.revision);
}

#[test]
fn dropping_control_does_not_stop_service_but_dropping_owner_does() {
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    drop(control.clone());
    assert_eq!(runtime.control().snapshot().phase, TaskPhase::Idle);
    drop(runtime);
    assert_eq!(control.snapshot().phase, TaskPhase::Closed);
    assert!(matches!(
        control.import(vec![], None),
        Err(TaskError::Closed)
    ));
}

#[test]
fn ready_cancel_cannot_restart_and_import_ids_never_wrap() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample(dir.path(), "photo.png", "rgb8.png");
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    let id = control.import(vec![source], None).unwrap();
    phase(&control, TaskPhase::Ready);
    control.cancel(id).unwrap();
    assert_eq!(control.snapshot().phase, TaskPhase::Cancelled);
    assert!(matches!(
        control.start(id, copy_settings()),
        Err(TaskError::NotReady)
    ));
    let revision = control.snapshot().revision;
    control.shared.lock().next_id = u64::MAX;
    assert!(matches!(
        control.import(vec![], None),
        Err(TaskError::IdExhausted)
    ));
    assert_eq!(control.snapshot().revision, revision);
    assert_eq!(control.snapshot().selection, Some(id));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

struct PanickingImport;
impl worker::ImportBackend for PanickingImport {
    fn scan(
        &self,
        _: &[PathBuf],
        _: ScanOptions,
        _: &CancellationToken,
        _: &mut dyn FnMut(&ScanProgress),
    ) -> Result<ImportScan, ImportError> {
        panic!("确定性导入异常")
    }
    fn plan(
        &self,
        scan: &ImportScan,
        settings: &TaskSettings,
    ) -> Result<pixofold_core::batch::BatchRequest, ImportError> {
        worker::ImportBackend::plan(&worker::NativeImport, scan, settings)
    }
}

#[test]
fn coordinator_panic_closes_and_joins_instead_of_leaving_busy_state() {
    let mut runtime =
        TaskRuntime::with_backend(TaskConfig::default(), Arc::new(PanickingImport)).unwrap();
    let control = runtime.control();
    control.import(vec![], None).unwrap();
    phase(&control, TaskPhase::Closed);
    assert!(matches!(runtime.shutdown(), Err(TaskError::WorkerPanicked)));
    assert!(matches!(
        control.snapshot().error,
        Some(TaskError::WorkerPanicked)
    ));
}

#[test]
fn poisoned_state_closes_safely_and_reports_failure() {
    let mut runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let control = runtime.control();
    let shared = control.shared.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = shared.state.lock().unwrap();
            panic!("确定性状态锁故障");
        })
        .join()
        .is_err()
    );
    assert!(matches!(
        control.import(vec![], None),
        Err(TaskError::ServiceFault)
    ));
    assert!(matches!(runtime.shutdown(), Err(TaskError::ServiceFault)));
    assert_eq!(control.snapshot().phase, TaskPhase::Closed);
}
