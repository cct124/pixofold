//! 应用任务契约；不是IPC DTO。路径、核心错误和快照不直接跨WebView边界。

use pixofold_core::{
    batch::{BatchConfig, BatchError, BatchParameters, BatchSnapshot},
    import::{ImportError, ImportOutput, ImportScan, ScanOptions, ScanProgress},
    model::{PngMode, QualityValue},
};
use std::{fmt, io, sync::Arc};

/// 应用生命周期内单调递增，清除或换批后旧标识不能操作新导入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionId(pub(super) u64);
impl SelectionId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// 应用默认有损80、覆盖；核心PngRequest仍默认无损，二者不混淆。
#[derive(Debug, Clone)]
pub struct TaskSettings {
    pub parameters: BatchParameters,
    pub output: ImportOutput,
}
impl Default for TaskSettings {
    fn default() -> Self {
        Self {
            parameters: BatchParameters {
                mode: PngMode::Lossy {
                    quality: QualityValue::default(),
                },
                ..BatchParameters::default()
            },
            output: ImportOutput::Overwrite,
        }
    }
}

/// 沿用核心默认边界；协调线程不创建第二份编码线程池或扩大资源预算。
#[derive(Debug, Clone, Copy, Default)]
pub struct TaskConfig {
    pub batch: BatchConfig,
    pub scan: ScanOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPhase {
    Idle,
    Scanning,
    Ready,
    Preparing,
    Running,
    Cancelling,
    Finished,
    Cancelled,
    Rejected,
    Clearing,
    Closing,
    Closed,
}

/// 最新权威视图。Arc保留只读清单和批次，不持有文件句柄；不是持久化/崩溃恢复。
/// revision覆盖整个应用会话；批次内部revision及attempt仍由核心维护。
#[derive(Debug, Clone)]
pub struct TaskSnapshot {
    pub revision: u64,
    pub selection: Option<SelectionId>,
    pub phase: TaskPhase,
    pub scan_progress: Option<ScanProgress>,
    pub import: Option<Arc<ImportScan>>,
    pub batch: Option<Arc<BatchSnapshot>>,
    pub error: Option<TaskError>,
}

/// 保留错误链，但Display不打印路径。请求接纳成功不等于后台操作成功，后者见快照。
#[derive(Debug, Clone)]
pub enum TaskError {
    Busy,
    Closed,
    StaleSelection,
    StaleBatch,
    NotReady,
    TooManyRoots,
    IdExhausted,
    TimedOut,
    ServiceFault,
    WorkerPanicked,
    WorkerStart(Arc<io::Error>),
    Import(Arc<ImportError>),
    Batch(Arc<BatchError>),
}
impl From<ImportError> for TaskError {
    fn from(value: ImportError) -> Self {
        Self::Import(Arc::new(value))
    }
}
impl From<BatchError> for TaskError {
    fn from(value: BatchError) -> Self {
        Self::Batch(Arc::new(value))
    }
}
impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Busy => "已有导入、待修正清单或批次，不能重复接纳",
            Self::Closed => "应用任务服务正在关闭或已关闭",
            Self::StaleSelection => "导入标识已失效",
            Self::StaleBatch => "批次版本已变化，请读取新快照",
            Self::NotReady => "当前状态不允许该操作",
            Self::TooManyRoots => "显式导入根超过1000上限",
            Self::IdExhausted => "导入标识已耗尽",
            Self::TimedOut => "等待变化超时，不代表后台计算停止",
            Self::ServiceFault => "应用任务状态锁异常，已停止接纳",
            Self::WorkerPanicked => "后台协调异常，已取消并等待收尾",
            Self::WorkerStart(_) => "无法创建应用协调线程",
            Self::Import(_) => "导入或输出规划失败，请检查反馈",
            Self::Batch(_) => "批次操作失败，请检查反馈",
        })
    }
}
impl std::error::Error for TaskError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Import(e) => Some(e.as_ref()),
            Self::Batch(e) => Some(e.as_ref()),
            Self::WorkerStart(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
