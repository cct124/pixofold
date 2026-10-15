//! 单文件生产模式的隔离交付证据；与实验矩阵分别记录。
#[path = "../../tests/support/apng_lossy.rs"]
mod sample;
use super::*;
use pixofold_core::model::QualityValue;

pub fn run(root: &Path) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    fs::write(root.join("inputs/palette-core.apng"), sample::animation())?;
    let mut rows = Vec::new();
    for (name, quality, expected) in [
        ("palette-core.apng", 80, "lossy"),
        ("palette-core.apng", 100, "lossless"),
        ("generated/default-in.png", 80, "insufficient_gain"),
        (
            "public/Animated_PNG_example_bouncing_beach_ball.png",
            80,
            "quality_below_target",
        ),
        ("generated/rgba16.apng", 80, "unsupported_depth"),
        ("generated/rgba16.apng", 100, "lossless"),
        ("generated/adam7.apng", 80, "unsupported_interlace"),
        ("generated/metadata-caBX.apng", 80, "unsupported_metadata"),
    ] {
        let source = root.join("inputs").join(name);
        let original = fs::read(&source)?;
        let destination = root
            .join("outputs")
            .join(format!("production-{}.apng", rows.len()));
        let mut request = ApngRequest::new(&source);
        request.mode = ApngMode::Lossy {
            quality: QualityValue::new(quality)?,
        };
        request.output = OutputPolicy::Copy {
            destination: destination.clone(),
        };
        let row = match optimize_apng(&request, &CancellationToken::default(), |_| {}) {
            Err(ApngError::Validation(error)) if error.0 == expected => {
                assert!(!destination.exists());
                json!({"file":name,"requestedMode":request.mode,"result":"rejected","reason":error.0})
            }
            Err(error) => return Err(error.into()),
            Ok(report) => {
                let actual = match report.processing {
                    ApngProcessing::Lossless => "lossless",
                    ApngProcessing::Lossy { .. } => "lossy",
                    ApngProcessing::LosslessFallback {
                        reason: ApngFallbackReason::QualityBelowTarget,
                        ..
                    } => "quality_below_target",
                    ApngProcessing::LosslessFallback {
                        reason: ApngFallbackReason::InsufficientGain,
                        ..
                    } => "insufficient_gain",
                };
                assert_eq!(actual, expected);
                assert_eq!(report.requested_mode, request.mode);
                assert_eq!(report.quality.is_some(), actual == "lossy");
                let (outcome, output_hash) = match &report.outcome {
                    ProcessingOutcome::NoGain => {
                        assert!(!destination.exists());
                        ("no_gain", None)
                    }
                    ProcessingOutcome::Optimized { output, backup } => {
                        assert!(backup.is_none());
                        let stored = fs::read(output)?;
                        if actual == "lossy" {
                            assert_eq!((stored[24], stored[25]), (8, 3));
                        } else {
                            compare_apng(
                                &original,
                                &stored,
                                &request.limits.validation,
                                &CancellationToken::default(),
                            )?;
                        }
                        ("gain", Some(hash(&stored)))
                    }
                };
                json!({"file":name,"requestedMode":report.requested_mode,"processing":report.processing,"quality":report.quality,"result":outcome,"inputBytes":report.input_bytes.0,"outputBytes":report.output_bytes.0,"outputSha256":output_hash,"elapsedMs":report.elapsed.as_millis(),"workingSetBytes":request.limits.working_set_for(request.mode,u64::from(report.image.width)*u64::from(report.image.height))?.0})
            }
        };
        assert_eq!(fs::read(&source)?, original);
        let mut row = row;
        row["sourceSha256"] = json!(hash(&original));
        row["sourceUnchanged"] = json!(true);
        rows.push(row);
    }
    Ok(rows)
}
