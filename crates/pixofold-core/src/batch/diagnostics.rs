//! 日志仅抽取冻结参数和最终报告中的受控分类，不携带路径、候选指标或元数据。
use super::{CompressionMode, ImageKind, ImageReport, ImageRequest};
use crate::{
    gif::{GifFallbackReason, GifProcessing},
    jpeg::{JpegLossyFallbackReason, JpegProcessing},
    model::{LossyFallbackReason, PngProcessing},
};

impl ImageRequest {
    pub(super) fn log_format(&self) -> &'static str {
        match self.format() {
            ImageKind::Png => "png",
            ImageKind::Jpeg => "jpeg",
            ImageKind::Gif => "gif",
            ImageKind::Apng => "apng",
        }
    }
    pub(super) fn log_mode(&self) -> &'static str {
        match self.mode {
            CompressionMode::Lossless => "lossless",
            CompressionMode::Lossy { .. } => "lossy",
        }
    }
    pub(super) fn log_quality(&self) -> Option<u64> {
        match self.mode {
            CompressionMode::Lossless => None,
            CompressionMode::Lossy { quality } => Some(u64::from(quality.get())),
        }
    }
}

impl ImageReport {
    /// processing描述选定候选；只有result=succeeded才意味着它已写入最终文件。
    pub(super) fn log_processing(&self) -> (&'static str, &'static str) {
        match self {
            Self::Png(r) => png(r.processing),
            Self::Jpeg(r) => jpeg(r.processing),
            Self::Gif(r) => gif(r.processing),
            Self::Apng(r) => match r.processing {
                crate::apng::ApngProcessing::Lossless=>("lossless","none"),
                crate::apng::ApngProcessing::Lossy {..}=>("lossy","none"),
                crate::apng::ApngProcessing::LosslessFallback {reason,..}=>("lossless_fallback",match reason {
                    crate::apng::ApngFallbackReason::QualityBelowTarget=>"quality_below_target",
                    crate::apng::ApngFallbackReason::InsufficientGain=>"insufficient_gain",
                }),
            },
        }
    }
}
fn png(p: PngProcessing) -> (&'static str, &'static str) {
    match p {
        PngProcessing::Lossless => ("lossless", "none"),
        PngProcessing::Lossy { .. } => ("lossy", "none"),
        PngProcessing::LosslessFallback { reason, .. } => (
            "lossless_fallback",
            match reason {
                LossyFallbackReason::HighBitDepth => "high_bit_depth",
                LossyFallbackReason::ColorMetadata => "color_metadata",
                LossyFallbackReason::RepresentationMetadata => "representation_metadata",
                LossyFallbackReason::QualityBelowTarget { .. } => "quality_below_target",
                LossyFallbackReason::TransparencyGuard => "transparency_guard",
                LossyFallbackReason::NoSizeBenefit => "no_size_benefit",
            },
        ),
    }
}
fn jpeg(p: JpegProcessing) -> (&'static str, &'static str) {
    match p {
        JpegProcessing::Lossless => ("lossless", "none"),
        JpegProcessing::Lossy { .. } => ("lossy", "none"),
        JpegProcessing::LosslessFallback { reason, .. } => (
            "lossless_fallback",
            match reason {
                JpegLossyFallbackReason::ColorProfile => "color_profile",
                JpegLossyFallbackReason::FourComponentColor => "four_component_color",
                JpegLossyFallbackReason::EmbeddedThumbnail => "embedded_thumbnail",
                JpegLossyFallbackReason::AmbiguousColor => "ambiguous_color",
            },
        ),
    }
}
fn gif(p: GifProcessing) -> (&'static str, &'static str) {
    match p {
        GifProcessing::Lossless => ("lossless", "none"),
        GifProcessing::Lossy { .. } => ("lossy", "none"),
        GifProcessing::LosslessFallback { reason, .. } => (
            "lossless_fallback",
            match reason {
                GifFallbackReason::QualityBelowTarget => "quality_below_target",
                GifFallbackReason::InsufficientGain => "insufficient_gain",
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gif::GifQualityMapping, jpeg::JpegQualityMapping, model::*};

    #[test]
    fn actual_paths_are_independent_of_requested_quality_and_rejected_metrics() {
        let quality = QualityValue::new(100).unwrap();
        let mut request: ImageRequest = PngRequest::new("PRIVATE.png").into();
        assert_eq!(request.log_quality(), None);
        request.mode = CompressionMode::Lossy { quality };
        assert_eq!(request.log_quality(), Some(100));
        assert_eq!(request.log_mode(), "lossy");
        assert_eq!(request.log_format(), "png");
        request.options = super::super::FormatOptions::Jpeg(Default::default());
        assert_eq!(request.log_format(), "jpeg");
        request.options = super::super::FormatOptions::Gif(Default::default());
        assert_eq!(request.log_format(), "gif");
        assert_eq!(png(PngProcessing::Lossless), ("lossless", "none"));
        assert_eq!(jpeg(JpegProcessing::Lossless), ("lossless", "none"));
        assert_eq!(gif(GifProcessing::Lossless), ("lossless", "none"));
        assert_eq!(
            jpeg(JpegProcessing::Lossy {
                parameters: JpegQualityMapping::new(quality)
            }),
            ("lossy", "none")
        );
        let parameters = GifQualityMapping::new(QualityValue::new(80).unwrap());
        assert_eq!(gif(GifProcessing::Lossy { parameters }), ("lossy", "none"));
        for (reason, label) in [
            (
                GifFallbackReason::QualityBelowTarget,
                "quality_below_target",
            ),
            (GifFallbackReason::InsufficientGain, "insufficient_gain"),
        ] {
            assert_eq!(
                gif(GifProcessing::LosslessFallback { parameters, reason }),
                ("lossless_fallback", label)
            );
        }
        let mapping = PngQualityMapping {
            version: 1,
            quality,
            minimum: 80,
            target: 100,
        };
        assert_eq!(
            png(PngProcessing::Lossy {
                mapping,
                measured_quality: 99
            }),
            ("lossy", "none")
        );
        for (reason, label) in [
            (LossyFallbackReason::HighBitDepth, "high_bit_depth"),
            (LossyFallbackReason::ColorMetadata, "color_metadata"),
            (
                LossyFallbackReason::RepresentationMetadata,
                "representation_metadata",
            ),
            (
                LossyFallbackReason::QualityBelowTarget { measured: Some(25) },
                "quality_below_target",
            ),
            (LossyFallbackReason::TransparencyGuard, "transparency_guard"),
            (LossyFallbackReason::NoSizeBenefit, "no_size_benefit"),
        ] {
            assert_eq!(
                png(PngProcessing::LosslessFallback { mapping, reason }),
                ("lossless_fallback", label)
            );
        }
        let parameters = JpegQualityMapping::new(quality);
        for (reason, label) in [
            (JpegLossyFallbackReason::ColorProfile, "color_profile"),
            (
                JpegLossyFallbackReason::FourComponentColor,
                "four_component_color",
            ),
            (
                JpegLossyFallbackReason::EmbeddedThumbnail,
                "embedded_thumbnail",
            ),
            (JpegLossyFallbackReason::AmbiguousColor, "ambiguous_color"),
        ] {
            assert_eq!(
                jpeg(JpegProcessing::LosslessFallback { parameters, reason }),
                ("lossless_fallback", label)
            );
        }
    }
}
