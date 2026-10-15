//! 单文件生产有损路径：真实候选选择、回退及共享文件提交边界。
#[path = "support/apng_lossy.rs"]
mod sample;
use pixofold_core::{
    apng::*,
    model::{
        CancellationToken, OutputPolicy, ProcessingError, ProcessingOutcome, ProcessingStage,
        QualityValue,
    },
};
use std::{fs, path::Path};
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/apng")
            .join(name),
    )
    .unwrap()
}
fn lossy(quality: u8) -> ApngMode {
    ApngMode::Lossy {
        quality: QualityValue::new(i32::from(quality)).unwrap(),
    }
}
fn setup(bytes: &[u8]) -> (tempfile::TempDir, ApngRequest) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("动画.APNG");
    fs::write(&source, bytes).unwrap();
    let mut request = ApngRequest::new(source);
    request.mode = lossy(80);
    (dir, request)
}
fn run(request: &ApngRequest) -> Result<ApngReport, ApngError> {
    optimize_apng(request, &CancellationToken::default(), |_| {})
}
fn source_only(dir: &Path, request: &ApngRequest, original: &[u8]) {
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(fs::read_dir(dir).unwrap().count(), 1);
}
fn assert_lossless(original: &[u8], report: &ApngReport) {
    assert!(report.quality.is_none());
    if let ProcessingOutcome::Optimized { output, .. } = &report.outcome {
        compare_apng(
            original,
            &fs::read(output).unwrap(),
            &ApngValidationLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    }
}
#[test]
fn actual_palette_output_beats_lossless_and_preserves_backup_and_copy_sources() {
    let original = sample::animation();
    for copy in [false, true] {
        let (dir, mut request) = setup(&original);
        let baseline = dir.path().join("lossless.apng");
        request.mode = ApngMode::Lossless;
        request.output = OutputPolicy::Copy {
            destination: baseline,
        };
        let lossless = run(&request).unwrap();
        request.mode = lossy(80);
        request.output = if copy {
            OutputPolicy::Copy {
                destination: dir.path().join("lossy.apng"),
            }
        } else {
            OutputPolicy::Overwrite
        };
        let report = run(&request).unwrap();
        assert!(
            matches!(report.processing, ApngProcessing::Lossy { .. }),
            "{report:?}"
        );
        assert_eq!(report.requested_mode, request.mode);
        let quality = report.quality.as_ref().unwrap();
        assert!(quality.palette_entries <= 256);
        assert!(quality.minimum_pixel_quality >= 80);
        assert!(quality.minimum_remapping_quality >= 80);
        assert_eq!(quality.max_alpha_error, 0.0);
        let saved = lossless.output_bytes.0 - report.output_bytes.0;
        assert!(saved >= 1024 && saved >= lossless.output_bytes.0.div_ceil(100));
        let ProcessingOutcome::Optimized { output, backup } = report.outcome else {
            panic!("expected gain")
        };
        let stored = fs::read(output).unwrap();
        assert_eq!(stored[24], 8);
        assert_eq!(stored[25], 3);
        let playback = validate_apng(
            &stored,
            &ApngValidationLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
        assert_eq!(playback.plays, 3);
        assert!(
            playback
                .frames
                .iter()
                .all(|frame| frame.effective_delay == [7, 100])
        );
        assert_eq!(backup.is_some(), !copy);
        if let Some(backup) = backup {
            assert_eq!(backup.extension().unwrap(), "APNG");
            assert_eq!(fs::read(backup).unwrap(), original);
        } else {
            assert_eq!(fs::read(&request.source).unwrap(), original);
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }
}
#[test]
fn q100_is_strict_lossless_even_for_representations_rejected_by_quantization() {
    for name in ["split-data.apng", "rgba16.apng", "adam7.apng"] {
        let original = fixture(name);
        let (dir, mut request) = setup(&original);
        assert_eq!(ApngRequest::new(&request.source).mode, ApngMode::Lossless);
        request.mode = lossy(100);
        // q100必须完全绕过色表检索，零额度不能影响严格无损。
        request.limits.max_palette_searches = 0;
        request.output = OutputPolicy::Copy {
            destination: dir.path().join("copy.apng"),
        };
        let report = run(&request).unwrap();
        assert_eq!(report.processing, ApngProcessing::Lossless);
        assert_eq!(report.requested_mode, lossy(100));
        assert_lossless(&original, &report);
        assert_eq!(fs::read(&request.source).unwrap(), original);
    }
}
#[test]
fn tightened_working_set_estimate_covers_live_baseline_and_summaries() {
    let original = sample::animation();
    let (_dir, mut request) = setup(&original);
    request.limits.validation.max_input_bytes = original.len();
    request.limits.validation.max_candidate_bytes = original.len();
    request.limits.validation.max_frames = 4;
    request.limits.validation.max_metadata_bytes = 1024; // 候选PLTE/tRNS也计费。
    request.limits.validation.max_validation_bytes = 16 << 20;
    let lossless = request
        .limits
        .working_set_for(ApngMode::Lossless, 128 * 96)
        .unwrap();
    assert_eq!(
        request
            .limits
            .working_set_for(lossy(100), 128 * 96)
            .unwrap(),
        lossless
    );
    let bound = request
        .limits
        .working_set_for(request.mode, 128 * 96)
        .unwrap();
    assert!(bound.0 > lossless.0);
    request.limits.max_working_set_bytes = bound;
    let report = run(&request).unwrap();
    assert!(matches!(report.processing, ApngProcessing::Lossy { .. }));
}
#[test]
fn insufficient_gain_falls_back_without_backup_or_copy_on_no_gain() {
    let original = fixture("default-in.png");
    for copy in [false, true] {
        let (dir, mut request) = setup(&original);
        if copy {
            request.output = OutputPolicy::Copy {
                destination: dir.path().join("copy.apng"),
            };
        }
        let report = run(&request).unwrap();
        assert!(
            matches!(
                report.processing,
                ApngProcessing::LosslessFallback {
                    reason: ApngFallbackReason::InsufficientGain,
                    ..
                }
            ),
            "{report:?}"
        );
        assert_eq!(report.outcome, ProcessingOutcome::NoGain);
        assert_eq!(report.input_bytes, report.output_bytes);
        assert_lossless(&original, &report);
        source_only(dir.path(), &request, &original);
    }
}
#[test]
fn quality_rejection_returns_verified_lossless_without_rejected_quality_metrics() {
    let original = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/manual-images/apng/Animated_PNG_example_bouncing_beach_ball.png"),
    )
    .unwrap();
    let (_dir, request) = setup(&original);
    let report = run(&request).unwrap();
    assert!(
        matches!(
            report.processing,
            ApngProcessing::LosslessFallback {
                reason: ApngFallbackReason::QualityBelowTarget,
                ..
            }
        ),
        "{report:?}"
    );
    assert_lossless(&original, &report);
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = report.outcome
    else {
        panic!("expected lossless gain")
    };
    assert_eq!(fs::read(backup).unwrap(), original);
}
#[test]
fn unsupported_or_damaged_inputs_and_search_limits_are_hard_failures() {
    for (name, code) in [
        ("rgba16.apng", "unsupported_depth"),
        ("adam7.apng", "unsupported_interlace"),
        ("metadata-caBX.apng", "unsupported_metadata"),
        ("bad-crc.apng", "crc"),
    ] {
        let original = fixture(name);
        let (dir, request) = setup(&original);
        let result = run(&request);
        assert!(
            matches!(&result, Err(ApngError::Validation(error)) if error.0 == code),
            "{name}: {result:?}"
        );
        source_only(dir.path(), &request, &original);
    }
    let original = fixture("default-in.png");
    let (dir, mut request) = setup(&original);
    request.limits.max_palette_searches = 0;
    assert!(matches!(
        run(&request),
        Err(ApngError::ResourceLimit("palette_search_budget"))
    ));
    source_only(dir.path(), &request, &original);
}
#[test]
fn lossy_cancellation_source_change_and_target_conflict_keep_originals() {
    let original = sample::animation();
    for stage in [
        ProcessingStage::Optimizing,
        ProcessingStage::Validating,
        ProcessingStage::BeforeCommit,
    ] {
        let (dir, request) = setup(&original);
        let cancel = CancellationToken::default();
        let result = optimize_apng(&request, &cancel, |at| {
            if at == stage {
                cancel.cancel();
            }
        });
        assert!(
            matches!(result, Err(ApngError::Cancelled)),
            "{stage:?}: {result:?}"
        );
        source_only(dir.path(), &request, &original);
    }
    let (dir, request) = setup(&original);
    let changed = fixture("changed-loop.apng");
    let result = optimize_apng(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::write(&request.source, &changed).unwrap();
        }
    });
    assert!(
        matches!(result, Err(ApngError::File(ProcessingError::SourceChanged))),
        "{result:?}"
    );
    source_only(dir.path(), &request, &changed);
    let (dir, mut request) = setup(&original);
    let destination = dir.path().join("copy.apng");
    request.output = OutputPolicy::Copy {
        destination: destination.clone(),
    };
    let result = optimize_apng(&request, &CancellationToken::default(), |stage| {
        if stage == ProcessingStage::BeforeCommit {
            fs::write(&destination, &changed).unwrap();
        }
    });
    assert!(
        matches!(
            result,
            Err(ApngError::File(ProcessingError::TargetConflict))
        ),
        "{result:?}"
    );
    assert_eq!(fs::read(&destination).unwrap(), changed);
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}
