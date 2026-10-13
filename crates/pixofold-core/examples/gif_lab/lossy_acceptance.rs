//! 真实单文件有损验收：只写新建目录，独立比较最终文件并核对原图/备份/取消/竞争。
use pixofold_core::{gif::*, model::*};
use serde_json::json;
use std::{fs, path::Path};

fn request(source: &Path, destination: &Path, q: i32) -> GifRequest {
    let mut request = GifRequest::new(source);
    request.mode = GifMode::Lossy {
        quality: QualityValue::new(q).unwrap(),
    };
    request.output = OutputPolicy::Copy {
        destination: destination.to_owned(),
    };
    request
}
fn dark_noise() -> Vec<u8> {
    // 低亮度32阶确定性噪声：用于证明有收益也可能越过RGB质量门槛。
    let palette: Vec<u8> = (0..32u8).flat_map(|n| [n, n, n]).collect();
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut bytes, 128, 128, &palette).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        let mut state = 0x12345678u32;
        for _ in 0..3 {
            let pixels: Vec<u8> = (0..128 * 128)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    (state & 31) as u8
                })
                .collect();
            encoder
                .write_frame(&gif::Frame {
                    width: 128,
                    height: 128,
                    delay: 10,
                    dispose: gif::DisposalMethod::Keep,
                    buffer: std::borrow::Cow::Owned(pixels),
                    ..Default::default()
                })
                .unwrap();
        }
    }
    bytes
}
pub fn run(engine: &GifEngine, fixtures: &Path, root: &Path) {
    let directory = root.join("lossy");
    fs::create_dir(&directory).unwrap();
    super::quality_fixtures::generate(
        &directory,
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/manual-images/gif/hopper.gif"),
        )
        .unwrap(),
    )
    .unwrap();
    let source = directory.join("photo-pan.gif");
    let original = fs::read(&source).unwrap();
    let cancel = CancellationToken::default();
    let mut rows = Vec::new();
    let noisy = directory.join("dark-noise.gif");
    let noisy_bytes = dark_noise();
    fs::write(&noisy, &noisy_bytes).unwrap();
    let noisy_output = directory.join("dark-noise-output.gif");
    let noisy_report =
        optimize_gif(&request(&noisy, &noisy_output, 60), engine, &cancel, |_| {}).unwrap();
    assert!(
        matches!(
            noisy_report.processing,
            GifProcessing::LosslessFallback {
                reason: GifFallbackReason::QualityBelowTarget,
                ..
            }
        ),
        "{noisy_report:?}"
    );
    assert_eq!(fs::read(&noisy).unwrap(), noisy_bytes);
    if noisy_output.exists() {
        assert!(
            compare_gif(
                &noisy_bytes,
                &fs::read(&noisy_output).unwrap(),
                GifLimits::default(),
                &cancel
            )
            .unwrap()
            .lossless_equivalent
        );
    }
    rows.push(json!({"sample":"dark-noise","processing":noisy_report.processing,"inputBytes":noisy_report.input_bytes.0,"outputBytes":noisy_report.output_bytes.0,"metrics":noisy_report.quality}));
    for q in [0, 60, 80, 99, 100] {
        let output = directory.join(format!("q{q}.gif"));
        let report = optimize_gif(&request(&source, &output, q), engine, &cancel, |_| {}).unwrap();
        if q <= 80 {
            assert!(matches!(report.processing, GifProcessing::Lossy { .. }));
        }
        if q == 100 {
            assert_eq!(report.processing, GifProcessing::Lossless);
        }
        assert_eq!(
            report.requested_mode,
            GifMode::Lossy {
                quality: QualityValue::new(q).unwrap()
            }
        );
        if matches!(report.outcome, ProcessingOutcome::Optimized { .. }) {
            let result = fs::read(&output).unwrap();
            let comparison =
                compare_gif(&original, &result, GifLimits::default(), &cancel).unwrap();
            assert!(comparison.hard_constraints);
            assert_eq!(
                comparison.lossless_equivalent,
                !matches!(report.processing, GifProcessing::Lossy { .. })
            );
            if !comparison.lossless_equivalent {
                assert!(meets_quality(&comparison));
            }
        }
        assert_eq!(fs::read(&source).unwrap(), original);
        rows.push(json!({"quality":q,"processing":report.processing,"inputBytes":report.input_bytes.0,"outputBytes":report.output_bytes.0,"metrics":report.quality}));
    }
    let recording = fixtures.join("openrefine-320.gif");
    let recording_before = fs::read(&recording).unwrap();
    let no_gain_output = directory.join("recording.gif");
    let report = optimize_gif(
        &request(&recording, &no_gain_output, 60),
        engine,
        &cancel,
        |_| {},
    )
    .unwrap();
    assert!(matches!(
        report.processing,
        GifProcessing::LosslessFallback {
            reason: GifFallbackReason::InsufficientGain,
            ..
        }
    ));
    assert!(matches!(report.outcome, ProcessingOutcome::NoGain));
    assert!(!no_gain_output.exists());
    assert_eq!(fs::read(&recording).unwrap(), recording_before);
    rows.push(json!({"sample":"recording","processing":report.processing,"noGain":true}));
    let ui = directory.join("ui-motion.gif");
    let report = optimize_gif(
        &request(&ui, &directory.join("ui-output.gif"), 80),
        engine,
        &cancel,
        |_| {},
    )
    .unwrap();
    assert_eq!(report.processing, GifProcessing::Lossless);
    assert!(matches!(
        report.outcome,
        ProcessingOutcome::Optimized { .. }
    ));

    for name in ["transparent-illustration.gif", "transparent-seam.gif"] {
        let source = directory.join(name);
        let original = fs::read(&source).unwrap();
        let destination = directory.join(format!("reject-{name}"));
        assert!(matches!(
            optimize_gif(&request(&source, &destination, 80), engine, &cancel, |_| {}),
            Err(GifError::ValidationFailed)
        ));
        assert!(!destination.exists());
        assert_eq!(fs::read(source).unwrap(), original);
    }
    for backed in [false, true] {
        let source = directory.join(if backed {
            "backup.GiF"
        } else {
            "overwrite.GiF"
        });
        fs::write(&source, &original).unwrap();
        let mut request = GifRequest::new(&source);
        request.mode = GifMode::Lossy {
            quality: QualityValue::new(80).unwrap(),
        };
        request.output = if backed {
            OutputPolicy::Overwrite
        } else {
            OutputPolicy::OverwriteWithoutBackup
        };
        let report = optimize_gif(&request, engine, &cancel, |_| {}).unwrap();
        let ProcessingOutcome::Optimized { backup, .. } = report.outcome else {
            panic!("缺少真实提交")
        };
        assert_eq!(backup.is_some(), backed);
        if let Some(backup) = backup {
            assert_eq!(backup.extension().unwrap(), "GiF");
            assert_eq!(fs::read(backup).unwrap(), original);
        }
        assert!(
            !compare_gif(
                &original,
                &fs::read(source).unwrap(),
                GifLimits::default(),
                &cancel
            )
            .unwrap()
            .lossless_equivalent
        );
    }
    for stage in [
        ProcessingStage::Reading,
        ProcessingStage::Optimizing,
        ProcessingStage::Validating,
        ProcessingStage::BeforeCommit,
    ] {
        let case = directory.join(format!("cancel-{stage:?}"));
        fs::create_dir(&case).unwrap();
        let input = case.join("input.gif");
        fs::write(&input, &original).unwrap();
        let token = CancellationToken::default();
        let mut job = GifRequest::new(&input);
        job.mode = GifMode::Lossy {
            quality: QualityValue::new(80).unwrap(),
        };
        assert!(matches!(
            optimize_gif(&job, engine, &token, |current| {
                if current == stage {
                    token.cancel();
                }
            }),
            Err(GifError::Cancelled)
        ));
        assert_eq!(fs::read(&input).unwrap(), original);
        assert_eq!(fs::read_dir(case).unwrap().count(), 1);
    }
    for mutation in ["candidate", "source", "conflict"] {
        let case = directory.join(mutation);
        fs::create_dir(&case).unwrap();
        let input = case.join("input.gif");
        fs::write(&input, &original).unwrap();
        let destination = case.join("output.gif");
        let job = request(&input, &destination, 80);
        let error = optimize_gif(&job, engine, &cancel, |stage| {
            if stage == ProcessingStage::Validating {
                match mutation {
                    "candidate" => {
                        let path = fs::read_dir(&case)
                            .unwrap()
                            .map(|e| e.unwrap().path())
                            .find(|p| *p != input)
                            .unwrap();
                        fs::write(path, &original).unwrap();
                    }
                    "source" => {
                        fs::write(
                            &input,
                            fs::read(fixtures.join("larger-pattern.gif")).unwrap(),
                        )
                        .unwrap();
                    }
                    _ => {
                        fs::write(&destination, b"existing target").unwrap();
                    }
                }
            }
        })
        .unwrap_err();
        match mutation {
            "candidate" => assert!(matches!(error, GifError::ValidationFailed)),
            "source" => assert!(matches!(
                error,
                GifError::File(ProcessingError::SourceChanged)
            )),
            _ => assert!(matches!(
                error,
                GifError::File(ProcessingError::TargetConflict)
            )),
        }
        if mutation != "source" {
            assert_eq!(fs::read(&input).unwrap(), original);
        }
        if mutation == "conflict" {
            assert_eq!(fs::read(&destination).unwrap(), b"existing target");
        } else {
            assert!(!destination.exists());
        }
        assert_eq!(
            fs::read_dir(case).unwrap().count(),
            if mutation == "conflict" { 2 } else { 1 }
        );
    }
    let mut limited = request(&source, &directory.join("limited.gif"), 80);
    limited.limits.resources.max_decoded_bytes = ByteCount(2 * 1024 * 1024);
    assert!(matches!(
        optimize_gif(&limited, engine, &cancel, |_| {}),
        Err(GifError::ResourceLimit(_))
    ));
    assert!(!directory.join("limited.gif").exists());
    assert_eq!(fs::read(&source).unwrap(), original);
    fs::write(directory.join("report.json"),serde_json::to_vec_pretty(&json!({"result":"passed","rows":rows,"boundaries":"actual lossless/lossy, insufficient gain NoGain, transparency, backup, cancellation, candidate/source/target changes, memory"})).unwrap()).unwrap();
    println!("GIF有损单文件真实验收通过：质量锚点、收益回退、原图安全与提交竞争");
}
