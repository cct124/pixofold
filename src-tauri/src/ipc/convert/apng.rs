//! v14 APNG实际模式/质量契约；不把请求有损、回退或被淘汰候选伪装成有损结果。
use super::*;
use pixofold_core::apng::{
    ApngFallbackReason, ApngMode, ApngProcessing, ApngQualityMapping, ApngReport,
};

pub(super) fn processing(report: &ApngReport) -> Result<ApngProcessingDto> {
    let frames = report.image.frames;
    let num_plays = report.image.num_plays;
    let default_participates = report.image.default_participates;
    if frames == 0 || frames > 256 {
        return Err(QueryError::InvalidSnapshot);
    }
    let parameters = match report.processing {
        ApngProcessing::Lossless => {
            if report.quality.is_some()
                || matches!(report.requested_mode, ApngMode::Lossy { quality } if quality.get() != 100)
            {
                return Err(QueryError::InvalidSnapshot);
            }
            return Ok(ApngProcessingDto::Lossless {
                frames,
                num_plays,
                default_participates,
            });
        }
        ApngProcessing::Lossy { parameters }
        | ApngProcessing::LosslessFallback { parameters, .. } => parameters,
    };
    let ApngMode::Lossy { quality } = report.requested_mode else {
        return Err(QueryError::InvalidSnapshot);
    };
    if quality.get() == 100 || parameters != ApngQualityMapping::new(quality) {
        return Err(QueryError::InvalidSnapshot);
    }
    let mapping_version = parameters.version;
    let policy_version = parameters.policy_version;
    match report.processing {
        ApngProcessing::Lossy { .. } => {
            let measured = report.quality.as_ref().ok_or(QueryError::InvalidSnapshot)?;
            let limit = f64::from(100 - quality.get()) * 0.2;
            if !matches!(report.outcome, ProcessingOutcome::Optimized { .. })
                || report.output_bytes >= report.input_bytes
                || !(1..=256).contains(&measured.palette_entries)
                || !(quality.get()..=100).contains(&measured.minimum_pixel_quality)
                || !(quality.get()..=100).contains(&measured.minimum_remapping_quality)
                || ![
                    (measured.max_alpha_error, 8.0),
                    (measured.worst_frame_rmse, limit * 1.5),
                    (measured.worst_temporal_rmse, limit * 2.0),
                ]
                .into_iter()
                .all(|(value, maximum)| value.is_finite() && value >= 0.0 && value <= maximum)
            {
                return Err(QueryError::InvalidSnapshot);
            }
            Ok(ApngProcessingDto::Lossy {
                frames,
                num_plays,
                default_participates,
                mapping_version,
                policy_version,
                quality: ApngQualityDto {
                    palette_entries: count(measured.palette_entries)?,
                    minimum_pixel_quality: measured.minimum_pixel_quality,
                    minimum_remapping_quality: measured.minimum_remapping_quality,
                    max_alpha_error: measured.max_alpha_error,
                    worst_frame_rmse: measured.worst_frame_rmse,
                    worst_temporal_rmse: measured.worst_temporal_rmse,
                },
            })
        }
        ApngProcessing::LosslessFallback { reason, .. } => {
            if report.quality.is_some() {
                return Err(QueryError::InvalidSnapshot);
            }
            Ok(ApngProcessingDto::LosslessFallback {
                frames,
                num_plays,
                default_participates,
                mapping_version,
                policy_version,
                reason: match reason {
                    ApngFallbackReason::QualityBelowTarget => ApngFallbackDto::QualityBelowTarget,
                    ApngFallbackReason::InsufficientGain => ApngFallbackDto::InsufficientGain,
                },
            })
        }
        ApngProcessing::Lossless => Err(QueryError::InvalidSnapshot),
    }
}
