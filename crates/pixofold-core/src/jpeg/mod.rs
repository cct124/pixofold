//! JPEG单文件无损/保守有损核心；批次通过受信引擎调用，尚未接入桌面。
//! 固定可信原生工具只接收字节，输出层独占文件提交；不复用PNG凭据许可。

mod engine;
mod format;
mod process;
mod quality;

pub use engine::JpegEngine;
pub(crate) use format::probe_header;
#[cfg(test)]
pub(crate) use process::{ProcessEvent, ProcessObserver};
#[cfg(test)]
pub(crate) fn run_process_for_test(
    command: &mut std::process::Command,
    input: &[u8],
    limit: u64,
    keep: bool,
    timeout: Duration,
    cancel: &CancellationToken,
    observer: ProcessObserver,
) -> Result<(), JpegError> {
    process::observe::with(observer, || {
        process::run(command, input, limit, keep, timeout, cancel).map(|_| ())
    })
}
pub use quality::{JpegLossyFallbackReason, JpegMode, JpegProcessing, JpegQualityMapping};

/// 受限预览像素：灰度单通道或RGB三通道，无元数据；orientation为1–8的Exif方向。
/// 不承诺ICC色彩管理，只用于宿主生成缩略图。宿主仍须限制并发和缓存。
pub struct JpegPreview {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub orientation: u8,
    pub pixels: Vec<u8>,
}

/// 用同一可信引擎解码预览，不改写输入或输出文件。
/// # Errors
/// 配置/输入/资源/工具错误、超时或取消返回错误；ICC、四分量和不明确颜色拒绝预览。
/// 取消/错误仍先回收子进程、管道和临时目录再返回，方向由宿主在缩放时应用。
pub fn decode_preview(
    input: &[u8],
    engine: &JpegEngine,
    limits: JpegLimits,
    cancel: &CancellationToken,
) -> Result<JpegPreview, JpegError> {
    limits.validate()?;
    cancel.check()?;
    let parsed = format::inspect(input, limits)?;
    let orientation = parsed.preview_orientation()?;
    let pixels = engine.preview_pixels(input, &parsed, limits, cancel)?;
    cancel.check()?;
    Ok(JpegPreview {
        width: parsed.info.width,
        height: parsed.info.height,
        channels: if parsed.info.components == 1 { 1 } else { 3 },
        orientation,
        pixels,
    })
}

/// 只检查压缩结构/元数据与声明的资源，不调用原生工具或解码像素。
pub(crate) fn inspect_structure(bytes: &[u8], limits: JpegLimits) -> Result<JpegInfo, JpegError> {
    limits.validate()?;
    Ok(format::inspect(bytes, limits)?.info)
}

use crate::{
    model::{
        ByteCount, CancellationToken, OutputPolicy, ProcessingError, ProcessingOutcome,
        ProcessingStage, ResourceLimits,
    },
    output::{self, Destination, Source},
};
use std::{
    fmt, io,
    path::PathBuf,
    time::{Duration, Instant},
};

/// JPEG执行预算；调用者仍需限制同时处理图片数及总工作集。
/// max_decoded_bytes约束系数/像素工作集并收紧原生分配，不是RSS硬限制；最多16M像素/64次扫描。
#[derive(Debug, Clone, Copy)]
pub struct JpegLimits {
    pub resources: ResourceLimits,
    pub max_scans: u32,
    /// 单次原生调用的墙钟超时（每文件最多三次）；允许1ms至120s。
    pub process_timeout: Duration,
}

impl Default for JpegLimits {
    fn default() -> Self {
        Self {
            resources: ResourceLimits::default(),
            max_scans: 64,
            process_timeout: Duration::from_secs(30),
        }
    }
}

impl JpegLimits {
    pub(crate) fn validate(self) -> Result<(), JpegError> {
        self.resources
            .validate()
            .map_err(|_| JpegError::InvalidLimits)?;
        if self.resources.max_input_bytes.0 > 64 * 1024 * 1024
            || !(1024 * 1024..=512 * 1024 * 1024).contains(&self.resources.max_decoded_bytes.0)
            || self.resources.max_pixels > 16 * 1024 * 1024
            || self.resources.max_dimension > 65535
            || !(1..=64).contains(&self.max_scans)
            || !(Duration::from_millis(1)..=Duration::from_secs(120))
                .contains(&self.process_timeout)
        {
            return Err(JpegError::InvalidLimits);
        }
        Ok(())
    }
}

/// JPEG专属请求；默认无损，有损只在已验证的颜色/元数据范围执行，无凭据移除开关。
#[derive(Debug, Clone)]
pub struct JpegRequest {
    pub source: PathBuf,
    pub output: OutputPolicy,
    pub mode: JpegMode,
    pub limits: JpegLimits,
}

impl JpegRequest {
    /// 核心API默认备份后覆盖；不代表桌面设置默认值。
    pub fn new(source: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            output: OutputPolicy::Overwrite,
            mode: JpegMode::Lossless,
            limits: JpegLimits::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JpegInfo {
    pub width: u32,
    pub height: u32,
    pub components: u8,
    pub progressive: bool,
}

#[derive(Debug, Clone)]
pub struct JpegReport {
    pub image: JpegInfo,
    pub processing: JpegProcessing,
    pub input_bytes: ByteCount,
    pub output_bytes: ByteCount,
    pub elapsed: Duration,
    pub outcome: ProcessingOutcome,
}

/// 原始引擎stderr/文件名不进入Display或诊断；文件恢复路径仅通过结构化错误交接。
#[derive(Debug)]
pub enum JpegError {
    InvalidLimits,
    InvalidJpeg(&'static str),
    UnsupportedJpeg,
    ProtectedMetadata(u8),
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
        original: Option<Box<JpegError>>,
        source: io::Error,
        temporary: PathBuf,
    },
}

impl From<ProcessingError> for JpegError {
    fn from(value: ProcessingError) -> Self {
        if matches!(value, ProcessingError::Cancelled) {
            Self::Cancelled
        } else {
            Self::File(value)
        }
    }
}

impl fmt::Display for JpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("JPEG资源配置无效"),
            Self::InvalidJpeg(reason) => write!(f, "JPEG结构无效：{reason}"),
            Self::UnsupportedJpeg => f.write_str("仅支持8位Huffman基线或渐进JPEG"),
            Self::ProtectedMetadata(marker) => write!(
                f,
                "JPEG含暂不支持安全改写的元数据（FF{marker:02X}），未修改原图"
            ),
            Self::ResourceLimit(resource) => write!(f, "JPEG资源超限：{resource}"),
            Self::ToolIdentity => f.write_str("JPEG工具缺失、被替换或身份校验失败"),
            Self::ToolIo { operation, .. } => write!(f, "JPEG工具操作失败：{operation}"),
            Self::ToolExit(_) => f.write_str("JPEG工具拒绝输入或异常退出"),
            Self::Timeout => f.write_str("JPEG处理超时，未提交"),
            Self::Cancelled => f.write_str("JPEG处理已中止，未提交"),
            Self::ValidationFailed => f.write_str("JPEG候选或元数据验证失败，未提交"),
            Self::File(ProcessingError::ValidationFailed(_)) => {
                f.write_str("JPEG文件验证失败，未提交")
            }
            Self::File(error) => write!(f, "JPEG文件处理失败：{error}"),
            Self::Cleanup { .. } => f.write_str("JPEG临时资源清理失败，残留位置已保留"),
        }
    }
}
impl std::error::Error for JpegError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ToolIo { source, .. } | Self::Cleanup { source, .. } => Some(source),
            Self::File(error) => Some(error),
            _ => None,
        }
    }
}

fn io_error(operation: &'static str, source: io::Error) -> JpegError {
    JpegError::ToolIo { operation, source }
}

enum CandidateValidation {
    Coefficients([u8; 32]),
    Color(engine::DecodedColor),
}

/// 单文件优化。无损核对系数；有损完整解码并保持颜色解释/元数据，报告实际路径及回退。
/// 只输出更小且验证通过的结果；无收益不生成备份/副本。
/// 同步调用应放入调用方有界worker；阶段回调须快速返回且不panic。
/// 取消/超时会终结并回收自有原生进程，提交临界区继续沿用输出层取消契约。
///
/// # Errors
/// 不支持/损坏输入、元数据保护、资源超限、工具故障、取消、验证和文件提交均明确失败。
/// 文件提交/清理错误保留恢复上下文，不承诺文件系统CAS或断电事务。
pub fn optimize_jpeg(
    request: &JpegRequest,
    engine: &JpegEngine,
    cancel: &CancellationToken,
    mut on_stage: impl FnMut(ProcessingStage),
) -> Result<JpegReport, JpegError> {
    let started = Instant::now();
    request.limits.validate()?;
    cancel.check()?;
    on_stage(ProcessingStage::Reading);
    cancel.check()?;
    let mut source = Source::read(&request.source, request.limits.resources)?;
    // 备份扩展名保持真实JPEG名称，禁止给PNG伪装成JPEG的输出。
    let extension = source
        .path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if !extension.eq_ignore_ascii_case("jpg") && !extension.eq_ignore_ascii_case("jpeg") {
        return Err(JpegError::UnsupportedJpeg);
    }
    source.use_jpeg_backup_suffix();
    let parsed = format::inspect(&source.bytes, request.limits)?;
    let destination = Destination::plan(&source, &request.output)?;
    on_stage(ProcessingStage::Optimizing);
    let processing = match request.mode {
        JpegMode::Lossless => JpegProcessing::Lossless,
        JpegMode::Lossy { quality } => {
            let parameters = JpegQualityMapping::new(quality);
            match parsed.lossy_fallback() {
                Some(reason) => JpegProcessing::LosslessFallback { parameters, reason },
                None => JpegProcessing::Lossy { parameters },
            }
        }
    };
    let (candidate, validation) = match processing {
        JpegProcessing::Lossy { parameters } => {
            parsed.check_lossy_budget(request.limits)?;
            let color = engine.decoded_color(&source.bytes, &parsed, request.limits, cancel)?;
            (
                engine.reencode(
                    &source.bytes,
                    parameters.native_quality,
                    request.limits,
                    cancel,
                )?,
                CandidateValidation::Color(color),
            )
        }
        _ => {
            let hash = engine.fingerprint(&source.bytes, &parsed, request.limits, cancel)?;
            (
                engine.optimize(&source.bytes, request.limits, cancel)?,
                CandidateValidation::Coefficients(hash),
            )
        }
    };
    cancel.check()?;
    let mut temp = destination.stage()?;
    let validated = (|| {
        output::write_candidate(&mut temp, &candidate, &source)?;
        on_stage(ProcessingStage::Validating);
        cancel.check()?;
        let stored = output::read_candidate(&temp, request.limits.resources)?;
        // 有损无法用源像素相等约束候选；必须绑定刚生成的候选，拒绝落盘后替换为另一合法JPEG。
        if stored != candidate {
            return Err(JpegError::ValidationFailed);
        }
        let output = format::inspect(&stored, request.limits)?;
        if !parsed.same_image_and_metadata(&output) {
            return Err(JpegError::ValidationFailed);
        }
        let matches = match validation {
            CandidateValidation::Coefficients(expected) => {
                expected == engine.fingerprint(&stored, &output, request.limits, cancel)?
            }
            CandidateValidation::Color(expected) => {
                expected == engine.decoded_color(&stored, &output, request.limits, cancel)?
            }
        };
        if !matches {
            return Err(JpegError::ValidationFailed);
        }
        source.verify_unchanged(request.limits.resources)?;
        cancel.check()?;
        Ok(stored)
    })();
    drop(candidate);
    let stored = match validated {
        Ok(bytes) => bytes,
        Err(original) => {
            let temporary = temp.path().to_owned();
            return Err(match temp.close() {
                Ok(()) => original,
                Err(source) => JpegError::Cleanup {
                    original: Some(Box::new(original)),
                    source,
                    temporary,
                },
            });
        }
    };
    let image = parsed.info.clone();
    drop(parsed);
    let input_bytes = ByteCount(source.bytes.len() as u64);
    let output_size = stored.len() as u64;
    let (outcome, output_bytes) = if output_size >= input_bytes.0 {
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
            ByteCount(output_size),
        )
    };
    Ok(JpegReport {
        image,
        processing,
        input_bytes,
        output_bytes,
        elapsed: started.elapsed(),
        outcome,
    })
}
