//! 质量输入契约：0–100 整数与判别模式；不把 native 参数或 UI 档位作为第二份 q。

use std::fmt;

use serde::{Deserialize, Serialize};

/// 有损质量尺度，合法范围 0–100；100 也不等同于严格像素无损。
/// 反序列化与显式构造都执行范围检查；默认 80。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub struct QualityValue(u8);

impl<'de> Deserialize<'de> for QualityValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(i32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl QualityValue {
    /// 验证 0–100 整数；不截断、钳制或接受负数。
    ///
    /// # Errors
    /// 超出范围返回 InvalidQuality，不改变调用方状态。
    pub fn new(value: i32) -> Result<Self, InvalidQuality> {
        Self::try_from(value)
    }

    /// 返回已经验证的整数值。
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for QualityValue {
    fn default() -> Self {
        Self(80)
    }
}

impl TryFrom<i32> for QualityValue {
    type Error = InvalidQuality;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if (0..=100).contains(&value) {
            Ok(Self(value as u8))
        } else {
            Err(InvalidQuality(value))
        }
    }
}

impl From<QualityValue> for u8 {
    fn from(value: QualityValue) -> Self {
        value.0
    }
}

/// 非法质量参数，保留原始整数以便边界诊断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidQuality(pub i32);

impl fmt::Display for InvalidQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "质量必须是 0–100 整数，收到 {}", self.0)
    }
}

impl std::error::Error for InvalidQuality {}

/// PNG 模式。无损不携带质量；拒绝额外字段，避免配置中残留的 q 被误用。
/// 开发 API 默认无损以兼容已有调用；产品 UI 默认有损 80 尚待桌面接入。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub enum PngMode {
    #[default]
    Lossless,
    Lossy {
        quality: QualityValue,
    },
}

impl<'de> Deserialize<'de> for PngMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // 严格读取规则独立于 TS 静态类型，避免 ts-rs 无法表达 deny_unknown_fields。
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
        enum WireMode {
            Lossless {},
            Lossy { quality: QualityValue },
        }
        Ok(match WireMode::deserialize(deserializer)? {
            WireMode::Lossless {} => Self::Lossless,
            WireMode::Lossy { quality } => Self::Lossy { quality },
        })
    }
}

/// 已解析的 PNG 质量参数快照，不作为外部可传入的 native 选项使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngQualityMapping {
    pub version: u32,
    pub quality: QualityValue,
    pub minimum: u8,
    pub target: u8,
}

/// 有损没有被采纳的可识别原因；结果仍必须经过严格无损验证和收益检查。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossyFallbackReason {
    HighBitDepth,
    ColorMetadata,
    RepresentationMetadata,
    QualityBelowTarget { measured: Option<u8> },
    TransparencyGuard,
    NoSizeBenefit,
}

/// 实际采用的处理路径，而不是用户请求模式的副本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PngProcessing {
    Lossless,
    Lossy {
        mapping: PngQualityMapping,
        measured_quality: u8,
    },
    LosslessFallback {
        mapping: PngQualityMapping,
        reason: LossyFallbackReason,
    },
}
