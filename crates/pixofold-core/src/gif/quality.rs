//! GIF v1保守质量策略：先保持动画硬语义，再比较质量与相对无损基准的额外收益。
use super::{GifComparison, GifError, GifQualityMetrics};
use crate::model::QualityValue;
use serde::Serialize;

/// 单文件默认为无损；有损q100明确选择严格无损。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GifMode {
    #[default]
    Lossless,
    Lossy {
        quality: QualityValue,
    },
}

/// v1强度ceil((100-q)/2)，最大20；q0–60同为20，q100为0且不运行有损编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GifQualityMapping {
    pub version: u32,
    pub quality: QualityValue,
    pub native_lossiness: u16,
}
impl GifQualityMapping {
    /// 仅映射已验证的0–100输入，不执行I/O；不同格式的q不承诺相同观感。
    pub fn new(quality: QualityValue) -> Self {
        Self {
            version: 1,
            quality,
            native_lossiness: u16::from(100 - quality.get()).div_ceil(2).min(20),
        }
    }
}

/// 仅质量或额外收益不足允许回退；语义、工具、资源和取消错误仍明确失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GifFallbackReason {
    QualityBelowTarget,
    InsufficientGain,
}

/// 实际选定候选的方式；NoGain为独立outcome，表示没有提交候选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GifProcessing {
    Lossless,
    Lossy {
        parameters: GifQualityMapping,
    },
    LosslessFallback {
        parameters: GifQualityMapping,
        reason: GifFallbackReason,
    },
}

fn within(m: &GifQualityMetrics) -> bool {
    [
        m.rmse,
        m.worst_interval_rmse,
        m.temporal_error_rmse,
        m.worst_temporal_rmse,
        m.changed_pixel_fraction,
    ]
    .iter()
    .all(|n| n.is_finite() && *n >= 0.0)
        && m.rmse <= 2.5
        && m.worst_interval_rmse <= 4.0
        && m.max_channel_error <= 64
        && m.temporal_error_rmse <= 3.0
        && m.worst_temporal_rmse <= 6.0
        && m.changed_pixel_fraction <= 1.0
}

/// 质量门槛使用RGB编码误差，不等于感知评分；硬约束失败永远不能通过。
pub fn meets_quality(comparison: &GifComparison) -> bool {
    comparison.hard_constraints
        && comparison.rejection.is_none()
        && comparison.transparent.as_ref().is_some_and(within)
        && comparison.logical.as_ref().is_some_and(within)
}

/// 相对min(源大小,有效无损候选)至少节省1%且不少于1024 bytes，使用整数避免边界舍入。
pub fn sufficient_gain(baseline_bytes: u64, candidate_bytes: u64) -> bool {
    baseline_bytes
        .checked_sub(candidate_bytes)
        .is_some_and(|gain| gain >= 1024 && gain >= baseline_bytes.div_ceil(100))
}

pub(super) fn decide(
    comparison: &GifComparison,
    baseline: u64,
    candidate: u64,
    parameters: GifQualityMapping,
) -> Result<GifProcessing, GifError> {
    if !comparison.hard_constraints || comparison.rejection.is_some() {
        return Err(GifError::ValidationFailed);
    }
    if comparison.lossless_equivalent {
        return Ok(GifProcessing::Lossless);
    }
    let reason = if !meets_quality(comparison) {
        Some(GifFallbackReason::QualityBelowTarget)
    } else if !sufficient_gain(baseline, candidate) {
        Some(GifFallbackReason::InsufficientGain)
    } else {
        None
    };
    Ok(match reason {
        Some(reason) => GifProcessing::LosslessFallback { parameters, reason },
        None => GifProcessing::Lossy { parameters },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_quality_values_are_bounded_monotone_and_anchors_exact() {
        let mut previous = 20;
        for q in 0..=100 {
            let mapping = GifQualityMapping::new(QualityValue::new(q).unwrap());
            assert_eq!(mapping.version, 1);
            assert!(mapping.native_lossiness <= previous);
            assert_eq!(mapping.native_lossiness == 0, q == 100);
            previous = mapping.native_lossiness;
        }
        for (q, expected) in [(0, 20), (60, 20), (80, 10), (99, 1), (100, 0)] {
            assert_eq!(
                GifQualityMapping::new(QualityValue::new(q).unwrap()).native_lossiness,
                expected
            );
        }
        assert!(QualityValue::new(-1).is_err());
        assert!(QualityValue::new(101).is_err());
    }
    #[test]
    fn gain_requires_both_absolute_and_relative_savings_without_overflow() {
        assert!(!sufficient_gain(88976, 88948));
        assert!(!sufficient_gain(200000, 198976));
        assert!(sufficient_gain(200000, 198000));
        assert!(!sufficient_gain(2048, 1025));
        assert!(sufficient_gain(2048, 1024));
        assert!(!sufficient_gain(1, 2));
        assert!(sufficient_gain(u64::MAX, 0));
    }
}
