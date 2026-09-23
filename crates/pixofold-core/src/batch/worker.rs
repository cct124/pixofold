//! 固定worker取队列项、执行pipeline并交还预算；外部编码和阶段回调不持有状态锁。

use super::*;
use crate::model::{PngRequest, ProcessingError, ProcessingOutcome, ProcessingStage};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

struct Work {
    batch: BatchId,
    index: usize,
    attempt: u32,
    request: PngRequest,
    cancel: CancellationToken,
    reservation: u64,
}

fn take(shared: &Shared) -> Option<Work> {
    let mut state = shared.lock();
    loop {
        if state.closed {
            return None;
        }
        if let Some(batch) = &mut state.batch
            && batch.cancel.is_cancelled()
        {
            let revision = batch.revision;
            batch.cancel(false);
            if batch.revision != revision {
                shared.changed.notify_all();
            }
        }
        if let Some(batch) = &mut state.batch
            && let Some(index) = batch.queue.front().copied()
        {
            let job = &mut batch.jobs[index];
            if job.reservation.0 <= shared.config.working_set_budget.0 - batch.reserved {
                batch.queue.pop_front();
                job.view.state = JobState::Running {
                    stage: ProcessingStage::Reading,
                    cancel_requested: false,
                };
                batch.running += 1;
                batch.reserved += job.reservation.0;
                batch.revision += 1;
                let work = Work {
                    batch: batch.id,
                    index,
                    attempt: job.view.attempt,
                    request: job.view.request.clone(),
                    cancel: batch.cancel.clone(),
                    reservation: job.reservation.0,
                };
                shared.changed.notify_all();
                return Some(work);
            }
        }
        state = shared.wait(state);
    }
}

fn order(stage: ProcessingStage) -> u8 {
    match stage {
        ProcessingStage::Reading => 0,
        ProcessingStage::Optimizing => 1,
        ProcessingStage::Validating => 2,
        ProcessingStage::BeforeCommit => 3,
    }
}

fn stage(shared: &Shared, work: &Work, next: ProcessingStage) {
    let mut state = shared.lock();
    if let Some(batch) = state.batch.as_mut().filter(|b| b.id == work.batch)
        && let Some(job) = batch
            .jobs
            .get_mut(work.index)
            .filter(|j| j.view.attempt == work.attempt)
        && let JobState::Running { stage, .. } = &mut job.view.state
        && order(next) > order(*stage)
    {
        *stage = next;
        batch.revision += 1;
        shared.changed.notify_all();
    }
}

pub(super) fn run(shared: Arc<Shared>, runner: Arc<dyn Runner>) {
    while let Some(work) = take(&shared) {
        let result = catch_unwind(AssertUnwindSafe(|| {
            runner.run(&work.request, &work.cancel, &mut |next| {
                stage(&shared, &work, next)
            })
        }));
        let terminal = match result {
            Ok(Ok(report)) => match report.outcome {
                ProcessingOutcome::Optimized { .. } => JobState::Succeeded(report),
                ProcessingOutcome::NoGain => JobState::NoGain(report),
            },
            Ok(Err(ProcessingError::Cancelled)) => JobState::Cancelled,
            Ok(Err(error)) => JobState::Failed(JobFailure::processing(error)),
            Err(_) => JobState::Failed(JobFailure::fault(JobErrorCode::WorkerPanicked)),
        };
        // 不再复查token来覆盖成功：编码器可能已经越过提交临界点。
        let mut state = shared.lock();
        if let Some(batch) = state.batch.as_mut().filter(|b| b.id == work.batch) {
            let job = &mut batch.jobs[work.index];
            if job.view.attempt == work.attempt
                && matches!(job.view.state, JobState::Running { .. })
            {
                if let JobState::Succeeded(report) | JobState::NoGain(report) = &terminal {
                    job.view.input_bytes = Some(report.input_bytes);
                }
                job.view.state = terminal;
                batch.running -= 1;
                batch.reserved -= work.reservation;
                batch.revision += 1;
                shared.changed.notify_all();
            }
        }
    }
}
