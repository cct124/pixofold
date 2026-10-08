//! JPEG专属模式与版本化质量映射；不把质量100解释为系数无损。

use crate::model::QualityValue;

/// 单文件核心默认无损；产品有损默认质量由调用方通过QualityValue::default()选择。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JpegMode {
    #[default]
    Lossless,
    Lossy {
        quality: QualityValue,
    },
}

/// v1使用固定MozJPEG的FASTEST配置、原颜色/采样及优化Huffman；原生质量为1–100。
/// 输入0明确映射到1，100仍重编码；不承诺与PNG相同数值具有相同观感。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JpegQualityMapping {
    pub version: u32,
    pub quality: QualityValue,
    pub native_quality: u8,
}

impl JpegQualityMapping {
    /// 从已校验的0–100输入生成版本1参数；不做I/O或颜色转换。
    pub fn new(quality: QualityValue) -> Self {
        Self {
            version: 1,
            quality,
            native_quality: quality.get().max(1),
        }
    }
}

/// 只在已有无损验证器支持的范围内回退；受保护/未知元数据仍作为错误拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JpegLossyFallbackReason {
    ColorProfile,
    FourComponentColor,
    EmbeddedThumbnail,
    AmbiguousColor,
}

/// 实际候选的处理方式；只有outcome为Optimized才提交该候选，NoGain保留原图。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JpegProcessing {
    Lossless,
    Lossy {
        parameters: JpegQualityMapping,
    },
    LosslessFallback {
        parameters: JpegQualityMapping,
        reason: JpegLossyFallbackReason,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_mapping_is_bounded_and_one_hundred_remains_lossy() {
        for q in 0..=100 {
            let quality = QualityValue::new(q).unwrap();
            let parameters = JpegQualityMapping::new(quality);
            assert_eq!(parameters.version, 1);
            assert_eq!(parameters.quality.get(), q as u8);
            assert_eq!(parameters.native_quality, (q as u8).max(1));
        }
        assert_eq!(JpegMode::default(), JpegMode::Lossless);
        assert_eq!(
            JpegQualityMapping::new(QualityValue::default()).native_quality,
            80
        );
        assert!(QualityValue::new(-1).is_err());
        assert!(QualityValue::new(101).is_err());
    }
}
