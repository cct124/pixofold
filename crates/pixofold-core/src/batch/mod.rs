//! 应用生命周期内的纯Rust批量任务服务；仅接收显式文件列表，不依赖Tauri或GUI。
//! 状态只在短临界区更新，路径I/O、编码、等待join均在锁外。同步pipeline是唯一处理入口。
//! worker持有Shared而非BatchService，避免所有权环；取消和Drop不等同于强杀编码器。

mod model;
mod planning;
mod worker;

pub use model::*;
pub use planning::estimate_working_set;

use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crate::model::{
    ByteCount, CancellationToken, PngRequest, ProcessingError, ProcessingReport, ProcessingStage,
};

#[derive(Clone)]
struct Job {
    view: JobSnapshot,
    reservation: ByteCount,
}
struct Batch {
    id: BatchId,
    revision: u64,
    parameters: BatchParameters,
    jobs: Vec<Job>,
    queue: VecDeque<usize>,
    cancel: CancellationToken,
    running: usize,
    reserved: u64,
}
impl Batch {
    fn new(id: BatchId, revision: u64, parameters: BatchParameters, jobs: Vec<Job>) -> Self {
        let queue = jobs
            .iter()
            .enumerate()
            .filter_map(|(i, j)| matches!(j.view.state, JobState::Queued).then_some(i))
            .collect();
        Self {
            id,
            revision,
            parameters,
            jobs,
            queue,
            cancel: CancellationToken::default(),
            running: 0,
            reserved: 0,
        }
    }
    fn active(&self) -> bool {
        self.running != 0 || !self.queue.is_empty()
    }
    fn snapshot(&self) -> BatchSnapshot {
        let jobs: Vec<_> = self.jobs.iter().map(|j| j.view.clone()).collect();
        BatchSnapshot {
            id: self.id,
            revision: self.revision,
            parameters: self.parameters,
            phase: if !self.active() {
                BatchPhase::Finished
            } else if self.cancel.is_cancelled() {
                BatchPhase::Cancelling
            } else {
                BatchPhase::Running
            },
            summary: BatchSummary::from_jobs(&jobs),
            jobs,
            active_workers: self.running,
            reserved_working_bytes: ByteCount(self.reserved),
        }
    }
    fn cancel(&mut self, fault: bool) {
        if !self.active() || self.cancel.is_cancelled() {
            return;
        }
        self.cancel.cancel();
        self.queue.clear();
        for job in &mut self.jobs {
            match &mut job.view.state {
                JobState::Queued => {
                    job.view.state = if fault {
                        JobState::Failed(JobFailure::fault(JobErrorCode::ServiceFault))
                    } else {
                        JobState::Cancelled
                    }
                }
                JobState::Running {
                    cancel_requested, ..
                } => *cancel_requested = true,
                _ => {}
            }
        }
        self.revision += 1;
    }
}

struct State {
    closed: bool,
    faulted: bool,
    preparing: bool,
    next_id: u64,
    batch: Option<Batch>,
}
impl State {
    fn close(&mut self, fault: bool) {
        self.closed = true;
        self.faulted |= fault;
        if let Some(batch) = &mut self.batch {
            batch.cancel(fault);
        }
    }
    fn available(&self) -> Result<(), BatchError> {
        if self.faulted {
            Err(BatchError::ServiceFault)
        } else if self.closed {
            Err(BatchError::Closed)
        } else if self.preparing || self.batch.as_ref().is_some_and(Batch::active) {
            Err(BatchError::Busy)
        } else {
            Ok(())
        }
    }
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    config: BatchConfig,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(error) => {
                // 不静默忽略poison：停止接纳、取消排队项，仍允许读取快照和安全join。
                let mut state = error.into_inner();
                state.close(true);
                self.changed.notify_all();
                state
            }
        }
    }
    fn wait<'a>(&self, state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        match self.changed.wait(state) {
            Ok(state) => state,
            Err(error) => {
                let mut state = error.into_inner();
                state.close(true);
                self.changed.notify_all();
                state
            }
        }
    }
}

// 私有测试边界，生产只有PipelineRunner。不向外暴露可绕过输出层的通用执行器。
trait Runner: Send + Sync {
    fn run(
        &self,
        request: &PngRequest,
        cancel: &CancellationToken,
        stage: &mut dyn FnMut(ProcessingStage),
    ) -> Result<ProcessingReport, ProcessingError>;
}
struct PipelineRunner;
impl Runner for PipelineRunner {
    fn run(
        &self,
        request: &PngRequest,
        cancel: &CancellationToken,
        stage: &mut dyn FnMut(ProcessingStage),
    ) -> Result<ProcessingReport, ProcessingError> {
        crate::pipeline::optimize_png(request, cancel, stage)
    }
}

/// 持有固定worker池，只保存最近批次；开启下一批不会删除旧输出或备份。
/// snapshot/cancel等方法可跨线程调用；shutdown需要独占服务，返回后后台线程已结束。
///
/// 显式列表与副本目标示例（父目录须已存在；不扫描目录，不覆盖示例源）：
/// ```no_run
/// use pixofold_core::{batch::*, model::{OutputPolicy, PngMode, QualityValue}};
/// use std::time::Duration;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut service = BatchService::new(BatchConfig::default())?;
/// let id = service.start(BatchRequest {
///     items: vec![BatchItem {
///         source: "input.png".into(),
///         output: OutputPolicy::Copy { destination: "output.png".into() },
///     }],
///     parameters: BatchParameters {
///         mode: PngMode::Lossy { quality: QualityValue::default() },
///         ..BatchParameters::default()
///     },
/// })?;
/// // 超时只停止等待；稍后可再次查询或请求取消。
/// match service.wait(id, Duration::from_secs(60)) {
///     Ok(snapshot) => println!("已处理 {} 项", snapshot.summary.processed),
///     Err(BatchError::TimedOut) => service.cancel(id)?,
///     Err(error) => return Err(error.into()),
/// }
/// service.shutdown()?; // 等待真实计算/清理结束，不将运行中资源交给界面释放。
/// # Ok(())
/// # }
/// ```
pub struct BatchService {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

struct Preparation<'a>(&'a Shared);
impl Drop for Preparation<'_> {
    fn drop(&mut self) {
        self.0.lock().preparing = false;
        self.0.changed.notify_all();
    }
}

impl BatchService {
    /// 创建固定大小的后台线程池；不读取图片。
    /// # Errors
    /// 配置非法或OS无法创建线程时失败，已创建线程会收尾，不遗留后台任务。
    pub fn new(config: BatchConfig) -> Result<Self, BatchError> {
        Self::with_runner(config, Arc::new(PipelineRunner))
    }

    fn with_runner(config: BatchConfig, runner: Arc<dyn Runner>) -> Result<Self, BatchError> {
        if !(1..=32).contains(&config.workers)
            || !(1..=100_000).contains(&config.max_jobs)
            || config.working_set_budget.0 == 0
        {
            return Err(BatchError::InvalidConfig);
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                closed: false,
                faulted: false,
                preparing: false,
                next_id: 1,
                batch: None,
            }),
            changed: Condvar::new(),
            config,
        });
        let mut service = Self {
            shared,
            workers: Vec::with_capacity(config.workers),
        };
        for i in 0..config.workers {
            let (shared, runner) = (Arc::clone(&service.shared), Arc::clone(&runner));
            match std::thread::Builder::new()
                .name(format!("pixofold-png-{i}"))
                .spawn(move || worker::run(shared, runner))
            {
                Ok(handle) => service.workers.push(handle),
                Err(error) => return Err(BatchError::WorkerStart(error)), // Drop关闭并join已启动的空闲worker。
            }
        }
        Ok(service)
    }

    fn prepare_guard(&self) -> Result<Preparation<'_>, BatchError> {
        let mut state = self.shared.lock();
        state.available()?;
        state.preparing = true;
        Ok(Preparation(&self.shared))
    }

    /// 同步只读预检文件列表后入队并返回ID；编码在后台，参数与输出路径已克隆固定。
    /// 单项缺失/权限/目标占用记录失败并继续其他行；跨任务路径冲突整批拒绝，不写文件。
    /// # Errors
    /// 空列表、队列上限、非法参数、服务忙/关闭、跨任务冲突均在启动前返回。
    pub fn start(&self, request: BatchRequest) -> Result<BatchId, BatchError> {
        if request.items.is_empty() {
            return Err(BatchError::EmptyBatch);
        }
        if request.items.len() > self.shared.config.max_jobs {
            return Err(BatchError::TooManyJobs);
        }
        estimate_working_set(request.parameters)?;
        let _guard = self.prepare_guard()?;
        let requests = request
            .items
            .into_iter()
            .map(|item| PngRequest {
                source: item.source,
                output: item.output,
                mode: request.parameters.mode,
                limits: request.parameters.limits,
            })
            .collect();
        let jobs = planning::prepare(requests, self.shared.config.working_set_budget)?;
        let mut state = self.shared.lock();
        if state.closed {
            return Err(BatchError::Closed);
        }
        let id = BatchId(state.next_id);
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or(BatchError::IdExhausted)?;
        state.batch = Some(Batch::new(id, 1, request.parameters, jobs));
        self.shared.changed.notify_all();
        Ok(id)
    }

    /// 获取独立只读快照；修改返回值不会影响后台参数。没有批次时为None。
    pub fn snapshot(&self) -> Option<BatchSnapshot> {
        self.shared.lock().batch.as_ref().map(Batch::snapshot)
    }

    /// 请求整批取消；排队项立即取消，运行项仅标记取消中。结束批次调用为幂等无操作。
    /// # Errors
    /// 批次不存在/被替换时返回NoSuchBatch。
    pub fn cancel(&self, id: BatchId) -> Result<(), BatchError> {
        let mut state = self.shared.lock();
        let batch = state
            .batch
            .as_mut()
            .filter(|b| b.id == id)
            .ok_or(BatchError::NoSuchBatch)?;
        batch.cancel(false);
        self.shared.changed.notify_all();
        Ok(())
    }

    /// 等待批次真正结束，超时只停止等待，不取消/终止计算。无需轮询或固定休眠。
    /// # Errors
    /// 批次被替换、内部锁故障或超时返回独立错误；超时后仍可查询/取消。
    pub fn wait(&self, id: BatchId, timeout: Duration) -> Result<BatchSnapshot, BatchError> {
        let started = Instant::now();
        let mut state = self.shared.lock();
        loop {
            if state.faulted {
                return Err(BatchError::ServiceFault);
            }
            let batch = state
                .batch
                .as_ref()
                .filter(|b| b.id == id)
                .ok_or(BatchError::NoSuchBatch)?;
            if !batch.active() {
                return Ok(batch.snapshot());
            }
            let remaining = timeout
                .checked_sub(started.elapsed())
                .ok_or(BatchError::TimedOut)?;
            state = match self.shared.changed.wait_timeout(state, remaining) {
                Ok((state, _)) => state,
                Err(error) => {
                    let (mut state, _) = error.into_inner();
                    state.close(true);
                    self.shared.changed.notify_all();
                    state
                }
            };
        }
    }

    /// 显式选择失败/取消行，以当前参数和目标重新启动；成功/无收益行原样保留。
    /// 整批路径重新预检，包含保留行，避免重试输出覆盖既有结果或别的输入。
    /// # Errors
    /// 运行/取消中拒绝重试；重复、无效或不可重试行、路径冲突均不改变原快照。
    pub fn retry(&self, id: BatchId, request: RetryRequest) -> Result<(), BatchError> {
        estimate_working_set(request.parameters)?;
        let _guard = self.prepare_guard()?;
        let (old, revision) = {
            let state = self.shared.lock();
            let batch = state
                .batch
                .as_ref()
                .filter(|b| b.id == id)
                .ok_or(BatchError::NoSuchBatch)?;
            (batch.jobs.clone(), batch.revision)
        };
        if request.jobs.is_empty() || request.jobs.len() > old.len() {
            return Err(BatchError::InvalidRetry);
        }
        let mut selected = HashSet::new();
        let mut requests: Vec<_> = old.iter().map(|j| j.view.request.clone()).collect();
        for retry in request.jobs {
            let Some(i) = retry.id.0.checked_sub(1).filter(|i| *i < old.len()) else {
                return Err(BatchError::InvalidRetry);
            };
            if !selected.insert(i) || !old[i].view.state.can_retry() {
                return Err(BatchError::InvalidRetry);
            }
            old[i]
                .view
                .attempt
                .checked_add(1)
                .ok_or(BatchError::IdExhausted)?;
            requests[i].output = retry.output;
            requests[i].mode = request.parameters.mode;
            requests[i].limits = request.parameters.limits;
        }
        let mut jobs = planning::prepare(requests, self.shared.config.working_set_budget)?;
        for (i, job) in jobs.iter_mut().enumerate() {
            if selected.contains(&i) {
                job.view.attempt = old[i].view.attempt + 1;
            } else {
                *job = old[i].clone();
            }
        }
        let mut state = self.shared.lock();
        if state.closed {
            return Err(BatchError::Closed);
        }
        state.batch = Some(Batch::new(id, revision + 1, request.parameters, jobs));
        self.shared.changed.notify_all();
        Ok(())
    }

    /// 仅清除已经结束的批次，返回最后快照供调用方保存恢复路径；不删除输出或备份。
    /// # Errors
    /// 运行/准备中为Busy，批次不存在为NoSuchBatch。
    pub fn clear(&self, id: BatchId) -> Result<BatchSnapshot, BatchError> {
        let mut state = self.shared.lock();
        if state.preparing {
            return Err(BatchError::Busy);
        }
        let batch = state
            .batch
            .as_ref()
            .filter(|b| b.id == id)
            .ok_or(BatchError::NoSuchBatch)?;
        if batch.active() {
            return Err(BatchError::Busy);
        }
        let snapshot = batch.snapshot();
        state.batch = None;
        self.shared.changed.notify_all();
        Ok(snapshot)
    }

    /// 停止接纳并取消任务，锁外等待所有线程结束；可重复调用，可能等待不可中断的编码。
    /// # Errors
    /// worker基础设施panic或锁poison返回ServiceFault，仍会等待所有已创建线程。
    pub fn shutdown(&mut self) -> Result<(), BatchError> {
        self.shared.lock().close(false);
        self.shared.changed.notify_all();
        let mut failed = false;
        for handle in self.workers.drain(..) {
            failed |= handle.join().is_err();
        }
        let mut state = self.shared.lock();
        if failed {
            state.close(true);
        }
        // 已join全部线程后若仍有非终态，属于基础设施故障，不允许永久停在「取消中」。
        if let Some(batch) = &mut state.batch
            && batch.active()
        {
            for job in &mut batch.jobs {
                if !job.view.state.is_terminal() {
                    job.view.state =
                        JobState::Failed(JobFailure::fault(JobErrorCode::ServiceFault));
                }
            }
            batch.queue.clear();
            batch.running = 0;
            batch.reserved = 0;
            batch.revision += 1;
            state.faulted = true;
        }
        self.shared.changed.notify_all();
        if state.faulted {
            Err(BatchError::ServiceFault)
        } else {
            Ok(())
        }
    }
}
impl Drop for BatchService {
    fn drop(&mut self) {
        // Drop无法返回错误；需要诊断的调用方先显式shutdown。无论结果如何都会join。
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests;
