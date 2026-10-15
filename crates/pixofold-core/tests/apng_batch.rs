//! APNG内容分流、真实批量输出与格式专属拒绝；隔离目录，不使用GUI或外部工具。
use pixofold_core::{apng::*, batch::*, import::*, model::*};
use std::{fs, path::Path, time::Duration};
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/apng")
            .join(name),
    )
    .unwrap()
}
#[test]
fn scan_routes_by_content_only_when_apng_is_enabled_and_keeps_rejection_domains() {
    let dir = tempfile::tempdir().unwrap();
    for (name, sample) in [
        ("animation.PNG", "split-data.apng"),
        ("animation.APNG", "default-out.apng"),
        ("low.apng", "low-depth.apng"),
        ("signed.apng", "metadata-caBX.apng"),
        ("bad.apng", "bad-crc.apng"),
        ("decode.apng", "bad-deflate.apng"),
    ] {
        fs::write(dir.path().join(name), fixture(sample)).unwrap();
    }
    fs::write(
        dir.path().join("static.apng"),
        include_bytes!("../../../tests/fixtures/png/rgb8.png"),
    )
    .unwrap();
    let cancel = CancellationToken::default();
    let roots = [dir.path().to_owned()];
    let png_only = scan(&roots, ScanOptions::default(), &cancel, |_| {}).unwrap();
    assert_eq!(png_only.files().len(), 1);
    assert_eq!(png_only.files()[0].image.format(), ImageKind::Png);
    let scan = scan_with_engines(
        &roots,
        ScanOptions::default(),
        &cancel,
        |_| {},
        &ImageEngines::default().add_apng(),
    )
    .unwrap();
    assert_eq!(scan.files().len(), 4);
    assert_eq!(
        scan.files()
            .iter()
            .filter(|f| f.image.format() == ImageKind::Apng)
            .count(),
        3
    );
    // 扫描不解码，坏deflate留到流水线；受保护APNG不能获得PNG移除许可。
    let codes: Vec<_> = scan
        .issues()
        .iter()
        .map(|issue| match &issue.kind {
            ImportIssueKind::Failure(f) => f.code,
            _ => panic!("unexpected issue"),
        })
        .collect();
    assert!(codes.contains(&JobErrorCode::UnsupportedApng));
    assert!(codes.contains(&JobErrorCode::UnsupportedApngMetadata));
    assert!(codes.contains(&JobErrorCode::InvalidInput));
}
#[test]
fn real_mixed_outputs_no_gain_conflicts_and_retry_keep_frozen_targets() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    let engines = ImageEngines::default().add_apng();
    let mut items = Vec::new();
    for (name, sample) in [
        ("gain.APNG", "split-data.apng"),
        ("small.png", "default-in.png"),
        ("bad.apng", "bad-deflate.apng"),
        ("occupied.apng", "split-data.apng"),
    ] {
        let source = dir.path().join(name);
        fs::write(&source, fixture(sample)).unwrap();
        items.push(BatchItem {
            source,
            format: ImageKind::Apng,
            output: OutputPolicy::Copy {
                destination: out.join(name),
            },
        });
    }
    fs::write(out.join("occupied.apng"), b"keep").unwrap();
    let parameters = BatchParameters {
        mode: PngMode::Lossy {
            quality: QualityValue::new(80).unwrap(),
        },
        ..Default::default()
    };
    let service = BatchService::with_engines(
        BatchConfig {
            workers: 2,
            ..Default::default()
        },
        engines.clone(),
    )
    .unwrap();
    let id = service
        .start(BatchRequest {
            items,
            parameters,
            engines,
        })
        .unwrap();
    let done = service.wait(id, Duration::from_secs(30)).unwrap();
    assert_eq!(
        (
            done.summary.succeeded,
            done.summary.no_gain,
            done.summary.failed
        ),
        (1, 1, 2)
    );
    assert_eq!((done.active_workers, done.reserved_working_bytes.0), (0, 0));
    let JobState::Succeeded(ImageReport::Apng(report)) = &done.jobs[0].state else {
        panic!("{done:?}")
    };
    assert_eq!(report.processing, ApngProcessing::Lossless);
    assert_eq!(
        report.requested_mode,
        pixofold_core::apng::ApngMode::Lossless
    );
    assert!(report.quality.is_none());
    assert_eq!(report.image.frames, 2);
    compare_apng(
        &fixture("split-data.apng"),
        &fs::read(out.join("gain.APNG")).unwrap(),
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert!(!out.join("small.png").exists());
    assert_eq!(fs::read(out.join("occupied.apng")).unwrap(), b"keep");
    let bad = &done.jobs[2];
    assert!(matches!(&bad.state, JobState::Failed(f) if f.code == JobErrorCode::Decode));
    assert!(bad.content_credentials_source().is_none());
    fs::write(&bad.request.source, fixture("split-data.apng")).unwrap();
    service
        .retry(
            id,
            RetryRequest {
                jobs: vec![RetryJob {
                    id: bad.id,
                    output: bad.request.output.clone(),
                    metadata: PngMetadataPolicy::Preserve,
                }],
                parameters: BatchParameters::default(),
            },
        )
        .unwrap();
    let retried = service.wait(id, Duration::from_secs(30)).unwrap();
    assert_eq!(retried.jobs[2].id, bad.id);
    assert_eq!(retried.jobs[2].attempt, 2);
    assert!(matches!(
        retried.jobs[2].state,
        JobState::Succeeded(ImageReport::Apng(_))
    ));
    assert_eq!(retried.jobs[0].attempt, 1);
    assert!(out.join("bad.apng").is_file());
    assert_eq!(
        fs::read(dir.path().join("gain.APNG")).unwrap(),
        fixture("split-data.apng")
    );
}
#[test]
fn missing_capability_and_insufficient_budget_fail_rows_without_touching_sources() {
    for (enabled, budget, expected) in [
        (false, 512 << 20, JobErrorCode::UnsupportedFormat),
        (true, 1, JobErrorCode::ResourceLimit),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.apng");
        let bytes = fixture("split-data.apng");
        fs::write(&source, &bytes).unwrap();
        let engines = if enabled {
            ImageEngines::default().add_apng()
        } else {
            ImageEngines::default()
        };
        let service = BatchService::with_engines(
            BatchConfig {
                working_set_budget: ByteCount(budget),
                ..Default::default()
            },
            engines.clone(),
        )
        .unwrap();
        let id = service
            .start(BatchRequest {
                items: vec![BatchItem {
                    source: source.clone(),
                    format: ImageKind::Apng,
                    output: OutputPolicy::Overwrite,
                }],
                engines,
                parameters: Default::default(),
            })
            .unwrap();
        let done = service.wait(id, Duration::from_secs(10)).unwrap();
        assert!(matches!(&done.jobs[0].state, JobState::Failed(f) if f.code == expected));
        assert_eq!(done.reserved_working_bytes.0, 0);
        assert_eq!(fs::read(source).unwrap(), bytes);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
