//! 单协调线程执行扫描/规划及核心调用；快照最多100ms合并一次批次变化，不伪造进度。
//! 空闲使用Condvar等待；100ms只用于活跃批次采样，后续Channel可直接等待应用revision。

use super::*;
use pixofold_core::{
    batch::{BatchError, BatchPhase, BatchRequest},
    import::{self, ImportError, ScanOptions, ScanProgress, ScanStatus},
};
use std::panic::{AssertUnwindSafe, catch_unwind};

const REFRESH: Duration = Duration::from_millis(100);
const IDLE_WAIT: Duration = Duration::from_secs(3600);

// 私有测试接缝仅替换只读导入，生产不能替换编码器或绕过可靠输出。
pub(super) trait ImportBackend: Send + Sync {
    fn scan(
        &self,
        roots: &[PathBuf],
        options: ScanOptions,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(&ScanProgress),
    ) -> Result<ImportScan, ImportError>;
    fn plan(&self, scan: &ImportScan, settings: &TaskSettings)
    -> Result<BatchRequest, ImportError>;
}
pub(super) struct NativeImport;
impl ImportBackend for NativeImport {
    fn scan(
        &self,
        roots: &[PathBuf],
        options: ScanOptions,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(&ScanProgress),
    ) -> Result<ImportScan, ImportError> {
        import::scan(roots, options, cancel, progress)
    }
    fn plan(
        &self,
        scan: &ImportScan,
        settings: &TaskSettings,
    ) -> Result<BatchRequest, ImportError> {
        scan.plan(&settings.output, settings.parameters)
    }
}

fn publish_batch(shared: &Shared, service: &BatchService) {
    let batch = service.snapshot().map(Arc::new);
    let mut state = shared.lock();
    if let Some(batch) = batch {
        if state.bound_batch != Some(batch.id) {
            return;
        }
        let phase = if batch.phase == BatchPhase::Finished {
            TaskPhase::Finished
        } else if state.cancel.is_cancelled() {
            TaskPhase::Cancelling
        } else {
            TaskPhase::Running
        };
        let changed = state
            .view
            .batch
            .as_ref()
            .is_none_or(|old| old.id != batch.id || old.revision != batch.revision)
            || (!state.closing && state.view.phase != phase);
        state.view.batch = Some(batch);
        if !state.closing {
            state.view.phase = phase;
        }
        if changed {
            shared.publish(&mut state);
        }
    }
}

fn prepare(
    shared: &Shared,
    service: &BatchService,
    backend: &dyn ImportBackend,
    scan: &ImportScan,
    settings: &TaskSettings,
    cancel: CancellationToken,
) -> Result<(), TaskError> {
    let result = if cancel.is_cancelled() {
        Err(TaskError::from(BatchError::Cancelled))
    } else {
        backend
            .plan(scan, settings)
            .map_err(TaskError::from)
            .and_then(|request| {
                service
                    .start_with_cancel(request, cancel.clone())
                    .map_err(TaskError::from)
            })
    };
    match result {
        Ok(id) => {
            shared.lock().bound_batch = Some(id);
            publish_batch(shared, service);
        }
        Err(error) => {
            if matches!(&error, TaskError::Batch(e) if matches!(e.as_ref(), BatchError::Closed | BatchError::ServiceFault))
            {
                return Err(error);
            }
            let mut state = shared.lock();
            if !state.closing {
                state.view.phase = if cancel.is_cancelled() {
                    TaskPhase::Cancelled
                } else {
                    TaskPhase::Ready
                };
            }
            state.view.error = Some(error);
            shared.publish(&mut state);
        }
    }
    Ok(())
}

fn execute(
    shared: &Shared,
    service: &BatchService,
    options: ScanOptions,
    backend: &dyn ImportBackend,
    command: Command,
    cancel: CancellationToken,
) -> Result<(), TaskError> {
    match command {
        Command::Import { roots, settings } => {
            if let Some(batch) = service.snapshot() {
                service.clear(batch.id)?;
            }
            let result = backend.scan(&roots, options, &cancel, &mut |progress| {
                let mut state = shared.lock();
                state.view.scan_progress = Some(*progress);
                shared.publish(&mut state);
            });
            match result {
                Err(error) => {
                    let mut state = shared.lock();
                    state.view.error = Some(error.into());
                    if !state.closing {
                        state.view.phase = if cancel.is_cancelled() {
                            TaskPhase::Cancelled
                        } else {
                            TaskPhase::Rejected
                        };
                    }
                    shared.publish(&mut state);
                }
                Ok(scan) => {
                    let scan = Arc::new(scan);
                    let ready =
                        scan.progress().status == ScanStatus::Complete && !scan.files().is_empty();
                    let auto;
                    {
                        let mut state = shared.lock();
                        state.view.scan_progress = Some(scan.progress());
                        state.view.import = Some(scan.clone());
                        auto =
                            ready && settings.is_some() && !cancel.is_cancelled() && !state.closing;
                        if !state.closing {
                            state.view.phase = if cancel.is_cancelled()
                                || scan.progress().status == ScanStatus::Cancelled
                            {
                                TaskPhase::Cancelled
                            } else if !ready {
                                TaskPhase::Rejected
                            } else if auto {
                                TaskPhase::Preparing
                            } else {
                                TaskPhase::Ready
                            };
                        }
                        if !ready {
                            state.view.error = Some(
                                if scan.progress().status == ScanStatus::Complete {
                                    ImportError::NoFiles
                                } else {
                                    ImportError::IncompleteScan
                                }
                                .into(),
                            );
                        }
                        shared.publish(&mut state);
                    }
                    // 自动启动前不暴露短暂Ready窗口，避免前端补发start成为第二个启动。
                    if auto && let Some(settings) = settings {
                        prepare(shared, service, backend, &scan, &settings, cancel)?;
                    }
                }
            }
        }
        Command::Start { scan, settings } => {
            prepare(shared, service, backend, &scan, &settings, cancel)?
        }
        Command::Retry(request) => {
            let batch = service.snapshot().ok_or(TaskError::NotReady)?;
            if let Err(error) = service.retry_with_cancel(batch.id, request, cancel) {
                let mut state = shared.lock();
                state.view.error = Some(error.into());
                shared.publish(&mut state);
            }
            publish_batch(shared, service);
        }
        Command::Clear => {
            if let Some(batch) = service.snapshot() {
                service.clear(batch.id)?;
            }
            let mut state = shared.lock();
            state.view.selection = None;
            state.view.import = None;
            state.view.scan_progress = None;
            state.view.batch = None;
            state.bound_batch = None;
            state.view.error = None;
            if !state.closing {
                state.view.phase = TaskPhase::Idle;
            }
            shared.publish(&mut state);
        }
    }
    Ok(())
}

fn coordinate(
    shared: &Shared,
    service: &BatchService,
    options: ScanOptions,
    backend: &dyn ImportBackend,
) -> Result<(), TaskError> {
    loop {
        let mut state = shared.lock();
        if state.closing {
            return Ok(());
        }
        if let Some(command) = state.pending.take() {
            let cancel = state.cancel.clone();
            drop(state);
            execute(shared, service, options, backend, command, cancel)?;
            continue;
        }
        let active = matches!(state.view.phase, TaskPhase::Running | TaskPhase::Cancelling);
        if active {
            let cancelled = state.cancel.is_cancelled();
            drop(state);
            if cancelled && let Some(batch) = service.snapshot() {
                service.cancel(batch.id)?;
            }
            publish_batch(shared, service);
            state = shared.lock();
        }
        // 命令可能在采样过程中接纳，必须重新检查再等待，避免丢失notify。
        if state.pending.is_none() && !state.closing {
            drop(shared.wait(state, if active { REFRESH } else { IDLE_WAIT }));
        }
    }
}

pub(super) fn run(
    shared: Arc<Shared>,
    mut service: BatchService,
    options: ScanOptions,
    backend: Arc<dyn ImportBackend>,
) -> Result<(), TaskError> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        coordinate(&shared, &service, options, backend.as_ref())
    }));
    let mut error = match result {
        Ok(result) => result.err(),
        Err(_) => Some(TaskError::WorkerPanicked),
    };
    if let Err(e) = service.shutdown() {
        error.get_or_insert(e.into());
    }
    let batch = service.snapshot().map(Arc::new);
    let mut state = shared.lock();
    if state.faulted {
        error.get_or_insert(TaskError::ServiceFault);
    }
    state.closing = true;
    state.cancel.cancel();
    state.pending = None;
    // 导入已接纳但尚未执行时关闭，核心可能仍保留上一批，不能挂到新selection下。
    state.view.batch = batch.filter(|batch| state.bound_batch == Some(batch.id));
    state.view.phase = TaskPhase::Closed;
    if let Some(error) = &error {
        state.view.error = Some(error.clone());
    }
    shared.publish(&mut state);
    error.map_or(Ok(()), Err)
}
