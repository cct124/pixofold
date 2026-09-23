//! 应用所有的有界只读通知服务：一个线程、一个订阅、最多一条待确认票据。
//! 变化不排队；确认后直接读取最新revision，查询器仍是任务明细的唯一来源。
//! Channel发送/释放和join在锁外；替换或断开不取消任务，旧会话不能操作新会话。

use crate::{
    ipc::{DecimalU64, SubscriptionError, TASK_PROTOCOL_VERSION, TaskChangeAck, TaskChangeNotice},
    tasks::{TaskControl, TaskError, TaskPhase},
};
use std::{
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::JoinHandle,
    time::Duration,
};
use tauri::ipc::Channel;

// 两个服务有独立Condvar；限制任务等待时间，以便替换/退出不依赖新的任务变化。
const CONTROL_WAIT: Duration = Duration::from_millis(100);

struct Session {
    id: u64,
    channel: Channel<TaskChangeNotice>,
    acknowledged: Option<u64>,
    in_flight: Option<u64>,
    terminal: bool,
}
struct State {
    session: Option<Session>,
    next_id: u64,
    closed: bool,
    faulted: bool,
}
impl State {
    fn available(&self) -> Result<(), SubscriptionError> {
        if self.faulted {
            Err(SubscriptionError::ServiceFault)
        } else if self.closed {
            Err(SubscriptionError::Closed)
        } else {
            Ok(())
        }
    }
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}
impl Shared {
    fn lock(&self) -> Result<MutexGuard<'_, State>, SubscriptionError> {
        self.state
            .lock()
            .map_err(|_| SubscriptionError::ServiceFault)
    }
    fn close(&self, faulted: bool) {
        let retired = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.closed = true;
            state.faulted |= faulted;
            let retired = state.session.take();
            self.changed.notify_all();
            retired
        };
        // Channel::drop会向WebView发送end，不能在状态锁内执行。
        drop(retired);
    }
}

#[derive(Clone)]
pub(crate) struct SubscriptionControl {
    shared: Arc<Shared>,
    tasks: TaskControl,
}
impl SubscriptionControl {
    /// 将一次短小的接纳操作绑定到当前已ACK会话；替换/取消订阅不能从校验与接纳之间穿过。
    /// 闭包不得包含文件I/O、Channel发送、await或等待；锁序为订阅→授权槽→任务。
    pub(crate) fn with_ready<T>(
        &self,
        id: DecimalU64,
        action: impl FnOnce() -> T,
    ) -> Result<T, SubscriptionError> {
        let state = self.shared.lock()?;
        state.available()?;
        let session = state
            .session
            .as_ref()
            .filter(|s| s.id == id.0)
            .ok_or(SubscriptionError::StaleSubscription)?;
        if session.acknowledged.is_none() {
            return Err(SubscriptionError::InvalidAcknowledgement);
        }
        Ok(action())
    }

    /// 替换唯一会话，返回首次待确认票据；确认之前不向Channel发送任何通知。
    pub(crate) fn subscribe(
        &self,
        channel: Channel<TaskChangeNotice>,
    ) -> Result<TaskChangeNotice, SubscriptionError> {
        let snapshot = self.tasks.snapshot();
        let (notice, retired) = {
            let mut state = self.shared.lock()?;
            state.available()?;
            let id = state.next_id;
            state.next_id = id.checked_add(1).ok_or(SubscriptionError::IdExhausted)?;
            let retired = state.session.replace(Session {
                id,
                channel,
                acknowledged: None,
                in_flight: Some(snapshot.revision),
                terminal: snapshot.phase == TaskPhase::Closed,
            });
            self.shared.changed.notify_all();
            (notice(id, snapshot.revision), retired)
        };
        drop(retired);
        Ok(notice)
    }

    /// 仅匹配会话的在途revision可解锁；重复已确认revision为幂等操作。
    pub(crate) fn acknowledge(&self, ack: TaskChangeAck) -> Result<(), SubscriptionError> {
        let mut state = self.shared.lock()?;
        state.available()?;
        let session = state
            .session
            .as_mut()
            .filter(|session| session.id == ack.subscription_id.0)
            .ok_or(SubscriptionError::StaleSubscription)?;
        if session.acknowledged == Some(ack.revision.0) {
            return Ok(());
        }
        if session.in_flight != Some(ack.revision.0) {
            return Err(SubscriptionError::InvalidAcknowledgement);
        }
        session.acknowledged = session.in_flight.take();
        self.shared.changed.notify_all();
        Ok(())
    }

    /// 销毁匹配会话；旧ID返回false，不影响替换后的会话或任何任务。
    pub(crate) fn unsubscribe(&self, id: DecimalU64) -> Result<bool, SubscriptionError> {
        let retired = {
            let mut state = self.shared.lock()?;
            if state
                .session
                .as_ref()
                .is_some_and(|session| session.id == id.0)
            {
                let retired = state.session.take();
                self.shared.changed.notify_all();
                retired
            } else {
                None
            }
        };
        let removed = retired.is_some();
        drop(retired);
        Ok(removed)
    }

    pub(crate) fn request_close(&self) {
        self.shared.close(false);
    }
}

/// 仅应用创建一次；Drop和显式shutdown都会停止接纳并join，不依赖WebView存活。
pub(crate) struct SubscriptionRuntime {
    control: SubscriptionControl,
    thread: Option<JoinHandle<Result<(), SubscriptionError>>>,
}
impl SubscriptionRuntime {
    pub(crate) fn new(tasks: TaskControl) -> Result<Self, io::Error> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                session: None,
                next_id: 1,
                closed: false,
                faulted: false,
            }),
            changed: Condvar::new(),
        });
        let control = SubscriptionControl {
            shared: shared.clone(),
            tasks: tasks.clone(),
        };
        let thread = std::thread::Builder::new()
            .name("pixofold-task-notifications".into())
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| pump(&shared, &tasks)))
                    .unwrap_or(Err(SubscriptionError::ServiceFault));
                shared.close(result.is_err());
                result
            })?;
        Ok(Self {
            control,
            thread: Some(thread),
        })
    }
    pub(crate) fn control(&self) -> SubscriptionControl {
        self.control.clone()
    }
    pub(crate) fn shutdown(&mut self) -> Result<(), SubscriptionError> {
        self.control.request_close();
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| SubscriptionError::ServiceFault)?
        } else {
            Ok(())
        }
    }
}
impl Drop for SubscriptionRuntime {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn notice(id: u64, revision: u64) -> TaskChangeNotice {
    TaskChangeNotice {
        protocol_version: TASK_PROTOCOL_VERSION,
        subscription_id: DecimalU64(id),
        revision: DecimalU64(revision),
    }
}

fn pump(shared: &Shared, tasks: &TaskControl) -> Result<(), SubscriptionError> {
    loop {
        let (id, after) = {
            let mut state = shared.lock()?;
            loop {
                if state.closed {
                    return if state.faulted {
                        Err(SubscriptionError::ServiceFault)
                    } else {
                        Ok(())
                    };
                }
                if let Some(session) = &state.session
                    && session.in_flight.is_none()
                    && !session.terminal
                    && let Some(after) = session.acknowledged
                {
                    break (session.id, after);
                }
                state = shared
                    .changed
                    .wait(state)
                    .map_err(|_| SubscriptionError::ServiceFault)?;
            }
        };
        let snapshot = match tasks.wait_for_change(after, CONTROL_WAIT) {
            Ok(snapshot) => snapshot,
            Err(TaskError::TimedOut) => continue,
            Err(_) => return Err(SubscriptionError::ServiceFault),
        };
        let channel = {
            let mut state = shared.lock()?;
            let Some(session) = state.session.as_mut().filter(|session| {
                session.id == id
                    && session.acknowledged == Some(after)
                    && session.in_flight.is_none()
            }) else {
                continue;
            };
            session.terminal = snapshot.phase == TaskPhase::Closed;
            if snapshot.revision <= after {
                continue;
            }
            session.in_flight = Some(snapshot.revision);
            session.channel.clone()
        };
        // 替换/关闭可与投递并发；会话ID过滤迟到消息，旧发送失败不能移除新会话。
        if channel.send(notice(id, snapshot.revision)).is_err() {
            let retired = {
                let mut state = shared.lock()?;
                if state
                    .session
                    .as_ref()
                    .is_some_and(|session| session.id == id)
                {
                    state.session.take()
                } else {
                    None
                }
            };
            drop(retired);
        }
    }
}

#[cfg(test)]
mod tests;
