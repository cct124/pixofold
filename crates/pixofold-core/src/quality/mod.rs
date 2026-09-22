//! 版本化纯质量映射。速度与抖动由适配器独立固定，不由 q 隐式修改。

use crate::model::{PngQualityMapping, QualityValue};

/// PNG 映射版本 1：imagequant minimum=0、target=q。
pub const PNG_QUALITY_MAPPING_VERSION: u32 = 1;

/// 解析已经过边界校验的质量值，无 I/O、无副作用。
/// 原生最低阈值为 0；最终候选还须通过实际 remapping 评分和透明度保护。
pub fn png_quality(quality: QualityValue) -> PngQualityMapping {
    PngQualityMapping {
        version: PNG_QUALITY_MAPPING_VERSION,
        quality,
        minimum: 0,
        target: quality.get(),
    }
}
