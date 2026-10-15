//! APNG单文件质量策略：q100严格无损，其余采用固定透明分组色表与实验v2门槛。
use crate::model::QualityValue;
use serde::Serialize;
use super::validation::quantization;

/// 默认严格无损；有损质量100同样直接走无损，不生成量化候选。
#[derive(Debug,Clone,Copy,Default,PartialEq,Eq,Serialize)]
#[serde(tag="kind",rename_all="snake_case")]
pub enum ApngMode {
    #[default]
    Lossless,
    Lossy {quality:QualityValue},
}
impl ApngMode {
    pub(super) fn quantized(self)->Option<ApngQualityMapping> {
        match self {Self::Lossy {quality} if quality.get()<100=>Some(ApngQualityMapping::new(quality)),_=>None}
    }
}
/// 冻结映射v1：alpha_palette，无抖动，质量门槛版本2；质量值不跨格式承诺相同观感。
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ApngQualityMapping {
    pub version:u32,
    pub quality:QualityValue,
    pub policy_version:u32,
    pub dithering:bool,
}
impl ApngQualityMapping {
    /// 只记录确定性参数；不会读取文件或执行编码。
    pub fn new(quality:QualityValue)->Self {
        Self {version:1,quality,policy_version:quantization::POLICY_VERSION,dithering:false}
    }
}
/// 只有质量或额外收益不足允许无损回退；不支持表示、硬语义和工具错误仍失败。
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum ApngFallbackReason {QualityBelowTarget,InsufficientGain}

/// 仅最终通过且写出的量化候选提供质量摘要；回退和NoGain不暴露淘汰候选指标。
#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ApngQuality {
    pub palette_entries:usize,
    pub minimum_pixel_quality:u8,
    pub minimum_remapping_quality:u8,
    pub max_alpha_error:f64,
    pub worst_frame_rmse:f64,
    pub worst_temporal_rmse:f64,
}
impl ApngQuality {
    pub(super) fn from_comparison(c:&quantization::Comparison,palette_entries:usize,minimum_remapping_quality:u8)->Self {
        Self {palette_entries,minimum_pixel_quality:c.minimum_pixel_quality,minimum_remapping_quality,max_alpha_error:c.max_alpha_error,
            worst_frame_rmse:c.black.worst_frame_rmse.max(c.white.worst_frame_rmse),
            worst_temporal_rmse:c.black.worst_temporal_rmse.max(c.white.worst_temporal_rmse)}
    }
}
pub(super) fn sufficient_gain(baseline:usize,candidate:usize)->bool {
    baseline.checked_sub(candidate).is_some_and(|saved|saved>=1024 && saved>=baseline.div_ceil(100))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mappings_and_gain_boundaries_are_exact() {
        for q in 0..=100 {let quality=QualityValue::new(q).unwrap();let mode=ApngMode::Lossy{quality};
            assert_eq!(mode.quantized().is_none(),q==100);
            let mapping=ApngQualityMapping::new(quality);assert_eq!(mapping.version,1);assert_eq!(mapping.policy_version,2);assert!(!mapping.dithering);
        }
        assert!(!sufficient_gain(2048,1025));assert!(sufficient_gain(2048,1024));
        assert!(!sufficient_gain(200000,198001));assert!(sufficient_gain(200000,198000));
        assert!(!sufficient_gain(1,2));assert!(sufficient_gain(usize::MAX,0));
    }
}
