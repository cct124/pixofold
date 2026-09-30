//! JPEG单文件无损核心（J1a），尚未接入桌面或批次。
//! 固定可信原生工具只接收字节，输出层独占文件提交；不复用PNG凭据许可。

mod engine;
mod format;
mod process;

pub use engine::JpegEngine;

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
/// max_decoded_bytes同时收紧原生系数分配，不是RSS硬限制；最多16M像素/64次扫描。
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
    fn validate(self) -> Result<(), JpegError> {
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

/// 仅无损，没有质量参数、凭据移除开关或自动回退有损路径。
#[derive(Debug, Clone)]
pub struct JpegRequest {
    pub source: PathBuf,
    pub output: OutputPolicy,
    pub limits: JpegLimits,
}

impl JpegRequest {
    /// 核心API默认备份后覆盖；不代表桌面设置默认值。
    pub fn new(source: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            output: OutputPolicy::Overwrite,
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

#[derive(Debug)]
pub struct JpegReport {
    pub image: JpegInfo,
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
            Self::ValidationFailed => f.write_str("JPEG无损或元数据验证失败，未提交"),
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

/// 单文件系数无损优化。只输出更小且验证通过的结果；无收益不生成备份/副本。
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
    let fingerprint = engine.fingerprint(&source.bytes, &parsed, request.limits, cancel)?;
    let candidate = engine.optimize(&source.bytes, request.limits, cancel)?;
    cancel.check()?;
    let mut temp = destination.stage()?;
    let validated = (|| {
        output::write_candidate(&mut temp, &candidate, &source)?;
        on_stage(ProcessingStage::Validating);
        cancel.check()?;
        let stored = output::read_candidate(&temp, request.limits.resources)?;
        let output = format::inspect(&stored, request.limits)?;
        if !parsed.same_image_and_metadata(&output)
            || fingerprint != engine.fingerprint(&stored, &output, request.limits, cancel)?
        {
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
        input_bytes,
        output_bytes,
        elapsed: started.elapsed(),
        outcome,
    })
}
