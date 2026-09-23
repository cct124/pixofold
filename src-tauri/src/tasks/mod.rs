//! 应用所有的任务协调器；不依赖窗口、Commands或React生命周期。
//! 单槽接纳+一个协调线程执行扫描/规划，核心BatchService唯一持有编码worker。
//! 所有I/O和join在状态锁外；TaskControl可克隆但不拥有线程，TaskRuntime负责收尾。
//! 后续IPC必须转换DTO与授权原生路径，不直接序列化本模块领域快照。

mod model;
mod worker;
pub use model::*;

use pixofold_core::{
    batch::{BatchId, BatchService, RetryRequest},
    import::ImportScan,
    model::CancellationToken,
};
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::JoinHandle,
    time::{Duration, Instant},
};

enum Command {
    Import {
        roots: Vec<PathBuf>,
        settings: Option<TaskSettings>,
    },
    Start {
        scan: Arc<ImportScan>,
        settings: TaskSettings,
    },
    Retry(RetryRequest),
    Clear,
}
struct State {
    view: TaskSnapshot,
    next_id: u64,
    pending: Option<Command>,
    bound_batch: Option<BatchId>,
    cancel: CancellationToken,
    closing: bool,
    faulted: bool,
}
impl State {
    fn available(&self) -> Result<(), TaskError> {
        if self.faulted {
            Err(TaskError::ServiceFault)
        } else if self.closing {
            Err(TaskError::Closed)
        } else {
            Ok(())
        }
    }
    fn selection(&self, id: SelectionId) -> Result<(), TaskError> {
        self.available()?;
        if self.view.selection == Some(id) {
            Ok(())
        } else {
            Err(TaskError::StaleSelection)
        }
    }
    fn close(&mut self) {
        self.closing = true;
        self.cancel.cancel();
        if self.view.phase != TaskPhase::Closed {
            self.view.phase = TaskPhase::Closing;
        }
    }
    fn fault(&mut self) {
        self.faulted = true;
        self.close();
        self.view.error = Some(TaskError::ServiceFault);
    }
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(error) => {
                let mut state = error.into_inner();
                state.fault();
                self.changed.notify_all();
                state
            }
        }
    }
    fn publish(&self, state: &mut State) {
        // 每次扫描/批次均有界；仍防止标识绕回后被误认成旧事件。
        if let Some(next) = state.view.revision.checked_add(1) {
            state.view.revision = next;
        } else {
            state.fault();
        }
        self.changed.notify_all();
    }
    fn wait<'a>(&self, state: MutexGuard<'a, State>, timeout: Duration) -> MutexGuard<'a, State> {
        match self.changed.wait_timeout(state, timeout) {
            Ok((state, _)) => state,
            Err(error) => {
                let (mut state, _) = error.into_inner();
                state.fault();
                self.changed.notify_all();
                state
            }
        }
    }
}

/// 克隆此句柄不会新建队列/线程；销毁前端持有的句柄不取消任务。
#[derive(Clone)]
pub struct TaskControl {
    shared: Arc<Shared>,
}
impl TaskControl {
    /// 接纳一份导入。settings=None保留完整清单等待修正，Some则扫描完成后自动规划启动。
    /// 只投递单槽后台命令，不执行文件I/O；冻结传入设置，不读取后续UI草稿。
    /// # Errors
    /// 忙、关闭、根数超限或标识耗尽时不改变现有任务。空列表在后台反馈NoFiles。
    pub fn import(
        &self,
        roots: Vec<PathBuf>,
        settings: Option<TaskSettings>,
    ) -> Result<SelectionId, TaskError> {
        if roots.len() > pixofold_core::import::ScanOptions::MAX_ROOTS {
            return Err(TaskError::TooManyRoots);
        }
        let mut state = self.shared.lock();
        state.available()?;
        if !matches!(
            state.view.phase,
            TaskPhase::Idle | TaskPhase::Finished | TaskPhase::Cancelled | TaskPhase::Rejected
        ) {
            return Err(TaskError::Busy);
        }
        let id = SelectionId(state.next_id);
        state.next_id = state.next_id.checked_add(1).ok_or(TaskError::IdExhausted)?;
        state.cancel = CancellationToken::default();
        state.view.selection = Some(id);
        state.view.phase = TaskPhase::Scanning;
        state.view.import = None;
        state.view.scan_progress = None;
        state.view.batch = None;
        state.bound_batch = None;
        state.view.error = None;
        state.pending = Some(Command::Import { roots, settings });
        self.shared.publish(&mut state);
        Ok(id)
    }

    /// 在同一份完整清单上固定新设置，仅Ready可启动一次。后台错误仍保留清单。
    /// # Errors
    /// 旧标识、关闭或非Ready状态拒绝；参数/路径错误通过后续快照反馈。
    pub fn start(&self, id: SelectionId, settings: TaskSettings) -> Result<(), TaskError> {
        let mut state = self.shared.lock();
        state.selection(id)?;
        if state.view.phase != TaskPhase::Ready {
            return Err(TaskError::NotReady);
        }
        let scan = state.view.import.clone().ok_or(TaskError::NotReady)?;
        state.cancel = CancellationToken::default();
        state.view.phase = TaskPhase::Preparing;
        state.view.error = None;
        state.pending = Some(Command::Start { scan, settings });
        self.shared.publish(&mut state);
        Ok(())
    }

    /// 对已结束批次显式重试；expected_revision来自最新批次快照，防止旧请求重复重试。
    /// # Errors
    /// 标识、状态、版本不符立即拒绝；行/输出校验由后台核心执行，失败不改attempt。
    pub fn retry(
        &self,
        id: SelectionId,
        expected_revision: u64,
        request: RetryRequest,
    ) -> Result<(), TaskError> {
        let mut state = self.shared.lock();
        state.selection(id)?;
        if state.view.phase != TaskPhase::Finished {
            return Err(TaskError::NotReady);
        }
        if state
            .view
            .batch
            .as_ref()
            .is_none_or(|b| b.revision != expected_revision)
        {
            return Err(TaskError::StaleBatch);
        }
        state.cancel = CancellationToken::default();
        state.view.phase = TaskPhase::Preparing;
        state.view.error = None;
        state.pending = Some(Command::Retry(request));
        self.shared.publish(&mut state);
        Ok(())
    }

    /// 请求取消；计算返回前保持Cancelling，已完成结果不改写。Ready取消后不自动启动。
    /// # Errors
    /// 旧标识/关闭拒绝，清除过程中不能取消；终态取消为幂等无操作。
    pub fn cancel(&self, id: SelectionId) -> Result<(), TaskError> {
        let mut state = self.shared.lock();
        state.selection(id)?;
        match state.view.phase {
            TaskPhase::Scanning | TaskPhase::Preparing | TaskPhase::Running => {
                state.cancel.cancel();
                state.view.phase = TaskPhase::Cancelling;
            }
            TaskPhase::Ready => {
                state.cancel.cancel();
                state.view.phase = TaskPhase::Cancelled;
            }
            TaskPhase::Clearing => return Err(TaskError::Busy),
            _ => return Ok(()),
        }
        self.shared.publish(&mut state);
        Ok(())
    }

    /// 清除非活动记录，不删除产物/备份；返回旧快照以保留恢复信息。
    /// # Errors
    /// 旧标识、关闭、扫描/准备/运行中拒绝；真正清除在后台完成。
    pub fn clear(&self, id: SelectionId) -> Result<TaskSnapshot, TaskError> {
        let mut state = self.shared.lock();
        state.selection(id)?;
        if !matches!(
            state.view.phase,
            TaskPhase::Ready | TaskPhase::Finished | TaskPhase::Cancelled | TaskPhase::Rejected
        ) {
            return Err(TaskError::Busy);
        }
        let previous = state.view.clone();
        state.view.phase = TaskPhase::Clearing;
        state.pending = Some(Command::Clear);
        self.shared.publish(&mut state);
        Ok(previous)
    }

    /// 无I/O的只读快照；完整路径仅供可信Rust调用方，不用于广播/日志。
    pub fn snapshot(&self) -> TaskSnapshot {
        self.shared.lock().view.clone()
    }

    /// 等待比after更新的视图或关闭；超时只停止等待，不取消任务。不得在UI线程调用。
    /// # Errors
    /// 超时返回TimedOut；锁故障仍可读取带诊断的关闭快照。
    pub fn wait_for_change(
        &self,
        after: u64,
        timeout: Duration,
    ) -> Result<TaskSnapshot, TaskError> {
        let started = Instant::now();
        let mut state = self.shared.lock();
        loop {
            if state.view.revision > after || state.view.phase == TaskPhase::Closed {
                return Ok(state.view.clone());
            }
            let remaining = timeout
                .checked_sub(started.elapsed())
                .ok_or(TaskError::TimedOut)?;
            state = self.shared.wait(state, remaining);
        }
    }

    /// 立即停止接纳并协作取消，非阻塞；真正退出必须等待TaskRuntime::shutdown完成。
    pub fn request_close(&self) {
        let mut state = self.shared.lock();
        if !state.closing {
            state.close();
            self.shared.publish(&mut state);
        }
    }
}

/// 应用唯一线程所有者。Drop取消并join；显式shutdown允许返回协调/核心故障。
pub struct TaskRuntime {
    control: TaskControl,
    thread: Option<JoinHandle<Result<(), TaskError>>>,
}
impl TaskRuntime {
    /// 创建一个协调线程及配置内的固定编码worker；不读取图片。
    /// # Errors
    /// 配置或线程创建失败，已创建线程会收尾。
    pub fn new(config: TaskConfig) -> Result<Self, TaskError> {
        Self::with_backend(config, Arc::new(worker::NativeImport))
    }
    fn with_backend(
        config: TaskConfig,
        backend: Arc<dyn worker::ImportBackend>,
    ) -> Result<Self, TaskError> {
        let service = BatchService::new(config.batch)?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                view: TaskSnapshot {
                    revision: 0,
                    selection: None,
                    phase: TaskPhase::Idle,
                    scan_progress: None,
                    import: None,
                    batch: None,
                    error: None,
                },
                next_id: 1,
                pending: None,
                bound_batch: None,
                cancel: CancellationToken::default(),
                closing: false,
                faulted: false,
            }),
            changed: Condvar::new(),
        });
        let owner = shared.clone();
        let thread = std::thread::Builder::new()
            .name("pixofold-application".into())
            .spawn(move || worker::run(owner, service, config.scan, backend))
            .map_err(|e| TaskError::WorkerStart(Arc::new(e)))?;
        Ok(Self {
            control: TaskControl { shared },
            thread: Some(thread),
        })
    }
    /// 获得不拥有线程的控制句柄；窗口重载只重取句柄，不新建服务。
    pub fn control(&self) -> TaskControl {
        self.control.clone()
    }
    /// 非UI线程调用：停止接纳，取消，等待扫描/编码真实返回及全部线程join。可重复。
    /// # Errors
    /// 协调panic/锁故障/核心关闭失败会返回错误，仍先完成资源收尾。
    pub fn shutdown(&mut self) -> Result<(), TaskError> {
        self.control.request_close();
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| TaskError::WorkerPanicked)?
        } else {
            Ok(())
        }
    }
}
impl Drop for TaskRuntime {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests;
