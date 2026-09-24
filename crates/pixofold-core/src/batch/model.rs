//! 批量领域契约；不是 IPC DTO。路径、Duration、原始错误留在 Rust 边界内。

use std::{fmt, io, path::PathBuf, sync::Arc};

use crate::model::{
    ByteCount, ContentCredentialsSource, OutputPolicy, PngMetadataPolicy, PngMode,
    PngQualityMapping, PngRequest, ProcessingError, ProcessingReport, ProcessingStage,
    ResourceLimits,
};

/// 服务生命周期内单调递增的批次标识，不能由调用方伪造。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchId(pub(super) u64);
impl BatchId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// 批次内稳定行标识；必须与 BatchId 组合使用，重试不改变该值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobId(pub(super) usize);
impl JobId {
    pub fn get(self) -> usize {
        self.0
    }
}

/// 一次启动的公共参数。核心 API 默认无损，产品默认有损由后续适配层显式传入。
#[derive(Debug, Clone, Copy, Default)]
pub struct BatchParameters {
    pub mode: PngMode,
    pub limits: ResourceLimits,
}

/// 已选定的一个源及其显式输出策略；P1 不扫描目录或自动生成副本名。
#[derive(Debug, Clone)]
pub struct BatchItem {
    pub source: PathBuf,
    pub output: OutputPolicy,
}

#[derive(Debug, Clone)]
pub struct BatchRequest {
    pub items: Vec<BatchItem>,
    pub parameters: BatchParameters,
}

/// 只选择失败/取消的行；可更新输出目标，源文件仍固定为该行的原路径。
#[derive(Debug, Clone)]
pub struct RetryJob {
    pub id: JobId,
    pub output: OutputPolicy,
    /// 普通重试须用Preserve；每次移除都显式提供原失败中的来源版本。
    pub metadata: PngMetadataPolicy,
}

#[derive(Debug, Clone)]
pub struct RetryRequest {
    pub jobs: Vec<RetryJob>,
    pub parameters: BatchParameters,
}

/// 同时执行数、保留任务数和估算活跃工作集预算；不是进程 RSS 硬限额。
/// 默认 1 worker、最多 1000 行、4 GiB 估算预算。worker 有效域 1–32，行数 1–100000。
#[derive(Debug, Clone, Copy)]
pub struct BatchConfig {
    pub workers: usize,
    pub max_jobs: usize,
    pub working_set_budget: ByteCount,
}
impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            workers: 1,
            max_jobs: 1000,
            working_set_budget: ByteCount(4 * 1024 * 1024 * 1024),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchPhase {
    Running,
    Cancelling,
    Finished,
}

/// 取消中仍是运行态；只在流水线返回后选择终态，成功提交不被取消请求覆盖。
#[derive(Debug, Clone)]
pub enum JobState {
    Queued,
    Running {
        stage: ProcessingStage,
        cancel_requested: bool,
    },
    Succeeded(ProcessingReport),
    NoGain(ProcessingReport),
    Failed(JobFailure),
    Cancelled,
}
impl JobState {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Queued | Self::Running { .. })
    }
    pub fn can_retry(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::Cancelled)
    }
}

/// 稳定错误类别；完整原因/恢复备份路径由 JobFailure::cause 保留，不写入日志。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobErrorCode {
    InvalidInput,
    UnsupportedFormat,
    UnsupportedAnimation,
    UnsupportedContentCredentials,
    UnsupportedMetadata,
    ResourceLimit,
    Decode,
    Encode,
    Validation,
    TargetConflict,
    SourceChanged,
    Io,
    CommitFailed,
    CleanupFailed,
    WorkerPanicked,
    ServiceFault,
}

#[derive(Debug, Clone)]
pub struct JobFailure {
    pub code: JobErrorCode,
    /// WorkerPanicked 的文件结果未知，不能宣称未写入；调用方应先核查备份/目标。
    pub cause: Option<Arc<ProcessingError>>,
}
impl JobFailure {
    pub(crate) fn processing(error: ProcessingError) -> Self {
        let code = match &error {
            ProcessingError::InvalidLimits
            | ProcessingError::InvalidPath
            | ProcessingError::InvalidPng(_) => JobErrorCode::InvalidInput,
            ProcessingError::UnsupportedFormat => JobErrorCode::UnsupportedFormat,
            ProcessingError::UnsupportedAnimation => JobErrorCode::UnsupportedAnimation,
            ProcessingError::UnsupportedMetadata([b'c', b'a', b'B', b'X'])
            | ProcessingError::ContentCredentialsRequireConsent(_) => {
                JobErrorCode::UnsupportedContentCredentials
            }
            ProcessingError::UnsupportedMetadata(_) => JobErrorCode::UnsupportedMetadata,
            ProcessingError::ResourceLimit(_) => JobErrorCode::ResourceLimit,
            ProcessingError::Decode(_) => JobErrorCode::Decode,
            ProcessingError::Encode(_) => JobErrorCode::Encode,
            ProcessingError::ValidationFailed(_) => JobErrorCode::Validation,
            ProcessingError::TargetConflict => JobErrorCode::TargetConflict,
            ProcessingError::SourceChanged => JobErrorCode::SourceChanged,
            ProcessingError::Io { .. } => JobErrorCode::Io,
            ProcessingError::CommitFailed { .. } => JobErrorCode::CommitFailed,
            ProcessingError::CleanupFailed { .. } => JobErrorCode::CleanupFailed,
            // 正常取消直接转换成 JobState::Cancelled，只有内部误用才进入此分支。
            ProcessingError::Cancelled => JobErrorCode::ServiceFault,
        };
        Self {
            code,
            cause: Some(Arc::new(error)),
        }
    }
    pub(super) fn fault(code: JobErrorCode) -> Self {
        Self { code, cause: None }
    }
}

#[derive(Debug, Clone)]
pub struct JobSnapshot {
    pub id: JobId,
    /// 从 1 起，每次显式重试递增。成功/无收益保留旧参数和尝试号。
    pub attempt: u32,
    pub request: PngRequest,
    pub mapping: Option<PngQualityMapping>,
    /// 准入时的真实文件大小；成功返回后更新为流水线实际读取值，无法读取则为 None。
    pub input_bytes: Option<ByteCount>,
    pub state: JobState,
}

impl JobSnapshot {
    /// 仅完整检查后、尚未写入的caBX失败可进入确认流程；嵌套清理失败不可绕过。
    pub fn content_credentials_source(&self) -> Option<&ContentCredentialsSource> {
        if let JobState::Failed(failure) = &self.state
            && let Some(cause) = &failure.cause
            && let ProcessingError::ContentCredentialsRequireConsent(source) = cause.as_ref()
        {
            return Some(source);
        }
        None
    }
}

/// 统计不等于耗时百分比。processed 不包括取消；terminal 包括取消，供生命周期判断。
/// 体积按「仅成功项计收益」记账；缺失大小/溢出为 None，不能冒充 0 bytes。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchSummary {
    pub total: usize,
    pub queued: usize,
    pub running: usize,
    pub succeeded: usize,
    pub no_gain: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub processed: usize,
    pub terminal: usize,
    pub input_bytes: Option<ByteCount>,
    pub current_bytes: Option<ByteCount>,
    pub saved_bytes: Option<ByteCount>,
}
impl BatchSummary {
    pub(super) fn from_jobs(jobs: &[JobSnapshot]) -> Self {
        let mut result = Self {
            total: jobs.len(),
            ..Self::default()
        };
        let (mut input, mut current, mut saved) = (Some(0_u64), Some(0_u64), Some(0_u64));
        for job in jobs {
            let size = job.input_bytes.map(|n| n.0);
            input = input.zip(size).and_then(|(sum, n)| sum.checked_add(n));
            let output = match &job.state {
                JobState::Succeeded(report) => {
                    result.succeeded += 1;
                    Some(report.output_bytes.0)
                }
                JobState::NoGain(_) => {
                    result.no_gain += 1;
                    size
                }
                JobState::Queued => {
                    result.queued += 1;
                    size
                }
                JobState::Running { .. } => {
                    result.running += 1;
                    size
                }
                JobState::Failed(_) => {
                    result.failed += 1;
                    size
                }
                JobState::Cancelled => {
                    result.cancelled += 1;
                    size
                }
            };
            current = current.zip(output).and_then(|(sum, n)| sum.checked_add(n));
            if let JobState::Succeeded(report) = &job.state {
                saved = saved.and_then(|sum| {
                    report
                        .input_bytes
                        .0
                        .checked_sub(report.output_bytes.0)
                        .and_then(|n| sum.checked_add(n))
                });
            }
        }
        result.processed = result.succeeded + result.no_gain + result.failed;
        result.terminal = result.processed + result.cancelled;
        result.input_bytes = input.map(ByteCount);
        result.current_bytes = current.map(ByteCount);
        result.saved_bytes = saved.map(ByteCount);
        result
    }
}

#[derive(Debug, Clone)]
pub struct BatchSnapshot {
    pub id: BatchId,
    /// 同一批次内单调递增，包含阶段、取消及重试变化；不是编码百分比。
    /// 最多100000行、每行u32次尝试且每次阶段更新有界，累计变化不会耗尽u64。
    pub revision: u64,
    pub phase: BatchPhase,
    /// 最近启动的参数。保留的已完成行可能属于更早参数，以各行 request 为准。
    pub parameters: BatchParameters,
    pub jobs: Vec<JobSnapshot>,
    pub summary: BatchSummary,
    pub active_workers: usize,
    pub reserved_working_bytes: ByteCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathConflictKind {
    DuplicateSource,
    DuplicateOutput,
    OutputIsInput,
    OutputHierarchy,
}

/// 服务/准入失败尚未启动新任务；单文件处理失败则保留在 JobState 中。
#[derive(Debug)]
pub enum BatchError {
    InvalidConfig,
    EmptyBatch,
    TooManyJobs,
    Busy,
    Closed,
    ServiceFault,
    NoSuchBatch,
    InvalidRetry,
    IdExhausted,
    TimedOut,
    /// 外部取消在准入提交前生效，未创建新批次/尝试。
    Cancelled,
    InvalidParameters(ProcessingError),
    PathConflict {
        first: JobId,
        second: JobId,
        kind: PathConflictKind,
    },
    WorkerStart(io::Error),
}
impl fmt::Display for BatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidConfig => "批量配置超出有效范围",
            Self::EmptyBatch => "文件列表不能为空",
            Self::TooManyJobs => "任务数超过队列上限",
            Self::Busy => "批次仍在准备、运行或取消中",
            Self::Closed => "任务服务已关闭",
            Self::ServiceFault => "任务服务内部异常，已停止接纳任务",
            Self::NoSuchBatch => "批次不存在或已替换",
            Self::InvalidRetry => "重试只能选择不重复的失败或取消项",
            Self::IdExhausted => "任务标识或尝试次数已耗尽",
            Self::TimedOut => "等待超时，不代表计算已经停止",
            Self::Cancelled => "准入已取消，未启动新任务",
            Self::InvalidParameters(_) => "批次参数无效",
            Self::PathConflict { .. } => "批量输入或输出路径相互冲突，未启动",
            Self::WorkerStart(_) => "无法创建后台工作线程",
        };
        f.write_str(message)
    }
}
impl std::error::Error for BatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WorkerStart(e) => Some(e),
            Self::InvalidParameters(e) => Some(e),
            _ => None,
        }
    }
}
