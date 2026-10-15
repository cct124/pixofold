//! 顺序生成/独立验证无损和有损候选，共用工作账本；只有质量或收益不足回退。
use super::validation::{
    Guard,
    quantization::{self, Usage},
};
use super::*;

pub(super) struct Selected {
    pub bytes: Vec<u8>,
    pub processing: ApngProcessing,
    pub quality: Option<ApngQuality>,
}
pub(super) fn summary_resident(before: &ApngPlayback) -> u64 {
    before.frames.len() as u64 * 8192 + (1 << 20)
}
pub(super) fn options(limits: ApngLimits, parameters: ApngQualityMapping) -> quantization::Options {
    quantization::Options {
        quality: parameters.quality.get(),
        dithering: parameters.dithering,
        remapping: quantization::Remapping::AlphaPalette,
        limits: limits.validation,
        max_working_set_bytes: limits.max_working_set_bytes.0,
        max_palette_searches: limits.max_palette_searches,
        ..quantization::Options::default()
    }
}
pub(super) fn strict_candidate(
    bytes: &[u8],
    before: &ApngPlayback,
    limits: ApngLimits,
    usage: &mut Usage,
    guard: &Guard<'_>,
    resident: u64,
) -> Result<(), ApngError> {
    let mut remaining = usage.remaining(limits.validation)?;
    remaining.max_input_bytes = remaining.max_candidate_bytes;
    remaining.max_validation_bytes = remaining
        .max_validation_bytes
        .checked_sub(resident)
        .ok_or(ApngError::ResourceLimit("validation_budget"))?;
    let mut after =
        validation::inspect_with_guard(bytes, &remaining, guard).map_err(candidate_error)?;
    usage.charge(after.decoded_pixels, after.decoded_bytes, limits.validation)?;
    // 压缩长度/存活量影响估算，不属于动画语义；其余字段保持严格相同。
    after.validation_bound = before.validation_bound;
    if &after != before {
        return Err(ApngError::ValidationFailed);
    }
    guard.check()?;
    Ok(())
}
pub(super) fn candidate_error(error: ApngValidationError) -> ApngError {
    match error.0 {
        "cancelled" | "deadline" | "invalid_limits" => error.into(),
        code if code.ends_with("_budget") => error.into(),
        _ => ApngError::ValidationFailed,
    }
}
fn quantization_error(error: ApngValidationError) -> ApngError {
    match error.0 {
        "unsupported_depth" | "unsupported_interlace" | "unsupported_color" => {
            ApngError::Validation(error)
        }
        "quantizer_memory" => ApngError::ResourceLimit("quantizer_memory"),
        "quantizer" | "encode" | "missing_quality" => ApngError::Quantization(error),
        _ => candidate_error(error),
    }
}
pub(super) fn passes(comparison: &quantization::Comparison, quality: u8, measured: u8) -> bool {
    quantization::meets_quality(comparison, quality, measured)
        && comparison.raw_alpha_class_changes == 0
        && comparison.max_raw_alpha_error <= 4.0
}
pub(super) fn select(
    source: &[u8],
    before: &ApngPlayback,
    lossless: Vec<u8>,
    mode: ApngMode,
    limits: ApngLimits,
    guard: &Guard<'_>,
    usage: &mut Usage,
) -> Result<Selected, ApngError> {
    let Some(parameters) = mode.quantized() else {
        return Ok(Selected {
            bytes: lossless,
            processing: ApngProcessing::Lossless,
            quality: None,
        });
    };
    strict_candidate(
        &lossless,
        before,
        limits,
        usage,
        guard,
        source.len() as u64 + summary_resident(before),
    )?;
    let baseline = source.len().min(lossless.len());
    let candidate = quantization::quantize_with_usage(
        source,
        options(limits, parameters),
        guard,
        usage,
        lossless.capacity() as u64 + summary_resident(before),
        |_| {},
    )
    .map_err(quantization_error)?;
    guard.check()?;
    let measured = candidate.report.minimum_remapping_quality;
    let reason = if !candidate.report.quality_passed
        || !passes(
            &candidate.report.comparison,
            parameters.quality.get(),
            measured,
        ) {
        Some(ApngFallbackReason::QualityBelowTarget)
    } else if !quality::sufficient_gain(baseline, candidate.bytes.len()) {
        Some(ApngFallbackReason::InsufficientGain)
    } else {
        None
    };
    if let Some(reason) = reason {
        return Ok(Selected {
            bytes: lossless,
            processing: ApngProcessing::LosslessFallback { parameters, reason },
            quality: None,
        });
    }
    Ok(Selected {
        quality: Some(ApngQuality::from_comparison(
            &candidate.report.comparison,
            candidate.report.palette_entries,
            measured,
        )),
        bytes: candidate.bytes,
        processing: ApngProcessing::Lossy { parameters },
    })
}
