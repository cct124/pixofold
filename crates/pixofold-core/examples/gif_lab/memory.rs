//! 开发采样当前example及直属固定helper；数字仅用于观测，不控制资源许可或证明退出。
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
pub struct Sampler {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<serde_json::Value>>,
}
impl Sampler {
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let worker = thread::spawn(move || {
            let pid = sysinfo::get_current_pid().unwrap();
            let mut system = System::new();
            let (mut parent_peak, mut helper_peak, mut combined, mut samples, mut helper_samples) =
                (0, 0, 0, 0, 0);
            while !signal.load(Ordering::Acquire) {
                system.refresh_processes_specifics(
                    ProcessesToUpdate::All,
                    true,
                    ProcessRefreshKind::nothing().with_memory(),
                );
                let parent = system.process(pid).map(|p| p.memory()).unwrap_or(0);
                let children: u64 = system
                    .processes()
                    .values()
                    .filter(|p| {
                        p.parent() == Some(pid)
                            && p.name().to_str().is_some_and(|name| {
                                name.starts_with("pixofold-gif-")
                                    || name.starts_with("pixofold-jpeg-")
                            })
                    })
                    .map(|p| p.memory())
                    .sum();
                parent_peak = parent_peak.max(parent);
                helper_peak = helper_peak.max(children);
                combined = combined.max(parent.saturating_add(children));
                samples += 1;
                if children > 0 {
                    helper_samples += 1;
                }
                thread::sleep(Duration::from_millis(10));
            }
            json!({"parentPeakRssBytes":parent_peak,"helperPeakRssBytes":(helper_samples>0).then_some(helper_peak),"combinedPeakRssBytes":combined,"samples":samples,"helperSamples":helper_samples,"intervalMs":10,"scope":"example and direct fixed helpers; sampled peak, not RSS quota"})
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
    pub fn finish(mut self) -> serde_json::Value {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap()
    }
}
impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
