//! 在真实pipeline阶段放置可释放门闩，证明整图并行及预约上限生效；不靠固定休眠。
use super::*;
use crate::model::{
    OutputPolicy, PngMetadataPolicy, PngMode, PngRequest, ProcessingOutcome, QualityValue,
};
use std::{fs, path::Path, sync::mpsc, thread::ThreadId};

const WAIT: Duration = Duration::from_secs(10);

struct GatedPipeline {
    stage: ProcessingStage,
    open: Mutex<bool>,
    changed: Condvar,
    entered: mpsc::Sender<(ThreadId, PngRequest)>,
}
impl GatedPipeline {
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
impl Runner for GatedPipeline {
    fn run(
        &self,
        request: &ImageRequest,
        cancel: &CancellationToken,
        stage: &mut dyn FnMut(ProcessingStage),
    ) -> Result<ImageReport, ImageError> {
        let request = request.png().unwrap();
        crate::pipeline::optimize_png(&request, cancel, |next| {
            stage(next);
            if next == self.stage {
                self.entered
                    .send((std::thread::current().id(), request.clone()))
                    .unwrap();
                let mut open = self.open.lock().unwrap();
                while !*open {
                    open = self.changed.wait(open).unwrap();
                }
            }
        })
        .map(ImageReport::Png)
        .map_err(ImageError::Png)
    }
}
struct Harness {
    pipeline: Arc<GatedPipeline>,
    service: BatchService,
    entered: mpsc::Receiver<(ThreadId, PngRequest)>,
    directory: tempfile::TempDir,
}
impl Harness {
    fn new(workers: usize, budget: ByteCount, stage: ProcessingStage) -> Self {
        let (send, entered) = mpsc::channel();
        let pipeline = Arc::new(GatedPipeline {
            stage,
            open: Mutex::new(false),
            changed: Condvar::new(),
            entered: send,
        });
        let service = BatchService::with_runner(
            BatchConfig {
                workers,
                working_set_budget: budget,
                ..BatchConfig::default()
            },
            pipeline.clone(),
        )
        .unwrap();
        Self {
            pipeline,
            service,
            entered,
            directory: tempfile::tempdir().unwrap(),
        }
    }
    fn request(&self, bytes: &[u8], count: usize) -> BatchRequest {
        let items = (0..count)
            .map(|i| {
                let source = self.directory.path().join(format!("input-{i}.png"));
                fs::write(&source, bytes).unwrap();
                BatchItem {
                    format: crate::batch::ImageKind::Png,
                    source,
                    output: OutputPolicy::Copy {
                        destination: self.directory.path().join(format!("result-{i}.png")),
                    },
                }
            })
            .collect();
        BatchRequest {
            engines: Default::default(),
            items,
            parameters: BatchParameters {
                mode: PngMode::Lossy {
                    quality: QualityValue::new(68).unwrap(),
                },
                ..BatchParameters::default()
            },
        }
    }
    fn entered(&self) -> (ThreadId, PngRequest) {
        self.entered
            .recv_timeout(WAIT)
            .expect("pipeline应进入受控阶段")
    }
    fn finish(&self, id: BatchId) -> BatchSnapshot {
        self.pipeline.release();
        self.service.wait(id, WAIT).unwrap()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.pipeline.release();
    }
}
fn fixture() -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/png/gradient-rgb8.png"),
    )
    .unwrap()
}
fn credentials() -> Vec<u8> {
    let bytes = fixture();
    let payload = b"synthetic credentials";
    let mut chunk = (payload.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(b"caBX");
    chunk.extend_from_slice(payload);
    chunk.extend_from_slice(&crc32fast::hash(&chunk[4..]).to_be_bytes());
    [bytes[..33].to_vec(), chunk, bytes[33..].to_vec()].concat()
}

#[test]
fn real_small_images_overlap_without_changing_public_batch_limits() {
    let h = Harness::new(2, ByteCount(128 * 1024 * 1024), ProcessingStage::Optimizing);
    let original = fixture();
    let request = h.request(&original, 3);
    assert!(estimate_working_set(request.parameters).unwrap().0 > 128 * 1024 * 1024);
    let id = h.service.start(request.clone()).unwrap();
    let first = h.entered();
    let second = h.entered();
    assert_ne!(first.0, second.0);
    assert!(first.1.limits.max_pixels < request.parameters.limits.max_pixels);
    let active = h.service.snapshot().unwrap();
    assert_eq!((active.active_workers, active.summary.queued), (2, 1));
    assert!(active.reserved_working_bytes.0 <= 128 * 1024 * 1024);
    assert_eq!(
        active.jobs[0].request.limits.max_pixels,
        request.parameters.limits.max_pixels
    );
    let done = h.finish(id);
    assert_eq!(done.summary.succeeded, 3);
    for item in request.items {
        assert_eq!(fs::read(item.source).unwrap(), original);
    }
    assert_eq!((done.active_workers, done.reserved_working_bytes.0), (0, 0));
}

#[test]
fn confirmed_credentials_use_two_whole_file_workers_for_all_output_policies() {
    for output_kind in 0..3 {
        let h = Harness::new(2, ByteCount(128 * 1024 * 1024), ProcessingStage::Optimizing);
        let original = credentials();
        let request = h.request(&original, 2);
        let id = h.service.start(request.clone()).unwrap();
        let first = h.service.wait(id, WAIT).unwrap();
        assert_eq!(first.summary.failed, 2);
        assert!(h.entered.try_recv().is_err()); // 未同意时不得进入编码或改写。
        let jobs = first
            .jobs
            .iter()
            .map(|job| RetryJob {
                id: job.id,
                output: match output_kind {
                    0 => job.request.output.clone(),
                    1 => OutputPolicy::Overwrite,
                    _ => OutputPolicy::OverwriteWithoutBackup,
                },
                metadata: PngMetadataPolicy::RemoveContentCredentials(
                    job.content_credentials_source().unwrap().clone(),
                ),
            })
            .collect();
        h.service
            .retry(
                id,
                RetryRequest {
                    jobs,
                    parameters: request.parameters,
                },
            )
            .unwrap();
        let one = h.entered();
        let two = h.entered();
        assert_ne!(one.0, two.0);
        assert_eq!(h.service.snapshot().unwrap().active_workers, 2);
        // 两个worker都已在内存中移除凭据，但原图仍完整存在，没有先改写后压缩。
        for item in &request.items {
            assert_eq!(fs::read(&item.source).unwrap(), original);
        }
        let done = h.finish(id);
        assert_eq!((done.summary.succeeded, done.summary.failed), (2, 0));
        for job in &done.jobs {
            let JobState::Succeeded(report) = &job.state else {
                panic!("应压缩成功");
            };
            assert!(report.credentials_removed());
            assert_eq!(job.attempt, 2);
            let ProcessingOutcome::Optimized { output, backup } = report.outcome() else {
                panic!("应有收益");
            };
            let result = fs::read(output).unwrap();
            assert!(
                !crate::probe::chunks(&result)
                    .unwrap()
                    .iter()
                    .any(|chunk| chunk.name == *b"caBX")
            );
            crate::probe::inspect_png(&result, ResourceLimits::default()).unwrap();
            if output_kind == 1 {
                assert_eq!(fs::read(backup.as_ref().unwrap()).unwrap(), original);
            } else {
                assert!(backup.is_none());
            }
            if output_kind == 0 {
                assert_eq!(fs::read(&job.request.source).unwrap(), original);
            }
        }
        assert_eq!(
            fs::read_dir(h.directory.path()).unwrap().count(),
            if output_kind == 2 { 2 } else { 4 }
        );
        assert_eq!((done.active_workers, done.reserved_working_bytes.0), (0, 0));
    }
}

#[test]
fn source_growth_after_admission_cannot_escape_the_actual_reserved_limits() {
    for grow_bytes in [false, true] {
        let h = Harness::new(1, ByteCount(128 * 1024 * 1024), ProcessingStage::Reading);
        let mut bytes = fixture();
        let request = h.request(&bytes, 1);
        let id = h.service.start(request.clone()).unwrap();
        let (_, executing) = h.entered();
        if grow_bytes {
            bytes.resize(executing.limits.max_input_bytes.0 as usize + 1, 0);
        } else {
            let width = executing.limits.max_dimension + 1;
            bytes[16..20].copy_from_slice(&width.to_be_bytes());
            let crc = crc32fast::hash(&bytes[12..29]).to_be_bytes();
            bytes[29..33].copy_from_slice(&crc);
        }
        fs::write(&request.items[0].source, &bytes).unwrap();
        let done = h.finish(id);
        assert!(
            matches!(&done.jobs[0].state, JobState::Failed(failure) if failure.code == JobErrorCode::ResourceLimit)
        );
        assert_eq!(fs::read(&request.items[0].source).unwrap(), bytes);
        assert_eq!(fs::read_dir(h.directory.path()).unwrap().count(), 1);
        assert_eq!(done.reserved_working_bytes.0, 0);
    }
}

#[test]
fn refined_budget_still_serializes_large_jobs_and_rejects_an_unfit_job_without_waiting() {
    let seed = Harness::new(1, ByteCount(u64::MAX), ProcessingStage::Optimizing);
    let request = seed.request(&fixture(), 1);
    let mut input = PngRequest::new(&request.items[0].source);
    input.mode = request.parameters.mode;
    let limits = resources::execution_limits(
        &input,
        Some(ByteCount(fs::metadata(&input.source).unwrap().len())),
    );
    let reservation = estimate_working_set(BatchParameters {
        mode: input.mode,
        limits,
        ..BatchParameters::default()
    })
    .unwrap();
    let h = Harness::new(2, reservation, ProcessingStage::Optimizing);
    let id = h.service.start(h.request(&fixture(), 2)).unwrap();
    h.entered();
    let active = h.service.snapshot().unwrap();
    assert_eq!((active.active_workers, active.summary.queued), (1, 1));
    assert_eq!(h.finish(id).summary.succeeded, 2);
    let h = Harness::new(2, ByteCount(reservation.0 - 1), ProcessingStage::Optimizing);
    let id = h.service.start(h.request(&fixture(), 2)).unwrap();
    let done = h.service.wait(id, WAIT).unwrap();
    assert_eq!(done.summary.failed, 2);
    assert!(h.entered.try_recv().is_err());
}
