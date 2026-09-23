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
    pub(crate) fn page_load(&self, label: &str, event: tauri::webview::PageLoadEvent) {
        if label == "main" && matches!(event, tauri::webview::PageLoadEvent::Started) {
            // 与任务变更保持订阅→授权锁序，不依赖旧页面unload/JS清理必达。
            self.subscriptions.invalidate_page(|| self.imports.revoke());
        }
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
    fn only_main_started_revokes_page_and_preserves_native_dialog_bound() {
        use crate::ipc::{MutationError, SubscriptionError, TaskChangeAck};
        use tauri::{ipc::Channel, webview::PageLoadEvent};
        let tasks = DesktopTasks::new(TaskRuntime::new(TaskConfig::default()).unwrap()).unwrap();
        let first = tasks
            .subscriptions
            .subscribe(Channel::new(|_| Ok(())))
            .unwrap();
        tasks
            .subscriptions
            .acknowledge(TaskChangeAck {
                subscription_id: first.subscription_id,
                revision: first.revision,
            })
            .unwrap();
        let pending = tasks.imports.reserve(first.subscription_id).unwrap();
        tasks.page_load("other", PageLoadEvent::Started);
        tasks.page_load("main", PageLoadEvent::Finished);
        assert!(
            tasks
                .subscriptions
                .with_ready(first.subscription_id, || ())
                .is_ok()
        );
        tasks.page_load("main", PageLoadEvent::Started);
        assert_eq!(
            tasks.subscriptions.with_ready(first.subscription_id, || ()),
            Err(SubscriptionError::StaleSubscription)
        );
        let second = tasks
            .subscriptions
            .subscribe(Channel::new(|_| Ok(())))
            .unwrap();
        assert!(matches!(
            tasks.imports.reserve(second.subscription_id),
            Err(MutationError::SelectionBusy)
        ));
        assert!(matches!(
            pending.complete(Some(vec![std::env::temp_dir()])),
            Err(MutationError::StaleGrant)
        ));
        assert!(tasks.imports.reserve(second.subscription_id).is_ok());
        assert_eq!(tasks.control.snapshot().phase, TaskPhase::Idle);
    }

    #[test]
    fn reload_does_not_cancel_accepted_png_or_recreate_its_terminal_snapshot() {
        use crate::tasks::TaskSettings;
        use pixofold_core::import::ImportOutput;
        use std::time::{Duration, Instant};
        use tauri::webview::PageLoadEvent;
        let tasks = DesktopTasks::new(TaskRuntime::new(TaskConfig::default()).unwrap()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("reload.png");
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png/rgb8.png");
        std::fs::copy(fixture, &source).unwrap();
        let original = std::fs::read(&source).unwrap();
        let id = tasks
            .control
            .import(
                vec![source.clone()],
                Some(TaskSettings {
                    output: ImportOutput::CopyBeside,
                    ..TaskSettings::default()
                }),
            )
            .unwrap();
        tasks.page_load("main", PageLoadEvent::Started);
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut view = tasks.control.snapshot();
        while view.phase != TaskPhase::Finished {
            assert!(!matches!(
                view.phase,
                TaskPhase::Cancelled | TaskPhase::Rejected | TaskPhase::Closed
            ));
            view = tasks
                .control
                .wait_for_change(
                    view.revision,
                    deadline.saturating_duration_since(Instant::now()),
                )
                .unwrap();
        }
        assert_eq!(view.selection, Some(id));
        assert_eq!(std::fs::read(source).unwrap(), original);
        tasks.page_load("main", PageLoadEvent::Started);
        let after = tasks.control.snapshot();
        assert_eq!(after.revision, view.revision);
        assert_eq!(after.selection, view.selection);
        assert_eq!(after.phase, view.phase);
    }

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
