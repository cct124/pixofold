//! 可控混合执行器验证共用调度和预算；真实helper/输出闭环由jpeg:core:check覆盖。
use super::*;
use crate::{jpeg::*, model::*};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, sync::mpsc};

const WAIT: Duration = Duration::from_secs(10);

struct GateRunner {
    entered: mpsc::Sender<ImageRequest>,
    permits: Mutex<usize>,
    changed: Condvar,
    actual: Option<PipelineRunner>,
}
impl GateRunner {
    fn release(&self, n: usize) {
        *self.permits.lock().unwrap() += n;
        self.changed.notify_all();
    }
}
impl Runner for GateRunner {
    fn run(
        &self,
        request: &ImageRequest,
        cancel: &CancellationToken,
        stage: &mut dyn FnMut(ProcessingStage),
    ) -> Result<ImageReport, ImageError> {
        self.entered.send(request.clone()).unwrap();
        let mut permits = self.permits.lock().unwrap();
        while *permits == 0 {
            permits = self.changed.wait(permits).unwrap();
        }
        *permits -= 1;
        drop(permits);
        if let Some(actual) = &self.actual {
            return actual.run(request, cancel, stage);
        }
        if cancel.is_cancelled() {
            return Err(match request.format() {
                ImageKind::Png => ProcessingError::Cancelled.into(),
                ImageKind::Jpeg => JpegError::Cancelled.into(),
            });
        }
        let bytes = ByteCount(fs::metadata(&request.source).unwrap().len());
        Ok(match request.format() {
            ImageKind::Jpeg => ImageReport::Jpeg(JpegReport {
                image: JpegInfo {
                    width: 16,
                    height: 16,
                    components: 3,
                    progressive: false,
                },
                processing: JpegProcessing::Lossless,
                input_bytes: bytes,
                output_bytes: bytes,
                elapsed: Duration::ZERO,
                outcome: ProcessingOutcome::NoGain,
            }),
            ImageKind::Png => {
                let image = ImageInfo {
                    width: 1,
                    height: 1,
                    bit_depth: 8,
                    color_type: PngColorType::Rgb,
                    interlaced: false,
                };
                ImageReport::Png(ProcessingReport {
                    image: image.clone(),
                    output_image: image,
                    processing: PngProcessing::Lossless,
                    content_credentials_removed: false,
                    input_bytes: bytes,
                    output_bytes: bytes,
                    elapsed: Duration::ZERO,
                    outcome: ProcessingOutcome::NoGain,
                })
            }
        })
    }
}
struct Harness {
    service: BatchService,
    runner: Arc<GateRunner>,
    entered: mpsc::Receiver<ImageRequest>,
    dir: tempfile::TempDir,
}
impl Harness {
    fn new(budget: u64, actual: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // 仅作为受信身份载体，从不执行此文件；actual用例在结构/预算检查时即返回。
        let name = if cfg!(windows) {
            "pixofold-jpeg-helper.exe"
        } else {
            "pixofold-jpeg-helper"
        };
        let bytes = b"identity fixture; never executed";
        fs::write(dir.path().join(name), bytes).unwrap();
        let engines = ImageEngines::with_jpeg(
            JpegEngine::load(dir.path(), Sha256::digest(bytes).into()).unwrap(),
        );
        let (send, entered) = mpsc::channel();
        let runner = Arc::new(GateRunner {
            entered: send,
            permits: Mutex::new(0),
            changed: Condvar::new(),
            actual: actual.then(|| PipelineRunner(engines.clone())),
        });
        let mut service = BatchService::with_runner(
            BatchConfig {
                workers: 2,
                working_set_budget: ByteCount(budget),
                ..BatchConfig::default()
            },
            runner.clone(),
        )
        .unwrap();
        service.engines = engines;
        Self {
            service,
            runner,
            entered,
            dir,
        }
    }
    fn request(&self, formats: &[ImageKind]) -> BatchRequest {
        let items = formats
            .iter()
            .enumerate()
            .map(|(i, &format)| {
                let source = self.dir.path().join(format!(
                    "{i}.{}",
                    if format == ImageKind::Png {
                        "png"
                    } else {
                        "jpg"
                    }
                ));
                let bytes = if format == ImageKind::Jpeg {
                    header(16)
                } else {
                    fs::read(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../tests/fixtures/png/rgb8.png"),
                    )
                    .unwrap()
                };
                fs::write(&source, bytes).unwrap();
                BatchItem {
                    source,
                    output: OutputPolicy::Overwrite,
                    format,
                }
            })
            .collect();
        BatchRequest {
            items,
            parameters: BatchParameters::default(),
            engines: self.service.engines.clone(),
        }
    }
    fn entered(&self) -> ImageRequest {
        self.entered.recv_timeout(WAIT).unwrap()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.runner.release(100);
    }
}

fn header(size: u16) -> Vec<u8> {
    let mut b = vec![0xff, 0xd8, 0xff, 0xc0, 0, 17, 8];
    b.extend(size.to_be_bytes());
    b.extend(size.to_be_bytes());
    b.extend([
        3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0, 0xff, 0xda, 0, 12, 3, 1, 0, 2, 0, 3, 0, 0, 63, 0,
        0xff, 0xd9,
    ]);
    b
}

#[test]
fn mixed_jobs_share_workers_and_cancellation_retains_reservations_until_return() {
    let h = Harness::new(256 * 1024 * 1024, false);
    let id = h
        .service
        .start(h.request(&[ImageKind::Png, ImageKind::Jpeg, ImageKind::Png]))
        .unwrap();
    let a = h.entered();
    let b = h.entered();
    assert_ne!(a.format(), b.format());
    let active = h.service.snapshot().unwrap();
    assert_eq!((active.active_workers, active.summary.queued), (2, 1));
    h.service.cancel(id).unwrap();
    let cancelling = h.service.snapshot().unwrap();
    assert_eq!(cancelling.phase, BatchPhase::Cancelling);
    assert_eq!(
        cancelling.reserved_working_bytes,
        active.reserved_working_bytes
    );
    h.runner.release(2);
    let done = h.service.wait(id, WAIT).unwrap();
    assert_eq!(done.summary.cancelled, 3);
    assert_eq!((done.active_workers, done.reserved_working_bytes.0), (0, 0));
}

#[test]
fn tight_budget_serializes_formats_and_unfit_rows_finish_without_waiting() {
    let h = Harness::new(64 * 1024 * 1024, false);
    let id = h
        .service
        .start(h.request(&[ImageKind::Png, ImageKind::Jpeg]))
        .unwrap();
    assert_eq!(h.entered().format(), ImageKind::Png);
    assert_eq!(h.service.snapshot().unwrap().active_workers, 1);
    assert!(h.entered.try_recv().is_err());
    h.runner.release(1);
    assert_eq!(h.entered().format(), ImageKind::Jpeg);
    h.runner.release(1);
    assert_eq!(h.service.wait(id, WAIT).unwrap().summary.no_gain, 2);
    let small = Harness::new(1, false);
    let id = small
        .service
        .start(small.request(&[ImageKind::Png, ImageKind::Jpeg]))
        .unwrap();
    let done = small.service.wait(id, WAIT).unwrap();
    assert_eq!(done.phase, BatchPhase::Finished);
    assert_eq!(done.summary.failed, 2);
    assert!(
        done.jobs.iter().all(
            |j| matches!(&j.state, JobState::Failed(f) if f.code == JobErrorCode::ResourceLimit)
        )
    );
    assert!(small.entered.try_recv().is_err());
}

#[test]
fn queued_jpeg_growth_and_format_replacement_are_rejected_by_actual_execution_limits() {
    for replace_format in [false, true] {
        let h = Harness::new(256 * 1024 * 1024, true);
        let id = h.service.start(h.request(&[ImageKind::Jpeg])).unwrap();
        let executing = h.entered();
        assert_eq!(executing.limits.max_pixels, 256);
        let changed = if replace_format {
            fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/png/rgb8.png"),
            )
            .unwrap()
        } else {
            header(32)
        };
        fs::write(&executing.source, &changed).unwrap();
        h.runner.release(1);
        let done = h.service.wait(id, WAIT).unwrap();
        let JobState::Failed(f) = &done.jobs[0].state else {
            panic!("changed source must fail")
        };
        assert_eq!(
            f.code,
            if replace_format {
                JobErrorCode::InvalidInput
            } else {
                JobErrorCode::ResourceLimit
            }
        );
        assert_eq!(fs::read(&executing.source).unwrap(), changed);
        assert_eq!(done.reserved_working_bytes.0, 0);
    }
}

#[test]
fn jpeg_errors_keep_their_domain_and_never_grant_png_metadata_consent() {
    let h = Harness::new(256 * 1024 * 1024, false);
    let mut request = h.request(&[ImageKind::Jpeg]);
    request.engines = ImageEngines::default();
    let id = h.service.start(request).unwrap();
    let done = h.service.wait(id, WAIT).unwrap();
    assert_eq!(done.summary.failed, 1);
    let job = &done.jobs[0];
    assert!(matches!(&job.state, JobState::Failed(f) if f.code == JobErrorCode::ToolIdentity));
    assert!(job.content_credentials_source().is_none());
    let protected = JobFailure::image(JpegError::ProtectedMetadata(0xeb).into());
    assert_eq!(protected.code, JobErrorCode::UnsupportedContentCredentials);
    let mut protected_job = job.clone();
    protected_job.state = JobState::Failed(protected);
    assert!(protected_job.content_credentials_source().is_none());
    for (e, code) in [
        (JpegError::Timeout, JobErrorCode::TimedOut),
        (JpegError::ToolExit(Some(3)), JobErrorCode::ToolExit),
    ] {
        let f = JobFailure::image(e.into());
        assert_eq!(f.code, code);
        assert!(matches!(f.cause.as_deref(), Some(ImageError::Jpeg(_))));
    }
}

struct ProcessRunner {
    ready: std::path::PathBuf,
    timeout: Duration,
    events: mpsc::Sender<crate::jpeg::ProcessEvent>,
    hold_reaped: bool,
}

#[test]
fn header_refinement_cannot_sanitize_invalid_jpeg_configuration() {
    let h = Harness::new(256 * 1024 * 1024, false);
    let mut request = h.request(&[ImageKind::Png, ImageKind::Jpeg]);
    request.parameters.limits.max_input_bytes = ByteCount(64 * 1024 * 1024 + 1);
    let id = h.service.start(request).unwrap();
    assert_eq!(h.entered().format(), ImageKind::Png);
    h.runner.release(1);
    let done = h.service.wait(id, WAIT).unwrap();
    assert_eq!((done.summary.no_gain, done.summary.failed), (1, 1));
    assert!(
        matches!(&done.jobs[1].state, JobState::Failed(f) if f.code == JobErrorCode::InvalidInput)
    );
    assert!(h.entered.try_recv().is_err());
}
impl Runner for ProcessRunner {
    fn run(
        &self,
        _: &ImageRequest,
        cancel: &CancellationToken,
        _: &mut dyn FnMut(ProcessingStage),
    ) -> Result<ImageReport, ImageError> {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "jpeg::process::tests::child_fixture",
                "--nocapture",
            ])
            .env("PIXOFOLD_PROCESS_FIXTURE", "hang")
            .env("PIXOFOLD_PROCESS_READY", &self.ready);
        let error = crate::jpeg::run_process_for_test(
            &mut command,
            &vec![0; 1024 * 1024],
            4096,
            false,
            self.timeout,
            cancel,
            crate::jpeg::ProcessObserver {
                events: self.events.clone(),
                hold_reaped: self.hold_reaped,
            },
        )
        .expect_err("controlled child must stop");
        assert!(
            !command.get_current_dir().unwrap().exists(),
            "process workspace must already be reclaimed"
        );
        Err(error.into())
    }
}

#[test]
fn real_jpeg_process_is_reaped_before_cancel_timeout_and_shutdown_release_the_budget() {
    for mode in ["cancel", "timeout", "shutdown"] {
        let h = Harness::new(256 * 1024 * 1024, false);
        let ready = h.dir.path().join("child-ready");
        let (events, exit_events) = mpsc::channel();
        let runner = Arc::new(ProcessRunner {
            ready: ready.clone(),
            events,
            hold_reaped: mode != "shutdown",
            timeout: if mode == "timeout" {
                Duration::from_secs(2)
            } else {
                WAIT
            },
        });
        let mut service = BatchService::with_runner(BatchConfig::default(), runner).unwrap();
        // panic时先丢弃观测接收者/门闩许可，再由service的Drop执行取消和join。
        let observed = exit_events;
        service.engines = h.service.engines.clone();
        let id = service.start(h.request(&[ImageKind::Jpeg])).unwrap();
        let start = Instant::now();
        let crate::jpeg::ProcessEvent::Spawned { pid, workspace } =
            observed.recv_timeout(WAIT).unwrap()
        else {
            panic!("{mode}: child must report its identity before exit");
        };
        // 身份取自实际Child，完整ready帧只确认夹具已进入阻塞路径。
        loop {
            if fs::read_to_string(&ready).is_ok_and(|text| text == format!("ready:{pid}\n")) {
                break;
            }
            if start.elapsed() > WAIT {
                panic!("{mode}: child {pid} never became ready");
            }
            std::thread::yield_now();
        }
        let active = service.snapshot().unwrap();
        assert_eq!(active.active_workers, 1);
        assert!(active.reserved_working_bytes.0 > 0);
        if mode == "cancel" {
            service.cancel(id).unwrap();
        }
        if mode == "shutdown" {
            service.shutdown().unwrap();
        }
        let exited = if mode == "shutdown" {
            // shutdown已返回，退出证据必须已经存在，不能等候迟到回收。
            observed.try_recv().unwrap_or_else(|error| {
                panic!("{mode}: child {pid} missing exit before shutdown: {error}")
            })
        } else {
            observed
                .recv_timeout(WAIT)
                .unwrap_or_else(|error| panic!("{mode}: child {pid} did not reap: {error}"))
        };
        let crate::jpeg::ProcessEvent::Reaped {
            pid: reaped_pid,
            status,
            resume,
        } = exited
        else {
            panic!("{mode}: unexpected event after child {pid} started");
        };
        assert_eq!(
            reaped_pid, pid,
            "{mode}: exit must belong to the original child"
        );
        assert!(
            status.is_ok(),
            "{mode}: child {pid} wait failed: {status:?}"
        );
        if let Some(resume) = resume {
            // 原生wait已完成，但目录尚未清理：worker和预算必须继续保留。
            let held = service.snapshot().unwrap();
            assert_eq!(held.active_workers, 1, "{mode}");
            assert!(held.reserved_working_bytes.0 > 0, "{mode}");
            assert!(
                workspace.exists(),
                "{mode}: cleanup must follow the reap gate"
            );
            assert!(matches!(
                service.wait(id, Duration::ZERO),
                Err(BatchError::TimedOut)
            ));
            resume.send(()).unwrap();
        } else {
            assert_eq!(mode, "shutdown");
        }
        let done = service.wait(id, WAIT).unwrap();
        if mode == "timeout" {
            assert!(
                matches!(&done.jobs[0].state,JobState::Failed(f) if f.code==JobErrorCode::TimedOut)
            );
        } else {
            assert!(matches!(&done.jobs[0].state, JobState::Cancelled));
        }
        assert_eq!((done.active_workers, done.reserved_working_bytes.0), (0, 0));
        assert!(
            !workspace.exists(),
            "{mode}: terminal row must have cleaned its workspace"
        );
        assert!(
            observed.try_recv().is_err(),
            "{mode}: duplicate process event"
        );
        service.shutdown().unwrap();
    }
}
