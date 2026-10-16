//! 显式真实JPEG元数据/凭据回归；由jpeg:core:check提供可信helper和隔离目录。
use pixofold_core::{
    batch::{
        BatchConfig, BatchItem, BatchParameters, BatchRequest, BatchService, ImageEngines,
        ImageKind, JobState, RetryJob, RetryRequest,
    },
    import::{ScanOptions, scan_with_engines},
    jpeg::{
        JpegEngine, JpegError, JpegLimits, JpegMode, JpegRequest, decode_preview, optimize_jpeg,
    },
    model::{
        CancellationToken, MetadataPolicy, OutputPolicy, PngMode, ProcessingError,
        ProcessingOutcome, ProcessingStage, QualityValue,
    },
};
use serde_json::json;
use std::{fs, path::Path, time::Duration};
#[path = "../tests/support/jpeg_metadata.rs"]
mod samples;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert!(
        matches!(args.len(), 3 | 4),
        "工具目录、SHA256、隔离语料目录，以及可选的公开C2PA JPEG"
    );
    let hash = args[1].to_str().unwrap();
    let digest = std::array::from_fn(|i| u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).unwrap());
    let engine = JpegEngine::load(Path::new(&args[0]), digest).unwrap();
    let fixtures = Path::new(&args[2]);
    let root = fixtures.join("metadata-results");
    fs::create_dir(&root).unwrap();
    let base = fs::read(fixtures.join("baseline-420.jpg")).unwrap();
    let [ordinary, signed, broken, unknown] = samples::samples(&base);
    let reference = root.join("reference.jpg");
    fs::write(&reference, &ordinary).unwrap();
    assert_eq!(
        decode_preview(
            &ordinary,
            &engine,
            JpegLimits::default(),
            &CancellationToken::default()
        )
        .unwrap()
        .orientation,
        6
    );
    let mut checks = Vec::new();
    for (name, bytes, needs_consent) in [("ordinary", &ordinary, false), ("c2pa", &signed, true)] {
        for mode in [
            JpegMode::Lossless,
            JpegMode::Lossy {
                quality: QualityValue::default(),
            },
        ] {
            let directory = root.join(format!(
                "{name}-{}",
                if mode == JpegMode::Lossless {
                    "lossless"
                } else {
                    "lossy"
                }
            ));
            fs::create_dir(&directory).unwrap();
            let source = directory.join("source.JpEg");
            fs::write(&source, bytes).unwrap();
            let mut request = JpegRequest::new(&source);
            request.mode = mode;
            request.output = OutputPolicy::Copy {
                destination: directory.join("result.JpEg"),
            };
            if needs_consent {
                let Err(JpegError::ContentCredentialsRequireConsent(version)) =
                    optimize_jpeg(&request, &engine, &CancellationToken::default(), |_| {})
                else {
                    panic!("未进入确认");
                };
                assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
                request.metadata = MetadataPolicy::RemoveContentCredentials(version.clone());
                fs::write(&source, &ordinary).unwrap();
                assert!(matches!(
                    optimize_jpeg(&request, &engine, &CancellationToken::default(), |_| {}),
                    Err(JpegError::File(ProcessingError::SourceChanged))
                ));
                fs::write(&source, bytes).unwrap();
                request.metadata = MetadataPolicy::Preserve;
                let Err(JpegError::ContentCredentialsRequireConsent(version)) =
                    optimize_jpeg(&request, &engine, &CancellationToken::default(), |_| {})
                else {
                    panic!("重新确认失败");
                };
                request.metadata = MetadataPolicy::RemoveContentCredentials(version);
            }
            let report =
                optimize_jpeg(&request, &engine, &CancellationToken::default(), |_| {}).unwrap();
            assert_eq!(fs::read(&source).unwrap(), *bytes);
            if let ProcessingOutcome::Optimized { output, backup } = report.outcome {
                assert!(backup.is_none());
                assert_eq!(report.content_credentials_removed, needs_consent);
                checks.push(json!({"source":source,"reference":reference,"output":output,"lossless":mode == JpegMode::Lossless,"removed":needs_consent}));
            } else {
                assert!(!report.content_credentials_removed);
            }
        }
    }
    // 已入库的公开照片覆盖真实Exif目录，保持原件不变并检查8种方向。
    for orientation in 1..=8 {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../tests/manual-images/jpeg/Landscape_{orientation}.jpg"
        ));
        let original = fs::read(&source).unwrap();
        let preview = decode_preview(
            &original,
            &engine,
            JpegLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
        assert_eq!(preview.orientation, orientation);
        let mut request = JpegRequest::new(&source);
        request.output = OutputPolicy::Copy {
            destination: root.join(format!("public-{orientation}.jpg")),
        };
        let report =
            optimize_jpeg(&request, &engine, &CancellationToken::default(), |_| {}).unwrap();
        assert!(!report.content_credentials_removed);
        assert_eq!(fs::read(&source).unwrap(), original);
        if let ProcessingOutcome::Optimized { output, .. } = report.outcome {
            checks.push(json!({"source":source,"reference":source,"output":output,"lossless":true,"removed":false}));
        }
    }
    for (name, bytes) in [("broken", &broken), ("unknown", &unknown)] {
        let source = root.join(format!("{name}.jpg"));
        fs::write(&source, bytes).unwrap();
        let result = optimize_jpeg(
            &JpegRequest::new(&source),
            &engine,
            &CancellationToken::default(),
            |_| {},
        );
        assert!(
            result.is_err()
                && !matches!(result, Err(JpegError::ContentCredentialsRequireConsent(_)))
        );
        assert_eq!(fs::read(&source).unwrap(), *bytes);
    }
    // 无效熵数据不能仅凭结构上存在C2PA而取得确认许可。
    let mut corrupt = signed.clone();
    let scan = corrupt.windows(2).position(|b| b == [0xff, 0xda]).unwrap();
    let scan_length = usize::from(u16::from_be_bytes([corrupt[scan + 2], corrupt[scan + 3]]));
    corrupt.drain(scan + 2 + scan_length..corrupt.len() - 2);
    let bad_source = root.join("bad-entropy.jpg");
    fs::write(&bad_source, &corrupt).unwrap();
    let error = optimize_jpeg(
        &JpegRequest::new(&bad_source),
        &engine,
        &CancellationToken::default(),
        |_| {},
    );
    assert!(
        error.is_err() && !matches!(error, Err(JpegError::ContentCredentialsRequireConsent(_)))
    );

    let inputs = root.join("mixed");
    fs::create_dir(&inputs).unwrap();
    let png_source = inputs.join("credential.png");
    let png = include_bytes!("../../../tests/fixtures/png/content-credentials.png");
    fs::write(&png_source, png).unwrap();
    let jpeg_source = inputs.join("credential.JpEg");
    fs::write(&jpeg_source, &signed).unwrap();
    let engines = ImageEngines::with_jpeg(engine);
    let scan = scan_with_engines(
        std::slice::from_ref(&inputs),
        ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
        &engines,
    )
    .unwrap();
    assert!(scan.issues().is_empty());
    assert_eq!(scan.files().len(), 2);
    let parameters = BatchParameters {
        mode: PngMode::Lossless,
        ..Default::default()
    };
    let mut service = BatchService::with_engines(BatchConfig::default(), engines.clone()).unwrap();
    let batch = service
        .start(BatchRequest {
            items: scan
                .files()
                .iter()
                .map(|f| BatchItem {
                    source: f.source.clone(),
                    output: OutputPolicy::Overwrite,
                    format: f.image.format(),
                })
                .collect(),
            parameters,
            engines: engines.clone(),
        })
        .unwrap();
    let first = service.wait(batch, Duration::from_secs(60)).unwrap();
    assert!(
        first
            .jobs
            .iter()
            .all(|job| job.content_credentials_source().is_some())
    );
    let retry = |snapshot: &pixofold_core::batch::BatchSnapshot, confirm: bool| RetryRequest {
        parameters,
        jobs: snapshot
            .jobs
            .iter()
            .map(|job| RetryJob {
                id: job.id,
                output: OutputPolicy::Overwrite,
                metadata: if confirm {
                    MetadataPolicy::RemoveContentCredentials(
                        job.content_credentials_source().unwrap().clone(),
                    )
                } else {
                    MetadataPolicy::Preserve
                },
            })
            .collect(),
    };
    service.retry(batch, retry(&first, false)).unwrap();
    let repeated = service.wait(batch, Duration::from_secs(60)).unwrap();
    assert!(
        repeated
            .jobs
            .iter()
            .all(|job| job.attempt == 2 && job.content_credentials_source().is_some())
    );
    service.retry(batch, retry(&repeated, true)).unwrap();
    let completed = service.wait(batch, Duration::from_secs(60)).unwrap();
    for job in &completed.jobs {
        let JobState::Succeeded(report) = &job.state else {
            panic!("混合确认失败：{:?}", job.state);
        };
        assert!(report.credentials_removed());
        let ProcessingOutcome::Optimized {
            output,
            backup: Some(backup),
        } = report.outcome()
        else {
            panic!("缺少备份");
        };
        if job.request.format() == ImageKind::Jpeg {
            assert_eq!(fs::read(backup).unwrap(), signed);
            assert_eq!(backup.extension().unwrap(), "JpEg");
            checks.push(json!({"source":backup,"reference":reference,"output":output,"lossless":true,"removed":true}));
        } else {
            assert_eq!(fs::read(backup).unwrap(), png);
        }
    }
    service.shutdown().unwrap();
    // 已确认任务的取消仍经相同输出契约，不产生副本或修改原件。
    let source = root.join("cancel.jpg");
    fs::write(&source, &signed).unwrap();
    let mut request = JpegRequest::new(&source);
    let engine = engines.jpeg().unwrap();
    let Err(JpegError::ContentCredentialsRequireConsent(version)) =
        optimize_jpeg(&request, engine, &CancellationToken::default(), |_| {})
    else {
        panic!("取消语料确认失败");
    };
    request.metadata = MetadataPolicy::RemoveContentCredentials(version);
    let cancel = CancellationToken::default();
    assert!(matches!(
        optimize_jpeg(&request, engine, &cancel, |stage| {
            if stage == ProcessingStage::BeforeCommit {
                cancel.cancel();
            }
        }),
        Err(JpegError::Cancelled)
    ));
    assert_eq!(fs::read(&source).unwrap(), signed);
    if let Some(public) = args.get(3) {
        let source = fs::canonicalize(public).unwrap();
        let original = fs::read(&source).unwrap();
        let mut request = JpegRequest::new(&source);
        request.output = OutputPolicy::Copy {
            destination: root.join("public-c2pa.jpg"),
        };
        let Err(JpegError::ContentCredentialsRequireConsent(version)) =
            optimize_jpeg(&request, engine, &CancellationToken::default(), |_| {})
        else {
            panic!("公开C2PA样本未进入确认");
        };
        request.metadata = MetadataPolicy::RemoveContentCredentials(version);
        let report =
            optimize_jpeg(&request, engine, &CancellationToken::default(), |_| {}).unwrap();
        assert!(report.content_credentials_removed);
        let ProcessingOutcome::Optimized {
            output,
            backup: None,
        } = report.outcome
        else {
            panic!("公开C2PA样本未输出副本");
        };
        assert_eq!(fs::read(&source).unwrap(), original);
        fs::write(root.join("public-c2pa.json"), serde_json::to_vec_pretty(&json!({"result":"passed","source":source,"output":output,"inputBytes":report.input_bytes.0,"outputBytes":report.output_bytes.0,"credentialsRemoved":true,"sourceUnchanged":true,"signatureAuthenticityVerified":false})).unwrap()).unwrap();
    }
    fs::write(
        root.join("checks.json"),
        serde_json::to_vec_pretty(&checks).unwrap(),
    )
    .unwrap();
    println!(
        "JPEG元数据/凭据真实回归通过，{}项输出待独立复验",
        checks.len()
    );
}
