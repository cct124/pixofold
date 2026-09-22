//! 单文件处理契约；无损/有损参数分离，不引入队列状态。

use super::{PngMode, PngProcessing};

use std::{
    fmt, io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// 文件或缓冲区大小，单位 byte。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteCount(pub u64);

/// PNG 源或实际输出的色型；无损保留色型，有损可能转为索引色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PngColorType {
    Grayscale,
    Rgb,
    Indexed,
    GrayscaleAlpha,
    Rgba,
}

/// 静态PNG属性；导入阶段只验证结构，pipeline/inspect_png返回值还经过完整像素解码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: PngColorType,
    pub interlaced: bool,
}

/// 单文件资源上限，所有值须非零。不是操作系统级硬内存/时间配额。
///
/// 默认输入 64 MiB、单个解码缓冲区 128 MiB、16M 像素、单边 16384；
/// 编码器使用单线程及固定速度预设，调用方仍须限制并行文件数和总工作集。
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits {
    pub max_input_bytes: ByteCount,
    pub max_decoded_bytes: ByteCount,
    pub max_pixels: u64,
    pub max_dimension: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: ByteCount(64 * 1024 * 1024),
            max_decoded_bytes: ByteCount(128 * 1024 * 1024),
            max_pixels: 16 * 1024 * 1024,
            max_dimension: 16384,
        }
    }
}

impl ResourceLimits {
    pub(crate) fn validate(self) -> Result<(), ProcessingError> {
        if self.max_input_bytes.0 == 0
            || self.max_decoded_bytes.0 == 0
            || self.max_pixels == 0
            || self.max_dimension == 0
            || self.max_input_bytes.0 >= isize::MAX as u64
            || self.max_decoded_bytes.0 >= isize::MAX as u64
        {
            return Err(ProcessingError::InvalidLimits);
        }
        Ok(())
    }
}

/// 覆盖必建可恢复备份；副本仅接受尚不存在的目标（包括拒绝符号链接）。
#[derive(Debug, Clone, Default)]
pub enum OutputPolicy {
    #[default]
    Overwrite,
    Copy {
        destination: PathBuf,
    },
    /// 显式允许在既有root内创建结构目录；relative必须为非空普通相对组件。
    /// 规划只读，output暂存时创建目录；取消/失败/无收益可能保留空目录，不自动删除。
    CopyTree {
        root: PathBuf,
        relative: PathBuf,
    },
}

/// 启动时固定的静态 PNG 请求。路径在执行时验证，默认无损并覆盖原图。
#[derive(Debug, Clone)]
pub struct PngRequest {
    pub source: PathBuf,
    pub output: OutputPolicy,
    pub limits: ResourceLimits,
    pub mode: PngMode,
}

impl PngRequest {
    /// 创建默认无损请求；此时不执行 I/O。
    pub fn new(source: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            output: OutputPolicy::default(),
            limits: ResourceLimits::default(),
            mode: PngMode::Lossless,
        }
    }
}

/// 协作取消令牌。编码期间取消会等待编码返回并丢弃产物；提交临界区不再取消。
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// 请求取消；不代表正在运行的编码计算已停止。
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// 查询是否已请求取消。
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<(), ProcessingError> {
        if self.is_cancelled() {
            Err(ProcessingError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// 真实处理阶段，不代表百分比；BeforeCommit 是最后一个可取消的通知点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingStage {
    Reading,
    Optimizing,
    Validating,
    BeforeCommit,
}

/// 只描述已经发生的文件结果；失败和取消通过 ProcessingError 返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessingOutcome {
    Optimized {
        output: PathBuf,
        /// 覆盖时保留的原始文件备份；不会由核心自动删除。
        backup: Option<PathBuf>,
    },
    NoGain,
}

/// 真实文件大小与墙钟耗时；无收益时 output_bytes == input_bytes。
#[derive(Debug, Clone)]
pub struct ProcessingReport {
    /// 源图片属性；有损输出色型与交错方式可能不同。
    pub image: ImageInfo,
    pub output_image: ImageInfo,
    pub processing: PngProcessing,
    pub input_bytes: ByteCount,
    pub output_bytes: ByteCount,
    pub elapsed: Duration,
    pub outcome: ProcessingOutcome,
}

/// 可识别失败，不在显示文案中输出完整私人路径；底层 I/O/解码原因保留为 source。
#[derive(Debug)]
pub enum ProcessingError {
    InvalidLimits,
    InvalidPath,
    UnsupportedFormat,
    UnsupportedAnimation,
    InvalidPng(&'static str),
    ResourceLimit(&'static str),
    Decode(Box<dyn std::error::Error + Send + Sync>),
    Encode(Box<dyn std::error::Error + Send + Sync>),
    ValidationFailed(&'static str),
    TargetConflict,
    SourceChanged,
    Cancelled,
    Io {
        operation: &'static str,
        source: io::Error,
    },
    /// 进入替换临界区后失败，保留并返回恢复备份；不自动回滚覆盖外部改动。
    CommitFailed {
        source: io::Error,
        backup: PathBuf,
    },
    /// 清理失败时暴露残留路径与原始失败，供调用方恢复；不得伪装成已清理。
    CleanupFailed {
        /// 无收益清理失败时没有更早的处理错误。
        original: Option<Box<ProcessingError>>,
        source: io::Error,
        temporary: PathBuf,
    },
}

impl ProcessingError {
    pub(crate) fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

impl fmt::Display for ProcessingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("资源上限必须为非零且可寻址的有效值"),
            Self::InvalidPath => f.write_str("路径必须指向普通文件且不能是符号链接"),
            Self::UnsupportedFormat => f.write_str("本阶段仅支持真实静态 PNG 文件"),
            Self::UnsupportedAnimation => f.write_str("本阶段尚不支持 APNG，未修改原图"),
            Self::InvalidPng(reason) => write!(f, "PNG 结构无效：{reason}"),
            Self::ResourceLimit(resource) => write!(f, "资源超限：{resource}"),
            Self::Decode(_) => f.write_str("PNG 解码失败"),
            Self::Encode(_) => f.write_str("PNG 编码失败"),
            Self::ValidationFailed(reason) => write!(f, "PNG 产物验证失败：{reason}"),
            Self::TargetConflict => f.write_str("副本目标已存在，未覆盖"),
            Self::SourceChanged => f.write_str("源文件在处理期间发生变化，未提交"),
            Self::Cancelled => f.write_str("处理已取消，未提交"),
            Self::Io { operation, .. } => write!(f, "文件操作失败：{operation}"),
            Self::CommitFailed { .. } => f.write_str("替换失败，原始备份已保留"),
            Self::CleanupFailed { .. } => f.write_str("临时文件清理失败，残留路径已保留"),
        }
    }
}

impl std::error::Error for ProcessingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::CommitFailed { source, .. } => Some(source),
            Self::CleanupFailed {
                original: Some(original),
                ..
            } => Some(original.as_ref()),
            Self::CleanupFailed {
                original: None,
                source,
                ..
            } => Some(source),
            Self::Decode(source) | Self::Encode(source) => Some(source.as_ref()),
            _ => None,
        }
    }
}
