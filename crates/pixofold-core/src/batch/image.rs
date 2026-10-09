//! 两种实际编码格式共用的任务外层；专属选项/结果/错误不借用另一格式的语义。
//! 引擎由可信宿主持有并显式传递；扫描、规划和运行须使用同一共享实例。

use crate::{
    jpeg::{JpegEngine, JpegError, JpegInfo, JpegLimits, JpegMode, JpegReport, JpegRequest},
    model::*,
};
use std::{path::PathBuf, sync::Arc, time::Duration};

/// 共用无损/有损输入；保留既有PngMode名称和序列化契约，质量映射仍由格式决定。
pub use crate::model::PngMode as CompressionMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
}

/// 默认仅PNG。JPEG工具只能由宿主通过JpegEngine::load验证后注入。
#[derive(Clone, Default)]
pub struct ImageEngines {
    pub(super) jpeg: Option<Arc<JpegEngine>>,
}
impl std::fmt::Debug for ImageEngines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageEngines")
            .field("jpeg_available", &self.jpeg.is_some())
            .finish()
    }
}
impl ImageEngines {
    /// 接管已验证引擎；克隆配置共享工具身份句柄，最后一个所有者释放时关闭。
    pub fn with_jpeg(engine: JpegEngine) -> Self {
        Self {
            jpeg: Some(Arc::new(engine)),
        }
    }
    /// 查询已配置能力，不执行I/O；工具运行前仍会重新检查身份。
    pub fn supports(&self, format: ImageKind) -> bool {
        format == ImageKind::Png || self.jpeg.is_some()
    }
    pub(super) fn accepts(&self, other: &Self) -> bool {
        match (&self.jpeg, &other.jpeg) {
            (_, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// JPEG除共用资源上限外的执行约束；单次原生调用超时不等于整图耗时上限。
#[derive(Debug, Clone, Copy)]
pub struct JpegOptions {
    pub max_scans: u32,
    pub process_timeout: Duration,
}
impl Default for JpegOptions {
    fn default() -> Self {
        let limits = JpegLimits::default();
        Self {
            max_scans: limits.max_scans,
            process_timeout: limits.process_timeout,
        }
    }
}
impl JpegOptions {
    pub(super) fn limits(self, resources: ResourceLimits) -> JpegLimits {
        JpegLimits {
            resources,
            max_scans: self.max_scans,
            process_timeout: self.process_timeout,
        }
    }
}

#[derive(Debug, Clone)]
pub enum FormatOptions {
    Png(PngMetadataPolicy),
    Jpeg(JpegOptions),
}

/// 冻结的单行请求；limits同时用于预约和实际读取/编码/验证，不接受路径IPC。
#[derive(Debug, Clone)]
pub struct ImageRequest {
    pub source: PathBuf,
    pub output: OutputPolicy,
    pub mode: CompressionMode,
    pub limits: ResourceLimits,
    pub options: FormatOptions,
}
impl ImageRequest {
    pub fn format(&self) -> ImageKind {
        match self.options {
            FormatOptions::Png(_) => ImageKind::Png,
            FormatOptions::Jpeg(_) => ImageKind::Jpeg,
        }
    }
    /// 转为PNG单文件请求；复制参数并共享输出目录句柄，JPEG返回None。
    pub fn png(&self) -> Option<PngRequest> {
        let FormatOptions::Png(metadata) = &self.options else {
            return None;
        };
        Some(PngRequest {
            source: self.source.clone(),
            output: self.output.clone(),
            mode: self.mode,
            limits: self.limits,
            metadata: metadata.clone(),
        })
    }
    /// 转为JPEG单文件请求；保留执行资源和超时约束，不携带PNG元数据许可。
    pub fn jpeg(&self) -> Option<JpegRequest> {
        let FormatOptions::Jpeg(options) = self.options else {
            return None;
        };
        Some(JpegRequest {
            source: self.source.clone(),
            output: self.output.clone(),
            mode: match self.mode {
                CompressionMode::Lossless => JpegMode::Lossless,
                CompressionMode::Lossy { quality } => JpegMode::Lossy { quality },
            },
            limits: options.limits(self.limits),
        })
    }
}
impl From<PngRequest> for ImageRequest {
    fn from(p: PngRequest) -> Self {
        Self {
            source: p.source,
            output: p.output,
            mode: p.mode,
            limits: p.limits,
            options: FormatOptions::Png(p.metadata),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportedImage {
    Png(ImageInfo),
    Jpeg(JpegInfo),
}
impl ImportedImage {
    pub fn format(&self) -> ImageKind {
        match self {
            Self::Png(_) => ImageKind::Png,
            Self::Jpeg(_) => ImageKind::Jpeg,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ImageReport {
    Png(ProcessingReport),
    Jpeg(JpegReport),
}
impl ImageReport {
    pub fn input_bytes(&self) -> ByteCount {
        match self {
            Self::Png(r) => r.input_bytes,
            Self::Jpeg(r) => r.input_bytes,
        }
    }
    pub fn output_bytes(&self) -> ByteCount {
        match self {
            Self::Png(r) => r.output_bytes,
            Self::Jpeg(r) => r.output_bytes,
        }
    }
    pub fn outcome(&self) -> &ProcessingOutcome {
        match self {
            Self::Png(r) => &r.outcome,
            Self::Jpeg(r) => &r.outcome,
        }
    }
    pub fn png(&self) -> Option<&ProcessingReport> {
        match self {
            Self::Png(r) => Some(r),
            _ => None,
        }
    }
    pub fn credentials_removed(&self) -> bool {
        self.png().is_some_and(|r| r.content_credentials_removed)
    }
}

/// 完整恢复上下文仅保留在Rust；日志只使用JobErrorCode。
#[derive(Debug)]
pub enum ImageError {
    Png(ProcessingError),
    Jpeg(JpegError),
}
impl ImageError {
    pub fn png(&self) -> Option<&ProcessingError> {
        match self {
            Self::Png(e) => Some(e),
            _ => None,
        }
    }
    pub(super) fn cancelled(&self) -> bool {
        matches!(
            self,
            Self::Png(ProcessingError::Cancelled) | Self::Jpeg(JpegError::Cancelled)
        )
    }
}
impl From<ProcessingError> for ImageError {
    fn from(e: ProcessingError) -> Self {
        Self::Png(e)
    }
}
impl From<JpegError> for ImageError {
    fn from(e: JpegError) -> Self {
        Self::Jpeg(e)
    }
}
impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png(e) => e.fmt(f),
            Self::Jpeg(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Png(e) => e,
            Self::Jpeg(e) => e,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityMapping {
    Png(PngQualityMapping),
    Jpeg(crate::jpeg::JpegQualityMapping),
}
