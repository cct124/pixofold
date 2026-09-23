//! 导入候选、进度和规划契约；不是IPC DTO，路径和完整错误仅保留在Rust。

use crate::{
    batch::{BatchError, JobFailure},
    model::{ByteCount, ImageInfo, ResourceLimits},
};
use std::{fmt, path::PathBuf};

/// 默认最多1000根/1000候选、10000条目、32层目录、累计读取1GiB。
/// max_files上限100000、max_entries上限1000000、max_depth上限256；0层允许根目录直接文件。
/// probe_limits限制单文件压缩数据和头部声明的解码资源，扫描不分配像素缓冲。
#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    pub max_files: usize,
    pub max_entries: usize,
    pub max_depth: usize,
    pub max_read_bytes: ByteCount,
    pub probe_limits: ResourceLimits,
    /// 显式选择true才包含PixoFold保留名的临时/备份文件；普通_compressed图片不排除。
    pub include_artifacts: bool,
}
impl ScanOptions {
    /// 显式根硬上限；应用接纳与核心扫描共用，根数仍不得超过max_entries。
    pub const MAX_ROOTS: usize = 1000;
}
impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_files: 1000,
            max_entries: 10_000,
            max_depth: 32,
            max_read_bytes: ByteCount(1024 * 1024 * 1024),
            probe_limits: ResourceLimits::default(),
            include_artifacts: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanLimit {
    Entries,
    Files,
    Depth,
    ReadBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStatus {
    Scanning,
    Complete,
    Cancelled,
    Limited(ScanLimit),
}

/// 数量与实际读取字节，不是百分比；discovered包括目录和显式根，examined是已检查条目。
/// 回调在扫描线程同步执行，应快速返回且不panic；最终回调含终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanProgress {
    pub status: ScanStatus,
    pub discovered: usize,
    pub examined: usize,
    pub accepted: usize,
    pub duplicates: usize,
    pub excluded: usize,
    pub rejected: usize,
    pub read_bytes: ByteCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedFormat {
    Jpeg,
    Gif,
    WebP,
    Other,
}

#[derive(Debug, Clone)]
pub enum ImportIssueKind {
    Failure(JobFailure),
    Unsupported(UnsupportedFormat),
    Duplicate { first: PathBuf },
    GeneratedArtifact,
}

/// 一条独立反馈；path不自动写入日志，UI展示时另做隐私处理。
#[derive(Debug, Clone)]
pub struct ImportIssue {
    pub path: PathBuf,
    pub kind: ImportIssueKind,
}

/// 结构有效的静态PNG候选。CRC正确但压缩像素损坏的输入仍可能在pipeline解码失败。
#[derive(Debug, Clone)]
pub struct ImportedFile {
    pub source: PathBuf,
    /// 首次接受该文件的规范化导入根。
    pub root: PathBuf,
    pub root_is_directory: bool,
    pub relative_path: PathBuf,
    pub input_bytes: ByteCount,
    pub image: ImageInfo,
}

/// 私有字段保证调用方不能把未完成清单伪装成完整扫描；更换设置不消耗或修改清单。
#[derive(Debug)]
pub struct ImportScan {
    pub(super) files: Vec<ImportedFile>,
    pub(super) issues: Vec<ImportIssue>,
    pub(super) progress: ScanProgress,
}
impl ImportScan {
    pub fn files(&self) -> &[ImportedFile] {
        &self.files
    }
    pub fn issues(&self) -> &[ImportIssue] {
        &self.issues
    }
    pub fn progress(&self) -> ScanProgress {
        self.progress
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyLayout {
    Flat,
    PreserveRoots,
}

/// 默认覆盖。副本统一使用stem_compressed.png；同名拒绝而不是静默覆盖/自动编号。
#[derive(Debug, Clone, Default)]
pub enum ImportOutput {
    #[default]
    Overwrite,
    CopyBeside,
    /// directory须已存在；保留结构时目录根映射为root_name/相对路径，单独文件置于根。
    CopyTo {
        directory: PathBuf,
        layout: CopyLayout,
    },
}

#[derive(Debug)]
pub enum ImportError {
    InvalidOptions,
    TooManyRoots,
    IncompleteScan,
    NoFiles,
    /// 不将不同导入根悄悄合并到同名目标文件夹，即使当前子文件名尚未重叠。
    RootNameConflict {
        first: PathBuf,
        second: PathBuf,
    },
    Batch(BatchError),
    /// 预检失败保留候选索引（从0起）和原始原因，清单仍可修改设置后重新规划。
    File {
        index: usize,
        failure: JobFailure,
    },
}
impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidOptions => "导入资源配置无效",
            Self::TooManyRoots => "导入根数量超出上限",
            Self::IncompleteScan => "扫描未完整结束，不能启动部分批次",
            Self::NoFiles => "没有可处理的静态PNG候选",
            Self::RootNameConflict { .. } => "不同导入根映射到同名目标文件夹",
            Self::Batch(_) => "批次参数或路径规划冲突",
            Self::File { .. } => "候选文件或输出路径预检失败",
        })
    }
}
impl std::error::Error for ImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Batch(error) => Some(error),
            Self::File { failure, .. } => failure
                .cause
                .as_deref()
                .map(|e| e as &dyn std::error::Error),
            _ => None,
        }
    }
}
