//! 实验进程的CPU累计时间及工作集采样，覆盖读取/验证/编码；不是RSS硬限制。
use serde_json::{Value, json};
use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
fn sample(system: &mut System, pid: Pid) -> (u64, u64) {
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory().with_cpu(),
    );
    let process = system.process(pid).expect("sampling current process");
    (process.memory(), process.accumulated_cpu_time())
}
pub struct Sampler {
    stop: mpsc::Sender<()>,
    worker: Option<JoinHandle<Value>>,
}
impl Sampler {
    pub fn start() -> Self {
        let pid = sysinfo::get_current_pid().expect("current process id");
        let mut system = System::new();
        let (initial, base_cpu) = sample(&mut system, pid);
        let (stop, wait) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut peak = initial;
            let mut samples = 1;
            loop {
                let done = wait.recv_timeout(Duration::from_millis(10)).is_ok();
                let (rss, cpu) = sample(&mut system, pid);
                peak = peak.max(rss);
                samples += 1;
                if done {
                    return json!({"peakRssBytes":peak,"cpuMs":cpu.saturating_sub(base_cpu),"samples":samples,"intervalMs":10,"scope":"entire experiment process including sampler; sampled RSS, not quota"});
                }
            }
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
    pub fn finish(mut self) -> Value {
        let _ = self.stop.send(());
        self.worker
            .take()
            .expect("single owner")
            .join()
            .expect("sampler")
    }
}
impl Drop for Sampler {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
