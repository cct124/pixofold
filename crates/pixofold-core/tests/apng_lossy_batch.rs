//! APNG批量有损：真实共享色表、冻结参数/重试与按行资源失败。
#[path = "support/apng_lossy.rs"]
mod sample;
use pixofold_core::{apng::*, batch::*, model::*};
use std::{fs, path::Path, time::Duration};
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/apng")
            .join(name),
    )
    .unwrap()
}
fn item(root: &Path, name: &str, original: &[u8], format: ImageKind) -> BatchItem {
    let source = root.join(name);
    fs::write(&source, original).unwrap();
    BatchItem {
        source,
        format,
        output: OutputPolicy::Copy {
            destination: root.join("out").join(name),
        },
    }
}
#[test]
fn five_qualities_preserve_actual_paths_and_lossless_retry_keeps_the_target() {
    let original = sample::animation();
    for q in [0, 50, 80, 95, 100] {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("out")).unwrap();
        let engines = ImageEngines::default().add_apng();
        let mut service = BatchService::with_engines(
            BatchConfig {
                workers: 2,
                ..Default::default()
            },
            engines.clone(),
        )
        .unwrap();
        let small = fixture("default-in.png");
        let deep = fixture("rgba16.apng");
        let items = vec![
            item(dir.path(), "palette.APNG", &original, ImageKind::Apng),
            item(dir.path(), "small.png", &small, ImageKind::Apng),
            item(dir.path(), "deep.apng", &deep, ImageKind::Apng),
            item(
                dir.path(),
                "static.png",
                include_bytes!("../../../tests/fixtures/png/rgb8.png"),
                ImageKind::Png,
            ),
        ];
        let quality = QualityValue::new(q).unwrap();
        let mode = PngMode::Lossy { quality };
        let id = service
            .start(BatchRequest {
                items,
                engines,
                parameters: BatchParameters {
                    mode,
                    ..Default::default()
                },
            })
            .unwrap();
        let done = service.wait(id, Duration::from_secs(30)).unwrap();
        assert_eq!(done.summary.failed, usize::from(q < 100));
        assert_eq!((done.active_workers, done.reserved_working_bytes.0), (0, 0));
        for job in &done.jobs[..3] {
            assert_eq!(job.request.mode, mode);
            assert_eq!(
                job.mapping,
                Some(QualityMapping::Apng(ApngQualityMapping::new(quality)))
            );
            assert!(job.content_credentials_source().is_none());
        }
        let JobState::Succeeded(ImageReport::Apng(report)) = &done.jobs[0].state else {
            panic!("{done:?}");
        };
        assert_eq!(report.requested_mode, ApngMode::Lossy { quality });
        if q == 80 {
            assert!(matches!(report.processing, ApngProcessing::Lossy { .. }));
        }
        if q == 100 {
            assert_eq!(report.processing, ApngProcessing::Lossless);
        }
        assert_eq!(
            report.quality.is_some(),
            matches!(report.processing, ApngProcessing::Lossy { .. })
        );
        assert_eq!(fs::read(&done.jobs[0].request.source).unwrap(), original);
        assert!(!dir.path().join("out/small.png").exists());
        assert_eq!(fs::read(&done.jobs[1].request.source).unwrap(), small);
        if q < 100 {
            let failed = &done.jobs[2];
            assert!(
                matches!(&failed.state, JobState::Failed(e) if e.code == JobErrorCode::UnsupportedApngMode)
            );
            let output = failed.request.output.clone();
            service
                .retry(
                    id,
                    RetryRequest {
                        jobs: vec![RetryJob {
                            id: failed.id,
                            output: output.clone(),
                            metadata: PngMetadataPolicy::Preserve,
                        }],
                        parameters: BatchParameters::default(),
                    },
                )
                .unwrap();
            let retried = service.wait(id, Duration::from_secs(30)).unwrap();
            let row = &retried.jobs[2];
            assert_eq!(row.id, failed.id);
            assert_eq!(row.attempt, 2);
            match (&row.request.output, &output) {
                (
                    OutputPolicy::Copy {
                        destination: actual,
                    },
                    OutputPolicy::Copy {
                        destination: expected,
                    },
                ) => assert_eq!(actual, expected),
                _ => panic!("copy target changed"),
            }
            assert_eq!(row.mapping, None);
            let JobState::Succeeded(ImageReport::Apng(result)) = &row.state else {
                panic!("{row:?}");
            };
            assert_eq!(result.requested_mode, ApngMode::Lossless);
            assert_eq!(result.processing, ApngProcessing::Lossless);
            assert_eq!(retried.jobs[0].attempt, 1);
        }
        assert_eq!(fs::read(dir.path().join("deep.apng")).unwrap(), deep);
        service.shutdown().unwrap();
    }
}
#[test]
fn quantization_resource_failure_is_isolated_and_unfit_batch_does_not_wait() {
    for tiny_pool in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("out")).unwrap();
        let original = sample::animation();
        let engines = ImageEngines::default().add_apng();
        let mut service = BatchService::with_engines(
            BatchConfig {
                working_set_budget: ByteCount(if tiny_pool { 1 } else { 512 << 20 }),
                ..Default::default()
            },
            engines.clone(),
        )
        .unwrap();
        let mut parameters = BatchParameters {
            mode: PngMode::Lossy {
                quality: QualityValue::new(80).unwrap(),
            },
            ..Default::default()
        };
        parameters.apng.limits.max_palette_searches = 0;
        let items = vec![
            item(dir.path(), "limited.apng", &original, ImageKind::Apng),
            item(
                dir.path(),
                "static.png",
                include_bytes!("../../../tests/fixtures/png/rgb8.png"),
                ImageKind::Png,
            ),
        ];
        let id = service
            .start(BatchRequest {
                items,
                parameters,
                engines,
            })
            .unwrap();
        let done = service.wait(id, Duration::from_secs(30)).unwrap();
        assert!(
            matches!(&done.jobs[0].state, JobState::Failed(e) if e.code == JobErrorCode::ResourceLimit)
        );
        assert_eq!(done.summary.failed, if tiny_pool { 2 } else { 1 });
        assert_eq!((done.active_workers, done.reserved_working_bytes.0), (0, 0));
        assert_eq!(fs::read(dir.path().join("limited.apng")).unwrap(), original);
        assert!(!dir.path().join("out/limited.apng").exists());
        service.shutdown().unwrap();
    }
}
