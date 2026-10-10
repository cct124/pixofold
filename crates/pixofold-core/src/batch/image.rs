//! 两种实际编码格式共用的任务外层；专属选项/结果/错误不借用另一格式的语义。
//! 引擎由可信宿主持有并显式传递；扫描、规划和运行须使用同一共享实例。

use crate::{
    gif::{GifEngine, GifError, GifInfo, GifLimits, GifReport, GifRequest, GifValidationLimits},
    jpeg::{JpegEngine, JpegError, JpegInfo, JpegLimits, JpegMode, JpegReport, JpegRequest},
    model::*,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// 共用无损/有损输入；保留既有PngMode名称和序列化契约，质量映射仍由格式决定。
pub use crate::model::PngMode as CompressionMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
    Gif,
}

/// 默认仅PNG。JPEG/GIF工具只能由宿主通过对应Engine::load验证后注入。
#[derive(Clone, Default)]
pub struct ImageEngines {
    pub(super) jpeg: Option<Arc<JpegEngine>>,
    pub(super) gif: Option<Arc<GifEngine>>,
}
impl std::fmt::Debug for ImageEngines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageEngines")
            .field("jpeg_available", &self.jpeg.is_some())
            .field("gif_available", &self.gif.is_some())
            .finish()
    }
}
impl ImageEngines {
    /// 接管已验证引擎；克隆配置共享工具身份句柄，最后一个所有者释放时关闭。
    pub fn with_jpeg(engine: JpegEngine) -> Self {
        Self {
            jpeg: Some(Arc::new(engine)),
            ..Self::default()
        }
    }
    /// 查询已配置能力，不执行I/O；工具运行前仍会重新检查身份。
    pub fn supports(&self, format: ImageKind) -> bool {
        match format {
            ImageKind::Png => true,
            ImageKind::Jpeg => self.jpeg.is_some(),
            ImageKind::Gif => self.gif.is_some(),
        }
    }
    /// 借用已验证的同一JPEG引擎；用于宿主有界预览，执行仍复查工具身份。
    pub fn jpeg(&self) -> Option<&JpegEngine> {
        self.jpeg.as_deref()
    }
    /// 接管已验证GIF引擎，默认PNG能力保持可用。
    pub fn with_gif(engine: GifEngine) -> Self {
        Self::default().add_gif(engine)
    }
    /// 在同一能力配置加入GIF；克隆配置共享身份句柄，用于PNG/JPEG/GIF混合核心。
    pub fn add_gif(mut self, engine: GifEngine) -> Self {
        self.gif = Some(Arc::new(engine));
        self
    }
    /// 借用可信GIF引擎，实际调用仍复查工具身份。
    pub fn gif(&self) -> Option<&GifEngine> {
        self.gif.as_deref()
    }
    pub(super) fn accepts(&self, other: &Self) -> bool {
        let jpeg = match (&self.jpeg, &other.jpeg) {
            (_, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        jpeg && match (&self.gif, &other.gif) {
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
    Gif(GifOptions),
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
            FormatOptions::Gif(_) => ImageKind::Gif,
        }
    }
    /// 转为PNG单文件请求；复制参数并共享输出目录句柄，其他格式返回None。
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
    /// 转为仅无损GIF请求，独立约束实际输入/验证缓冲和原生分配。
    pub fn gif(&self) -> Result<GifRequest, GifError> {
        let FormatOptions::Gif(options) = self.options else {
            return Err(GifError::InvalidLimits);
        };
        if !matches!(self.mode, CompressionMode::Lossless) {
            return Err(GifError::UnsupportedMode);
        }
        Ok(GifRequest {
            source: self.source.clone(),
            output: self.output.clone(),
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
    Gif(GifInfo),
}
impl ImportedImage {
    pub fn format(&self) -> ImageKind {
        match self {
            Self::Png(_) => ImageKind::Png,
            Self::Jpeg(_) => ImageKind::Jpeg,
            Self::Gif(_) => ImageKind::Gif,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ImageReport {
    Png(ProcessingReport),
    Jpeg(JpegReport),
    Gif(GifReport),
}
impl ImageReport {
    pub fn input_bytes(&self) -> ByteCount {
        match self {
            Self::Png(r) => r.input_bytes,
            Self::Jpeg(r) => r.input_bytes,
            Self::Gif(r) => r.input_bytes,
        }
    }
    pub fn output_bytes(&self) -> ByteCount {
        match self {
            Self::Png(r) => r.output_bytes,
            Self::Jpeg(r) => r.output_bytes,
            Self::Gif(r) => r.output_bytes,
        }
    }
    pub fn outcome(&self) -> &ProcessingOutcome {
        match self {
            Self::Png(r) => &r.outcome,
            Self::Jpeg(r) => &r.outcome,
            Self::Gif(r) => &r.outcome,
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
    Gif(GifError),
}
impl ImageError {
    /// 沿清理错误查找实际提交失败留下的备份；不猜测文件名、不执行I/O。
    pub fn recovery_backup(&self) -> Option<&Path> {
        let mut file = match self {
            Self::Gif(error) => {
                let mut current = Some(error);
                let mut found = None;
                while let Some(error) = current {
                    match error {
                        GifError::File(error) => {
                            found = Some(error);
                            break;
                        }
                        GifError::Cleanup { original, .. } => current = original.as_deref(),
                        _ => break,
                    }
                }
                found
            }
            Self::Png(error) => Some(error),
            Self::Jpeg(error) => {
                let mut current = Some(error);
                let mut found = None;
                while let Some(error) = current {
                    match error {
                        JpegError::File(error) => {
                            found = Some(error);
                            break;
                        }
                        JpegError::Cleanup { original, .. } => current = original.as_deref(),
                        _ => break,
                    }
                }
                found
            }
        };
        while let Some(error) = file {
            match error {
                ProcessingError::CommitFailed { backup, .. } => return Some(backup),
                ProcessingError::CleanupFailed { original, .. } => file = original.as_deref(),
                _ => break,
            }
        }
        None
    }
    pub fn png(&self) -> Option<&ProcessingError> {
        match self {
            Self::Png(e) => Some(e),
            _ => None,
        }
    }
    pub(super) fn cancelled(&self) -> bool {
        matches!(
            self,
            Self::Png(ProcessingError::Cancelled)
                | Self::Jpeg(JpegError::Cancelled)
                | Self::Gif(GifError::Cancelled)
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
impl From<GifError> for ImageError {
    fn from(error: GifError) -> Self {
        Self::Gif(error)
    }
}
impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png(e) => e.fmt(f),
            Self::Jpeg(e) => e.fmt(f),
            Self::Gif(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Png(e) => e,
            Self::Jpeg(e) => e,
            Self::Gif(e) => e,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityMapping {
    Png(PngQualityMapping),
    Jpeg(crate::jpeg::JpegQualityMapping),
}
/// GIF仅无损；格式硬上限在转换时与公共资源取较小值，额外约束固定到每行请求。
#[derive(Debug, Clone, Copy)]
pub struct GifOptions {
    pub validation: GifValidationLimits,
    pub max_native_bytes: ByteCount,
    pub process_timeout: Duration,
    pub validation_timeout: Duration,
}
impl Default for GifOptions {
    fn default() -> Self {
        let limits = GifLimits::default();
        Self {
            validation: limits.validation,
            max_native_bytes: limits.max_native_bytes,
            process_timeout: limits.process_timeout,
            validation_timeout: limits.validation_timeout,
        }
    }
}
impl GifOptions {
    pub(crate) fn limits(self, resources: ResourceLimits) -> GifLimits {
        let maximum = GifLimits::default().resources;
        GifLimits {
            resources: ResourceLimits {
                max_input_bytes: ByteCount(
                    resources.max_input_bytes.0.min(maximum.max_input_bytes.0),
                ),
                max_decoded_bytes: ByteCount(
                    resources
                        .max_decoded_bytes
                        .0
                        .min(maximum.max_decoded_bytes.0),
                ),
                max_pixels: resources.max_pixels.min(maximum.max_pixels),
                max_dimension: resources.max_dimension.min(maximum.max_dimension),
            },
            validation: self.validation,
            max_native_bytes: self.max_native_bytes,
            process_timeout: self.process_timeout,
            validation_timeout: self.validation_timeout,
        }
    }
}
