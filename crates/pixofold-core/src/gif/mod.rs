//! GIF无损核心：严格子集结构/独立播放验证、可信有界工具和安全输出提交。
//! 核心只处理可信Rust宿主请求；GIF有损/桌面IPC和预览由后续阶段分别接入。
mod engine;
mod playback;
mod process;
mod structure;
use crate::{
    model::*,
    output::{self, Destination, Source},
};
pub use engine::GifEngine;
pub use playback::{Event as GifEvent, Playback as GifPlayback, normalize as normalize_timeline};
use std::{
    fmt, io,
    path::PathBuf,
    time::{Duration, Instant},
};
pub use structure::{
    Code as GifValidationCode, LabError as GifValidationError, Limits as GifValidationLimits,
};

/// GIF执行额度；累计解码为双背景/代表轮次工作量，resources.max_decoded_bytes约束验证缓冲。
/// 原生分配单独限制并计入预约；不是操作系统RSS硬限制。所有调用保持单线程编码。
#[derive(Debug, Clone, Copy)]
pub struct GifLimits {
    pub resources: ResourceLimits,
    pub validation: GifValidationLimits,
    pub max_native_bytes: ByteCount,
    pub process_timeout: Duration,
    pub validation_timeout: Duration,
}
impl Default for GifLimits {
    fn default() -> Self {
        Self {
            resources: ResourceLimits {
                max_input_bytes: ByteCount(16 * 1024 * 1024),
                max_decoded_bytes: ByteCount(64 * 1024 * 1024),
                max_pixels: 4 * 1024 * 1024,
                max_dimension: 65535,
            },
            validation: GifValidationLimits::default(),
            max_native_bytes: ByteCount(32 * 1024 * 1024),
            process_timeout: Duration::from_secs(30),
            validation_timeout: Duration::from_secs(10),
        }
    }
}
impl GifLimits {
    pub(crate) fn validate(self) -> Result<(), GifError> {
        self.resources
            .validate()
            .map_err(|_| GifError::InvalidLimits)?;
        let maximum = GifValidationLimits::default();
        let configured = self.validation;
        for (value, limit) in [
            (configured.max_input_bytes, maximum.max_input_bytes),
            (configured.max_canvas_pixels, maximum.max_canvas_pixels),
            (configured.max_frame_pixels, maximum.max_frame_pixels),
            (configured.max_frames, maximum.max_frames),
            (
                configured.max_total_decoded_bytes,
                maximum.max_total_decoded_bytes,
            ),
            (configured.max_metadata_bytes, maximum.max_metadata_bytes),
        ] {
            if value == 0 || value > limit {
                return Err(GifError::InvalidLimits);
            }
        }
        if self.resources.max_input_bytes.0 > maximum.max_input_bytes as u64
            || self.resources.max_pixels > maximum.max_canvas_pixels as u64
            || self.resources.max_dimension > 65535
            || self.resources.max_decoded_bytes.0 > 512 * 1024 * 1024
            || self.max_native_bytes.0 == 0
            || self.max_native_bytes.0 > 256 * 1024 * 1024
            || !(Duration::from_millis(1)..=Duration::from_secs(120))
                .contains(&self.process_timeout)
            || !(Duration::from_millis(1)..=Duration::from_secs(120))
                .contains(&self.validation_timeout)
        {
            return Err(GifError::InvalidLimits);
        }
        Ok(())
    }
    fn validation_limits(self) -> GifValidationLimits {
        GifValidationLimits {
            max_input_bytes: self
                .validation
                .max_input_bytes
                .min(self.resources.max_input_bytes.0 as usize),
            max_canvas_pixels: self
                .validation
                .max_canvas_pixels
                .min(self.resources.max_pixels as usize),
            max_frame_pixels: self
                .validation
                .max_frame_pixels
                .min(self.resources.max_pixels as usize),
            ..self.validation
        }
    }
    /// 返回保守单图预约：八份输入/候选/复查容量、验证缓冲、原生分配及32MiB系统余量。
    /// # Errors
    /// 参数无效或计算溢出时拒绝；调用方还须限制同时处理文件数。
    pub fn working_set(self) -> Result<ByteCount, GifError> {
        self.validate()?;
        let total = self
            .resources
            .max_input_bytes
            .0
            .checked_mul(8)
            .and_then(|n| n.checked_add(self.resources.max_decoded_bytes.0))
            .and_then(|n| n.checked_add(self.max_native_bytes.0))
            .and_then(|n| n.checked_add(32 * 1024 * 1024))
            .ok_or(GifError::ResourceLimit("GIF预约溢出"))?;
        Ok(ByteCount(total))
    }
}
/// 静态/动画结构信息，循环字段保持无扩展/有限/无限的GIF原始语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifInfo {
    pub width: u16,
    pub height: u16,
    pub frames: usize,
    pub loop_count: Option<u16>,
    pub total_decoded_bytes: ByteCount,
}
/// 单文件GIF无损请求；默认可恢复备份覆盖，桌面默认设置另由宿主确定。
#[derive(Debug, Clone)]
pub struct GifRequest {
    pub source: PathBuf,
    pub output: OutputPolicy,
    pub limits: GifLimits,
}
impl GifRequest {
    /// 使用默认无损执行额度和备份覆盖策略，不发起I/O。
    pub fn new(source: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            output: OutputPolicy::Overwrite,
            limits: GifLimits::default(),
        }
    }
}
/// 已验证的GIF无损结果；NoGain仍返回原始大小，路径仅供可信Rust调用方使用。
#[derive(Debug, Clone)]
pub struct GifReport {
    pub image: GifInfo,
    pub input_bytes: ByteCount,
    pub output_bytes: ByteCount,
    pub elapsed: Duration,
    pub outcome: ProcessingOutcome,
}
/// 结构、资源、工具和文件错误各自保留；Display不输出原始stderr、评论或私人路径。
#[derive(Debug)]
pub enum GifError {
    InvalidLimits,
    UnsupportedGif,
    UnsupportedMode,
    Validation(GifValidationError),
    ResourceLimit(&'static str),
    ToolIdentity,
    ToolIo {
        operation: &'static str,
        source: io::Error,
    },
    ToolExit(Option<i32>),
    Timeout,
    Cancelled,
    ValidationFailed,
    File(ProcessingError),
    Cleanup {
        original: Option<Box<GifError>>,
        source: io::Error,
        temporary: PathBuf,
    },
}
impl From<ProcessingError> for GifError {
    fn from(error: ProcessingError) -> Self {
        if matches!(error, ProcessingError::Cancelled) {
            Self::Cancelled
        } else {
            Self::File(error)
        }
    }
}
impl From<GifValidationError> for GifError {
    fn from(error: GifValidationError) -> Self {
        match error.code {
            GifValidationCode::Cancelled => Self::Cancelled,
            GifValidationCode::TimedOut => Self::Timeout,
            GifValidationCode::ResourceLimit => Self::ResourceLimit(error.reason),
            _ => Self::Validation(error),
        }
    }
}
impl fmt::Display for GifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("GIF资源配置无效"),
            Self::UnsupportedGif => f.write_str("GIF格式或扩展名暂不支持"),
            Self::UnsupportedMode => f.write_str("GIF当前仅支持无损模式"),
            Self::Validation(error) => write!(f, "GIF验证拒绝：{error}"),
            Self::ResourceLimit(reason) => write!(f, "GIF资源超限：{reason}"),
            Self::ToolIdentity => f.write_str("GIF工具缺失、被替换或身份不符"),
            Self::ToolIo { operation, .. } => write!(f, "GIF工具操作失败：{operation}"),
            Self::ToolExit(_) => f.write_str("GIF工具异常退出"),
            Self::Timeout => f.write_str("GIF处理超时"),
            Self::Cancelled => f.write_str("GIF处理已取消"),
            Self::ValidationFailed => f.write_str("GIF候选验证失败"),
            Self::File(error) => error.fmt(f),
            Self::Cleanup { .. } => f.write_str("GIF临时产物清理失败，需检查恢复状态"),
        }
    }
}
impl std::error::Error for GifError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Validation(error) => Some(error),
            Self::File(error) => Some(error),
            Self::ToolIo { source, .. } | Self::Cleanup { source, .. } => Some(source),
            _ => None,
        }
    }
}
fn io_error(operation: &'static str, source: io::Error) -> GifError {
    GifError::ToolIo { operation, source }
}
struct Work<'a> {
    cancel: &'a CancellationToken,
    deadline: Instant,
}
impl Work<'_> {
    fn check(&self) -> structure::Result<()> {
        structure::check(
            !self.cancel.is_cancelled(),
            GifValidationCode::Cancelled,
            "GIF验证取消",
        )?;
        structure::check(
            Instant::now() < self.deadline,
            GifValidationCode::TimedOut,
            "GIF验证期限",
        )
    }
}
/// 有界独立解码/合成，返回时间轴摘要和评论哈希；不修改文件、不调用编码器。
/// # Errors
/// 不支持/损坏/超限、取消或期限耗尽时明确拒绝。期限在有界解码/合成片段检查。
pub fn validate_gif(
    bytes: &[u8],
    limits: GifLimits,
    cancel: &CancellationToken,
) -> Result<GifPlayback, GifError> {
    limits.validate()?;
    let work = Work {
        cancel,
        deadline: Instant::now() + limits.validation_timeout,
    };
    let report = playback::inspect(
        bytes,
        limits.validation_limits(),
        limits.resources.max_decoded_bytes.0,
        limits.resources.max_dimension,
        &work,
    )?;
    if u32::from(report.width.max(report.height)) > limits.resources.max_dimension {
        return Err(GifError::ResourceLimit("GIF单边尺寸"));
    }
    work.check()?;
    Ok(report)
}
pub(crate) fn inspect_structure(
    bytes: &[u8],
    limits: GifLimits,
    cancel: &CancellationToken,
) -> Result<GifInfo, GifError> {
    limits.validate()?;
    let work = Work {
        cancel,
        deadline: Instant::now() + limits.validation_timeout,
    };
    let parsed = structure::parse(bytes, limits.validation_limits(), &work)?;
    if u32::from(parsed.width.max(parsed.height)) > limits.resources.max_dimension {
        return Err(GifError::ResourceLimit("GIF单边尺寸"));
    }
    Ok(GifInfo {
        width: parsed.width,
        height: parsed.height,
        frames: parsed.frames.len(),
        loop_count: parsed.loop_count,
        total_decoded_bytes: ByteCount(parsed.decoded_bytes as u64),
    })
}
/// 优化GIF并独立验证落盘候选，再通过共享输出层提交；NoGain不创建备份或修改原图。
/// # Errors
/// 错误/取消/超时保留原图，清理失败保留恢复上下文。输出身份检查不承诺文件系统CAS。
pub fn optimize_gif(
    request: &GifRequest,
    engine: &GifEngine,
    cancel: &CancellationToken,
    mut on_stage: impl FnMut(ProcessingStage),
) -> Result<GifReport, GifError> {
    let started = Instant::now();
    request.limits.validate()?;
    cancel.check()?;
    on_stage(ProcessingStage::Reading);
    cancel.check()?;
    let mut source = Source::read(&request.source, request.limits.resources)?;
    if !source
        .path
        .extension()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("gif"))
    {
        return Err(GifError::UnsupportedGif);
    }
    source.use_gif_backup_suffix();
    let image = inspect_structure(&source.bytes, request.limits, cancel)?;
    let before = validate_gif(&source.bytes, request.limits, cancel)?;
    let destination = Destination::plan(&source, &request.output)?;
    on_stage(ProcessingStage::Optimizing);
    cancel.check()?;
    let candidate = engine.optimize(&source.bytes, request.limits, cancel)?;
    cancel.check()?;
    let mut temp = destination.stage()?;
    let validated = (|| {
        output::write_candidate(&mut temp, &candidate, &source)?;
        on_stage(ProcessingStage::Validating);
        cancel.check()?;
        let stored = output::read_candidate(&temp, request.limits.resources)?;
        if stored != candidate {
            return Err(GifError::ValidationFailed);
        }
        let after = validate_gif(&stored, request.limits, cancel)?;
        if !before.equivalent(&after) {
            return Err(GifError::ValidationFailed);
        }
        source.verify_unchanged(request.limits.resources)?;
        cancel.check()?;
        Ok(stored)
    })();
    drop(candidate);
    let stored = match validated {
        Ok(stored) => stored,
        Err(original) => {
            let temporary = temp.path().to_owned();
            return Err(match temp.close() {
                Ok(()) => original,
                Err(source) => GifError::Cleanup {
                    original: Some(Box::new(original)),
                    source,
                    temporary,
                },
            });
        }
    };
    let input_bytes = ByteCount(source.bytes.len() as u64);
    let size = stored.len() as u64;
    let (outcome, output_bytes) = if size >= input_bytes.0 {
        output::discard_no_gain(temp)?;
        (ProcessingOutcome::NoGain, input_bytes)
    } else {
        (
            output::commit(
                temp,
                &stored,
                &destination,
                source,
                request.limits.resources,
                cancel,
                &mut on_stage,
            )?,
            ByteCount(size),
        )
    };
    Ok(GifReport {
        image,
        input_bytes,
        output_bytes,
        elapsed: started.elapsed(),
        outcome,
    })
}
