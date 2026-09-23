//! 桌面退出桥：先停止接纳，在唯一后台收尾任务join，再允许事件循环退出。
//! 常规关闭/退出可等待；OS强杀、断电与未接入的重启路径不提供恢复保证。

use crate::{
    ingress::NativeImports,
    subscriptions::{SubscriptionControl, SubscriptionRuntime},
    tasks::{TaskControl, TaskRuntime},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Manager};

pub(crate) struct DesktopTasks {
    runtime: Mutex<Option<DesktopRuntime>>,
    pub(crate) control: TaskControl,
    pub(crate) subscriptions: SubscriptionControl,
    pub(crate) imports: Arc<NativeImports>,
    ready_to_exit: AtomicBool,
}
struct DesktopRuntime {
    // 兜底Drop也先停止订阅，再停止任务，不能让通知线程依赖窗口释放。
    subscriptions: SubscriptionRuntime,
    tasks: TaskRuntime,
}
impl DesktopTasks {
    pub(crate) fn new(runtime: TaskRuntime) -> Result<Self, std::io::Error> {
        let subscriptions = SubscriptionRuntime::new(runtime.control())?;
        Ok(Self {
            control: runtime.control(),
            subscriptions: subscriptions.control(),
            imports: Arc::new(NativeImports::default()),
            runtime: Mutex::new(Some(DesktopRuntime {
                subscriptions,
                tasks: runtime,
            })),
            ready_to_exit: AtomicBool::new(false),
        })
    }
    pub(crate) fn ready(&self) -> bool {
        self.ready_to_exit.load(Ordering::Acquire)
    }
    fn take_for_shutdown(&self) -> Option<(DesktopRuntime, bool)> {
        self.subscriptions.request_close();
        self.imports.close();
        self.control.request_close();
        // 只在锁内转移所有权；OS/I/O和join均不持锁，且不会再次创建收尾线程。
        match self.runtime.lock() {
            Ok(mut runtime) => runtime.take().map(|runtime| (runtime, false)),
            Err(error) => error.into_inner().take().map(|runtime| (runtime, true)),
        }
    }
}

pub(crate) fn request_exit(app: &AppHandle, code: i32) {
    let tasks = app.state::<DesktopTasks>();
    if let Some((mut runtime, owner_fault)) = tasks.take_for_shutdown() {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            // 即使某个服务收尾失败，另一个也必须join；不得用?提前返回。
            let subscriptions = runtime.subscriptions.shutdown();
            let result = runtime.tasks.shutdown();
            let failed = subscriptions.is_err() || result.is_err() || owner_fault;
            if let Err(error) = subscriptions {
                eprintln!("PixoFold 订阅收尾失败：{error}");
            }
            if let Err(error) = result {
                // 只记录无路径的类别；失败也已经等待核心线程，不隐瞒异常退出。
                eprintln!("PixoFold 任务收尾失败：{error}");
            }
            if owner_fault {
                eprintln!("PixoFold 任务所有权锁异常，线程已收尾");
            }
            let exit_code = if failed { 1 } else { code };
            app.state::<DesktopTasks>()
                .ready_to_exit
                .store(true, Ordering::Release);
            app.exit(exit_code);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{TaskConfig, TaskError, TaskPhase};

    #[test]
    fn repeated_exit_takes_owner_once_and_closes_all_handles() {
        let tasks = DesktopTasks::new(TaskRuntime::new(TaskConfig::default()).unwrap()).unwrap();
        let old_handle = tasks.control.clone();
        let subscription_handle = tasks.subscriptions.clone();
        let pending_selection = tasks.imports.reserve(crate::ipc::DecimalU64(1)).unwrap();
        let ticket = subscription_handle
            .subscribe(tauri::ipc::Channel::new(|_| Ok(())))
            .unwrap();
        let (mut owner, fault) = tasks.take_for_shutdown().unwrap();
        assert!(!fault);
        assert!(tasks.take_for_shutdown().is_none());
        assert!(!tasks.ready());
        assert!(matches!(
            pending_selection.complete(Some(vec![std::env::temp_dir()])),
            Err(crate::ipc::MutationError::Closed)
        ));
        assert_eq!(
            subscription_handle.acknowledge(crate::ipc::TaskChangeAck {
                subscription_id: ticket.subscription_id,
                revision: ticket.revision,
            }),
            Err(crate::ipc::SubscriptionError::Closed)
        );
        assert!(matches!(
            old_handle.import(vec![], None),
            Err(TaskError::Closed)
        ));
        owner.subscriptions.shutdown().unwrap();
        owner.tasks.shutdown().unwrap();
        assert_eq!(old_handle.snapshot().phase, TaskPhase::Closed);
        assert_eq!(
            subscription_handle.subscribe(tauri::ipc::Channel::new(|_| Ok(()))),
            Err(crate::ipc::SubscriptionError::Closed)
        );
    }
}
