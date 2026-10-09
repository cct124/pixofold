//! 真实PNG/JPEG扫描到批次/输出/重试的隔离回归；由jpeg:core:check提供可信工具与语料。
use pixofold_core::{batch::*, import::*, jpeg::*, model::*};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[path = "../tests/support/native_names.rs"]
mod native_names;

const WAIT: Duration = Duration::from_secs(60);
fn png(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}
fn folder(root: &Path, name: &str) -> PathBuf {
    let p = root.join(name);
    fs::create_dir(&p).unwrap();
    p
}
fn put(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, bytes).unwrap();
    p
}
fn scan_all(roots: &[PathBuf], engines: &ImageEngines) -> ImportScan {
    scan_with_engines(
        roots,
        ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
        engines,
    )
    .unwrap()
}
fn service(engines: &ImageEngines) -> BatchService {
    BatchService::with_engines(
        BatchConfig {
            workers: 3,
            working_set_budget: ByteCount(512 * 1024 * 1024),
            ..BatchConfig::default()
        },
        engines.clone(),
    )
    .unwrap()
}
fn finish(service: &BatchService, id: BatchId) -> BatchSnapshot {
    let done = service.wait(id, WAIT).unwrap();
    assert_eq!(done.phase, BatchPhase::Finished);
    assert_eq!(done.summary.terminal, done.summary.total);
    assert_eq!((done.active_workers, done.reserved_working_bytes.0), (0, 0));
    done
}
fn direct(items: Vec<BatchItem>, engines: &ImageEngines) -> BatchRequest {
    BatchRequest {
        items,
        parameters: BatchParameters::default(),
        engines: engines.clone(),
    }
}
fn item(source: &Path, format: ImageKind, output: OutputPolicy) -> BatchItem {
    BatchItem {
        source: source.to_owned(),
        output,
        format,
    }
}

fn roundtrip(
    root: &Path,
    fixtures: &Path,
    engines: &ImageEngines,
    mode: CompressionMode,
    checks: &mut Vec<Value>,
) {
    let dir = folder(
        root,
        if mode == CompressionMode::Lossless {
            "lossless"
        } else {
            "lossy"
        },
    );
    let input = folder(&dir, "input");
    let output = folder(&dir, "output");
    put(&input, "图片.PNG", &png("gradient-rgb8.png"));
    put(&input, "real-png.jpg", &png("rgb8.png"));
    put(&input, "bad-pixels.png", &png("bad-deflate.png"));
    put(&input, "already.png", &png("already-optimized.png"));
    for name in ["baseline-420", "icc", "cmyk", "opaque-app11"] {
        put(
            &input,
            &format!("{name}.JpEg"),
            &fs::read(fixtures.join(format!("{name}.jpg"))).unwrap(),
        );
    }
    put(&input, "broken.jpg", &[0xff, 0xd8, 0xff, 0xc0, 0, 1]);
    let before: Vec<_> = fs::read_dir(&input)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let b = fs::read(&p).unwrap();
            (p, b)
        })
        .collect();
    let png_only = scan(
        std::slice::from_ref(&input),
        ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert!(
        png_only
            .files()
            .iter()
            .all(|f| f.image.format() == ImageKind::Png)
    );
    assert!(png_only.issues().iter().any(|i| matches!(
        i.kind,
        ImportIssueKind::Unsupported(UnsupportedFormat::Jpeg)
    )));
    let found = scan_all(std::slice::from_ref(&input), engines);
    assert_eq!(found.files().len(), 7);
    assert_eq!(found.issues().len(), 2);
    assert!(found.issues().iter().any(|i| matches!(&i.kind, ImportIssueKind::Failure(f) if f.code == JobErrorCode::UnsupportedContentCredentials)));
    let parameters = BatchParameters {
        mode,
        ..BatchParameters::default()
    };
    let plan = found
        .plan(
            &ImportOutput::CopyTo {
                directory: output.clone(),
                layout: CopyLayout::Flat,
            },
            parameters,
        )
        .unwrap();
    let mut service = service(engines);
    let id = service.start(plan).unwrap();
    let done = finish(&service, id);
    assert_eq!(done.summary.failed, 1, "只有坏PNG像素进入执行失败");
    assert!(done.summary.succeeded >= 3);
    if mode == CompressionMode::Lossless {
        assert!(done.summary.no_gain >= 1);
    }
    for job in &done.jobs {
        if let JobState::Succeeded(report) | JobState::NoGain(report) = &job.state {
            assert_eq!(report.input_bytes(), job.input_bytes.unwrap());
            if let ImageReport::Jpeg(r) = report {
                if mode != CompressionMode::Lossless
                    && ["icc", "cmyk"]
                        .contains(&job.request.source.file_stem().unwrap().to_str().unwrap())
                {
                    assert!(matches!(
                        r.processing,
                        JpegProcessing::LosslessFallback { .. }
                    ));
                }
                if let ProcessingOutcome::Optimized { output, backup } = &r.outcome {
                    assert!(backup.is_none());
                    checks.push(json!({ "source": job.request.source, "output": output, "lossless": !matches!(r.processing, JpegProcessing::Lossy { .. }) }));
                }
            }
        }
        if matches!(job.state, JobState::Failed(_) | JobState::NoGain(_)) {
            assert!(
                !output
                    .join(job.request.source.file_name().unwrap())
                    .exists()
            );
        }
    }
    for (p, b) in before {
        assert_eq!(fs::read(p).unwrap(), b);
    }
    service.shutdown().unwrap();
}

fn conflicts_and_retry(root: &Path, fixtures: &Path, engines: &ImageEngines) {
    let dir = folder(root, "conflicts");
    let input = folder(&dir, "input");
    let output = folder(&dir, "output");
    let p = put(&input, "one.png", &png("gradient-rgb8.png"));
    let j = put(
        &input,
        "two.jpg",
        &fs::read(fixtures.join("baseline-420.jpg")).unwrap(),
    );
    let roots = [p.clone(), j.clone()];
    let found = scan_all(&roots, engines);
    let mut service = service(engines);
    let id = service
        .start(
            found
                .plan(&ImportOutput::CopyBeside, BatchParameters::default())
                .unwrap(),
        )
        .unwrap();
    let all = finish(&service, id);
    assert_eq!(all.summary.failed, 2);
    assert!(
        all.jobs.iter().all(
            |j| matches!(&j.state,JobState::Failed(f) if f.code==JobErrorCode::TargetConflict)
        )
    );
    let target = output.join("two.jpg");
    fs::write(&target, b"existing").unwrap();
    let id = service
        .start(
            found
                .plan(
                    &ImportOutput::CopyTo {
                        directory: output.clone(),
                        layout: CopyLayout::Flat,
                    },
                    BatchParameters::default(),
                )
                .unwrap(),
        )
        .unwrap();
    let first = finish(&service, id);
    assert_eq!((first.summary.succeeded, first.summary.failed), (1, 1));
    assert_eq!(fs::read(&target).unwrap(), b"existing");
    fs::remove_file(&target).unwrap();
    let retry = first.jobs.iter().find(|j| j.state.can_retry()).unwrap();
    let row_id = retry.id;
    service
        .retry(
            id,
            RetryRequest {
                jobs: vec![RetryJob {
                    id: row_id,
                    output: retry.request.output.clone(),
                    metadata: PngMetadataPolicy::Preserve,
                }],
                parameters: BatchParameters {
                    mode: CompressionMode::Lossy {
                        quality: QualityValue::new(40).unwrap(),
                    },
                    ..BatchParameters::default()
                },
            },
        )
        .unwrap();
    let second = finish(&service, id);
    assert!(second.revision > first.revision);
    let png_row = second
        .jobs
        .iter()
        .find(|r| r.request.format() == ImageKind::Png)
        .unwrap();
    assert_eq!(png_row.attempt, 1);
    assert_eq!(png_row.request.mode, CompressionMode::Lossless);
    let jpeg_row = second.jobs.iter().find(|r| r.id == row_id).unwrap();
    assert_eq!(jpeg_row.attempt, 2);
    assert!(
        matches!(&jpeg_row.state, JobState::Succeeded(ImageReport::Jpeg(JpegReport {processing:JpegProcessing::Lossy {parameters},..})) if parameters.quality.get()==40)
    );
    assert!(
        matches!(&jpeg_row.request.output,OutputPolicy::CopyTree {root,relative} if same_file::is_same_file(root,&output).unwrap() && relative==Path::new("two.jpg"))
    );
    let same = output.join("same.bin");
    let id = service
        .start(direct(
            vec![
                item(
                    &p,
                    ImageKind::Png,
                    OutputPolicy::Copy {
                        destination: same.clone(),
                    },
                ),
                item(
                    &j,
                    ImageKind::Jpeg,
                    OutputPolicy::Copy { destination: same },
                ),
            ],
            engines,
        ))
        .unwrap();
    assert_eq!(finish(&service, id).summary.failed, 2);
    assert!(matches!(
        service.start(direct(
            vec![
                item(&p, ImageKind::Png, OutputPolicy::Overwrite),
                item(&p, ImageKind::Png, OutputPolicy::Overwrite)
            ],
            engines
        )),
        Err(BatchError::PathConflict {
            kind: PathConflictKind::DuplicateSource,
            ..
        })
    ));
    service.shutdown().unwrap();
}

fn backup_layout_and_native_names(root: &Path, fixtures: &Path, engines: &ImageEngines) {
    let jpeg = fs::read(fixtures.join("baseline-420.jpg")).unwrap();
    for backup in [false, true] {
        let dir = folder(root, if backup { "backup" } else { "no-backup" });
        let p = put(&dir, "原图.PNG", &png("rgb8.png"));
        let j = put(&dir, "原图.JpEg", &jpeg);
        let originals = [
            (p.clone(), fs::read(&p).unwrap()),
            (j.clone(), jpeg.clone()),
        ];
        let found = scan_all(std::slice::from_ref(&dir), engines);
        let mut service = service(engines);
        let strategy = if backup {
            ImportOutput::Overwrite
        } else {
            ImportOutput::OverwriteWithoutBackup
        };
        let done = finish(
            &service,
            service
                .start(found.plan(&strategy, BatchParameters::default()).unwrap())
                .unwrap(),
        );
        assert_eq!(done.summary.succeeded, 2);
        for job in &done.jobs {
            let bytes = &originals
                .iter()
                .find(|(path, _)| path.file_name() == job.request.source.file_name())
                .unwrap()
                .1;
            let JobState::Succeeded(r) = &job.state else {
                panic!("success")
            };
            let ProcessingOutcome::Optimized { backup: copy, .. } = r.outcome() else {
                panic!("output")
            };
            assert_eq!(copy.is_some(), backup);
            if let Some(copy) = copy {
                assert_eq!(fs::read(copy).unwrap(), *bytes);
                assert_eq!(
                    copy.extension().unwrap(),
                    if job.request.format() == ImageKind::Png {
                        "png"
                    } else {
                        "JpEg"
                    }
                );
            }
        }
        let rescan = scan_all(std::slice::from_ref(&dir), engines);
        assert_eq!(rescan.files().len(), 2);
        assert_eq!(rescan.progress().excluded, if backup { 2 } else { 0 });
        service.shutdown().unwrap();
    }
    let dir = folder(root, "layout");
    let left = folder(&dir, "left");
    let nested = folder(&left, "sub");
    let dest = folder(&dir, "output");
    let p = put(&nested, "same.PNG", &png("rgb8.png"));
    let j = put(&nested, "same.JPEG", &jpeg);
    let found = scan_all(&[left], engines);
    let mut service = service(engines);
    let done = finish(
        &service,
        service
            .start(
                found
                    .plan(
                        &ImportOutput::CopyTo {
                            directory: dest.clone(),
                            layout: CopyLayout::PreserveRoots,
                        },
                        BatchParameters::default(),
                    )
                    .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(done.summary.succeeded, 2);
    for path in [p, j] {
        assert!(
            dest.join("left/sub")
                .join(path.file_name().unwrap())
                .exists()
        );
    }
    service.shutdown().unwrap();
    let native = folder(root, "native-name");
    let source = native_names::write_native_name(&native, ".JpEg", &jpeg);
    let dest = folder(root, "native-output");
    let found = scan_all(std::slice::from_ref(&source), engines);
    let mut service = self::service(engines);
    let done = finish(
        &service,
        service
            .start(
                found
                    .plan(
                        &ImportOutput::CopyTo {
                            directory: dest.clone(),
                            layout: CopyLayout::Flat,
                        },
                        BatchParameters::default(),
                    )
                    .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(done.summary.succeeded, 1);
    assert!(dest.join(source.file_name().unwrap()).exists());
    assert_eq!(fs::read(&source).unwrap(), jpeg);
    service.shutdown().unwrap();
}

fn credentials_and_capabilities(
    root: &Path,
    fixtures: &Path,
    engines: &ImageEngines,
    tool: &Path,
    hash: [u8; 32],
) {
    let dir = folder(root, "credentials");
    let p = put(&dir, "consent.png", &png("content-credentials.png"));
    let j = put(
        &dir,
        "protected.jpg",
        &fs::read(fixtures.join("opaque-app11.jpg")).unwrap(),
    );
    let mut service = service(engines);
    let items = vec![
        item(&p, ImageKind::Png, OutputPolicy::Overwrite),
        item(&j, ImageKind::Jpeg, OutputPolicy::Overwrite),
    ];
    let done = finish(
        &service,
        service.start(direct(items.clone(), engines)).unwrap(),
    );
    assert_eq!(done.summary.failed, 2);
    let consent = done.jobs[0].content_credentials_source().unwrap().clone();
    assert!(done.jobs[1].content_credentials_source().is_none());
    assert!(matches!(
        service.retry(
            done.id,
            RetryRequest {
                jobs: vec![RetryJob {
                    id: done.jobs[1].id,
                    output: OutputPolicy::Overwrite,
                    metadata: PngMetadataPolicy::RemoveContentCredentials(consent.clone())
                }],
                parameters: BatchParameters::default()
            }
        ),
        Err(BatchError::InvalidRetry)
    ));
    assert_eq!(service.snapshot().unwrap().revision, done.revision);
    service
        .retry(
            done.id,
            RetryRequest {
                jobs: vec![RetryJob {
                    id: done.jobs[0].id,
                    output: OutputPolicy::Overwrite,
                    metadata: PngMetadataPolicy::RemoveContentCredentials(consent),
                }],
                parameters: BatchParameters::default(),
            },
        )
        .unwrap();
    let confirmed = finish(&service, done.id);
    assert_eq!(confirmed.jobs[0].attempt, 2);
    assert_eq!(confirmed.jobs[1].attempt, 1);
    assert!(matches!(&confirmed.jobs[0].state,JobState::Succeeded(r) if r.credentials_removed()));
    let other = ImageEngines::with_jpeg(JpegEngine::load(tool, hash).unwrap());
    assert!(matches!(
        service.start(direct(items, &other)),
        Err(BatchError::InvalidConfig)
    ));
    service.shutdown().unwrap();
    let dir = folder(root, "missing-engine");
    let p = put(&dir, "png.png", &png("rgb8.png"));
    let j = put(
        &dir,
        "jpeg.jpg",
        &fs::read(fixtures.join("baseline-420.jpg")).unwrap(),
    );
    let mut png_service = BatchService::new(BatchConfig::default()).unwrap();
    let done = finish(
        &png_service,
        png_service
            .start(direct(
                vec![
                    item(&p, ImageKind::Png, OutputPolicy::Overwrite),
                    item(&j, ImageKind::Jpeg, OutputPolicy::Overwrite),
                ],
                &ImageEngines::default(),
            ))
            .unwrap(),
    );
    assert_eq!((done.summary.succeeded, done.summary.failed), (1, 1));
    assert!(
        matches!(&done.jobs[1].state,JobState::Failed(f) if f.code==JobErrorCode::ToolIdentity)
    );
    png_service.shutdown().unwrap();
}

fn bounded_scan_and_shutdown(root: &Path, fixtures: &Path, engines: &ImageEngines) {
    let dir = folder(root, "bounded");
    let base = fs::read(fixtures.join("baseline-420.jpg")).unwrap();
    let source = put(&dir, "source.jpg", &base);
    let limited = scan_with_engines(
        std::slice::from_ref(&source),
        ScanOptions {
            max_read_bytes: ByteCount(12),
            ..ScanOptions::default()
        },
        &CancellationToken::default(),
        |_| {},
        engines,
    )
    .unwrap();
    assert_eq!(
        limited.progress().status,
        ScanStatus::Limited(ScanLimit::ReadBytes)
    );
    assert_eq!(limited.progress().read_bytes.0, 12);
    assert!(matches!(
        limited.plan(&ImportOutput::Overwrite, BatchParameters::default()),
        Err(ImportError::IncompleteScan)
    ));
    let cancel = CancellationToken::default();
    let cancelled = scan_with_engines(
        std::slice::from_ref(&source),
        ScanOptions::default(),
        &cancel,
        |p| {
            if p.read_bytes.0 > 0 {
                cancel.cancel();
            }
        },
        engines,
    )
    .unwrap();
    assert_eq!(cancelled.progress().status, ScanStatus::Cancelled);
    let mut items = Vec::new();
    for i in 0..12 {
        let path = put(&dir, &format!("{i}.jpg"), &base);
        items.push(item(
            &path,
            ImageKind::Jpeg,
            OutputPolicy::Copy {
                destination: dir.join(format!("out-{i}.jpg")),
            },
        ));
    }
    let mut service = service(engines);
    service.start(direct(items, engines)).unwrap();
    service.shutdown().unwrap();
    let snapshot = service.snapshot().unwrap();
    assert_eq!(snapshot.phase, BatchPhase::Finished);
    assert_eq!(
        (snapshot.active_workers, snapshot.reserved_working_bytes.0),
        (0, 0)
    );
    for i in 0..12 {
        assert_eq!(fs::read(dir.join(format!("{i}.jpg"))).unwrap(), base);
    }
    assert!(fs::read_dir(&dir).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".pixofold-")
    }));
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 3);
    let hex = args[1].to_str().unwrap();
    let hash = std::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap());
    let tool = Path::new(&args[0]);
    let fixtures = Path::new(&args[2]);
    let engines = ImageEngines::with_jpeg(JpegEngine::load(tool, hash).unwrap());
    let root = folder(fixtures, "mixed-results");
    let mut checks = Vec::new();
    roundtrip(
        &root,
        fixtures,
        &engines,
        CompressionMode::Lossless,
        &mut checks,
    );
    roundtrip(
        &root,
        fixtures,
        &engines,
        CompressionMode::Lossy {
            quality: QualityValue::default(),
        },
        &mut checks,
    );
    conflicts_and_retry(&root, fixtures, &engines);
    backup_layout_and_native_names(&root, fixtures, &engines);
    credentials_and_capabilities(&root, fixtures, &engines, tool, hash);
    bounded_scan_and_shutdown(&root, fixtures, &engines);
    fs::write(
        root.join("checks.json"),
        serde_json::to_vec_pretty(&checks).unwrap(),
    )
    .unwrap();
    println!(
        "真实PNG/JPEG混合批次通过：扫描/能力、两种模式与回退、冲突/重试、备份/原名/结构、凭据隔离、扫描上限与关闭回收。"
    );
}
