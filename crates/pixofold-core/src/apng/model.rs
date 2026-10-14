//! APNG专属请求、资源、报告及固定错误分类；不复用PNG有损模式或凭据许可。
use super::{ApngPlayback, ApngValidationError, ApngValidationLimits};
use crate::model::{ByteCount, OutputPolicy, ProcessingError, ProcessingOutcome, ResourceLimits};
use std::{fmt, io, path::PathBuf, time::Duration};

/// 请求级额度。验证字节/像素是累计工作；工作集是保守准入估算，不是RSS硬上限。
#[derive(Debug, Clone, Copy)]
pub struct ApngLimits {
    pub validation: ApngValidationLimits,
    /// 编码/验证/提交阶段的保守工作集估算上限；默认512MiB，只可收紧，非RSS配额。
    pub max_working_set_bytes: ByteCount,
    /// 准备阶段总协作期限，默认30秒；进入共享提交层后不再按此改写结果。
    pub preparation_timeout: Duration,
}
impl Default for ApngLimits {
    fn default() -> Self {
        Self {
            validation: ApngValidationLimits::default(),
            max_working_set_bytes: ByteCount(512 << 20),
            preparation_timeout: Duration::from_secs(30),
        }
    }
}
impl ApngLimits {
    pub(crate) fn check(self) -> Result<(), ApngError> {
        self.validation
            .check()
            .map_err(|_| ApngError::InvalidLimits)?;
        if self.max_working_set_bytes.0 == 0
            || self.max_working_set_bytes.0 > 512 << 20
            || self.preparation_timeout > Duration::from_secs(30)
        {
            return Err(ApngError::InvalidLimits);
        }
        Ok(())
    }
    pub(super) fn resources(self) -> ResourceLimits {
        ResourceLimits {
            max_input_bytes: ByteCount(self.validation.max_input_bytes as u64),
            max_decoded_bytes: ByteCount(self.validation.max_validation_bytes),
            max_pixels: self.validation.max_canvas_pixels,
            max_dimension: self.validation.max_dimension,
        }
    }
    /// 按实际画布收紧准入估算，包含父进程压缩输入/候选/复读、编码过滤缓冲与原生余量。
    /// # Errors
    /// 无效额度、画布超限或估算超过请求工作集时拒绝；调用方必须保持许可直到核心返回。
    pub fn working_set(self, canvas_pixels: u64) -> Result<ByteCount, ApngError> {
        self.check()?;
        if canvas_pixels > self.validation.max_canvas_pixels {
            return Err(ApngError::ResourceLimit("canvas_budget"));
        }
        // O1单线程且不变换表示；按96 bytes/画布像素估算原始/过滤/编码器scratch。
        // 压缩向量容量、元数据副本和原生libdeflate独立计入；验证阶段取二者较大值。
        let compressed = (self
            .validation
            .max_input_bytes
            .max(self.validation.max_candidate_bytes) as u64)
            * 8;
        let summaries = self.validation.max_frames as u64 * 16384;
        let native = canvas_pixels * 96
            + compressed
            + (self.validation.max_metadata_bytes as u64) * 4
            + summaries
            + (32 << 20);
        let validation = self.validation.max_validation_bytes + compressed + summaries + (32 << 20);
        let bound = native.max(validation);
        if bound > self.max_working_set_bytes.0 {
            return Err(ApngError::ResourceLimit("working_set_budget"));
        }
        Ok(ByteCount(bound))
    }
}
/// 冻结的无损请求；只接受内容为APNG的源，扩展名不作为格式证据。
#[derive(Debug, Clone)]
pub struct ApngRequest {
    pub source: PathBuf,
    pub output: OutputPolicy,
    pub limits: ApngLimits,
}
impl ApngRequest {
    /// 默认无损覆盖并保留备份；不执行I/O，参数在实际执行时验证。
    pub fn new(source: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            output: OutputPolicy::default(),
            limits: ApngLimits::default(),
        }
    }
}
/// 输入动画属性，num_plays是总播放次数，0表示无限；不采用GIF额外循环次数定义。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApngInfo {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: u8,
    pub interlaced: bool,
    pub frames: u32,
    pub num_plays: u32,
    pub default_participates: bool,
}
impl From<&ApngPlayback> for ApngInfo {
    fn from(p: &ApngPlayback) -> Self {
        Self {
            width: p.width,
            height: p.height,
            bit_depth: p.depth,
            color_type: p.color,
            interlaced: p.interlace == 1,
            frames: p.frames.len() as u32,
            num_plays: p.plays,
            default_participates: p.default_participates,
        }
    }
}
/// 本阶段仅严格无损，不以量化、降深或静态默认图替代动画。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApngProcessing {
    Lossless,
}
/// 只描述实际文件结果；NoGain的output_bytes等于input_bytes且不创建备份/副本。
#[derive(Debug, Clone)]
pub struct ApngReport {
    pub image: ApngInfo,
    pub processing: ApngProcessing,
    pub input_bytes: ByteCount,
    pub output_bytes: ByteCount,
    pub elapsed: Duration,
    pub outcome: ProcessingOutcome,
}
/// 原始错误仅保留于Rust；Display和诊断只输出受控说明，不打印图片/路径/元数据。
#[derive(Debug)]
pub enum ApngError {
    InvalidLimits,
    ResourceLimit(&'static str),
    Validation(ApngValidationError),
    Encoding(oxipng::PngError),
    ValidationFailed,
    Cancelled,
    Timeout,
    File(ProcessingError),
    Cleanup {
        original: Box<ApngError>,
        source: io::Error,
        temporary: PathBuf,
    },
}
impl From<ProcessingError> for ApngError {
    fn from(error: ProcessingError) -> Self {
        if matches!(error, ProcessingError::Cancelled) {
            Self::Cancelled
        } else {
            Self::File(error)
        }
    }
}
impl From<ApngValidationError> for ApngError {
    fn from(error: ApngValidationError) -> Self {
        match error.0 {
            "cancelled" => Self::Cancelled,
            "deadline" => Self::Timeout,
            "invalid_limits" => Self::InvalidLimits,
            code if code.ends_with("_budget") => Self::ResourceLimit(code),
            "not_equivalent" => Self::ValidationFailed,
            _ => Self::Validation(error),
        }
    }
}
impl fmt::Display for ApngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("APNG资源配置无效"),
            Self::ResourceLimit(code) => write!(f, "APNG资源超限：{code}"),
            Self::Validation(e) => write!(f, "APNG验证拒绝：{}", e.0),
            Self::Encoding(_) => f.write_str("APNG编码失败"),
            Self::ValidationFailed => f.write_str("APNG候选验证失败"),
            Self::Cancelled => f.write_str("APNG处理已取消"),
            Self::Timeout => f.write_str("APNG准备阶段超时"),
            Self::File(e) => e.fmt(f),
            Self::Cleanup { .. } => f.write_str("APNG临时文件清理失败，需检查恢复状态"),
        }
    }
}
impl std::error::Error for ApngError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Validation(e) => Some(e),
            Self::Encoding(e) => Some(e),
            Self::File(e) => Some(e),
            Self::Cleanup { source, .. } => Some(source),
            _ => None,
        }
    }
}
