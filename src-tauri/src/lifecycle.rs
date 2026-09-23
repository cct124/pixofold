//! 桌面退出桥：先停止接纳，在唯一后台收尾任务join，再允许事件循环退出。
//! 常规关闭/退出可等待；OS强杀、断电与未接入的重启路径不提供恢复保证。

use crate::tasks::{TaskControl, TaskRuntime};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Manager};

pub(crate) struct DesktopTasks {
    runtime: Mutex<Option<TaskRuntime>>,
    pub(crate) control: TaskControl,
    ready_to_exit: AtomicBool,
}
impl DesktopTasks {
    pub(crate) fn new(runtime: TaskRuntime) -> Self {
        Self {
            control: runtime.control(),
            runtime: Mutex::new(Some(runtime)),
            ready_to_exit: AtomicBool::new(false),
        }
    }
    pub(crate) fn ready(&self) -> bool {
        self.ready_to_exit.load(Ordering::Acquire)
    }
    fn take_for_shutdown(&self) -> Option<(TaskRuntime, bool)> {
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
            let result = runtime.shutdown();
            let exit_code = if let Err(error) = result {
                // 只记录无路径的类别；失败也已经等待核心线程，不隐瞒异常退出。
                eprintln!("PixoFold 任务收尾失败：{error}");
                1
            } else if owner_fault {
                eprintln!("PixoFold 任务所有权锁异常，线程已收尾");
                1
            } else {
                code
            };
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
        let tasks = DesktopTasks::new(TaskRuntime::new(TaskConfig::default()).unwrap());
        let old_handle = tasks.control.clone();
        let (mut owner, fault) = tasks.take_for_shutdown().unwrap();
        assert!(!fault);
        assert!(tasks.take_for_shutdown().is_none());
        assert!(!tasks.ready());
        assert!(matches!(
            old_handle.import(vec![], None),
            Err(TaskError::Closed)
        ));
        owner.shutdown().unwrap();
        assert_eq!(old_handle.snapshot().phase, TaskPhase::Closed);
    }
}
