//! 独立GIF播放验证和固定工具实验进程入口；不启动GUI、不用于产品路径授权。
mod gif_lab;

use gif_lab::{playback::inspect, read_bounded, structure::Limits};
use serde_json::json;
use std::{
    error::Error,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let command = args
        .first()
        .and_then(|value| value.to_str())
        .ok_or("缺少命令")?;
    let report = match (command, args.len()) {
        ("corpus", 3) => gif_lab::corpus(Path::new(&args[1]), Path::new(&args[2]))?,
        ("inspect", 2) => match inspect(&read_bounded(Path::new(&args[1]), Limits::default())?, Limits::default()) {
            Ok(report) => json!({"result":"accepted","playback":report}),
            Err(error) => json!({"result":"rejected","code":error.code,"reason":error.reason}),
        },
        ("compare", 3) => {
            let before = inspect(&read_bounded(Path::new(&args[1]), Limits::default())?, Limits::default())?;
            match inspect(&read_bounded(Path::new(&args[2]), Limits::default())?, Limits::default()) {
                Ok(after) => json!({"equivalent":before.equivalent(&after),"inputFrames":before.frame_count,"outputFrames":after.frame_count}),
                Err(error) => json!({"equivalent":false,"candidateRejected":error.code,"reason":error.reason}),
            }
        }
        ("optimize", 6) => {
            inspect(&read_bounded(Path::new(&args[2]), Limits::default())?, Limits::default())?;
            if Path::new(&args[3]).exists() { return Err("实验输出必须不存在".into()); }
            let level = args[4].to_str().ok_or("优化级别无效")?;
            if !["1", "2", "3"].contains(&level) { return Err("级别为1–3".into()); }
            let careful = args[5] == "careful";
            if !careful && args[5] != "default" { return Err("策略为careful/default".into()); }
            let started = Instant::now();
            let mut command = Command::new(&args[1]);
            command.arg(format!("-O{level}"));
            if careful { command.arg("--careful"); }
            command.args(["--same-comments", "--same-extensions"]);
            command.arg("--output").arg(&args[3]).arg(&args[2]);
            command.stdout(Stdio::null()).stderr(Stdio::null()).stdin(Stdio::null());
            #[cfg(windows)] {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW，开发CLI无可见窗口。
            }
            let mut child = OwnedChild(command.spawn()?);
            let pid = sysinfo::Pid::from_u32(child.0.id());
            let mut system = System::new();
            let mut peak = 0;
            let mut samples = 0;
            let status = loop {
                system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory());
                if let Some(process) = system.process(pid).filter(|process| process.parent() == sysinfo::get_current_pid().ok()
                    && process.name().to_str().is_some_and(|name| name == "gifsicle" || name == "gifsicle.exe")) {
                    peak = peak.max(process.memory()); samples += 1;
                }
                if let Some(status) = child.0.try_wait()? { break status; }
                if started.elapsed() > Duration::from_secs(10) {
                    let _ = child.0.kill();
                    child.0.wait()?;
                    return Err("Gifsicle实验超时，已回收进程".into());
                }
                thread::sleep(Duration::from_millis(10));
            };
            if !status.success() { return Err("Gifsicle实验异常退出".into()); }
            json!({"result":"completed","elapsedMs":started.elapsed().as_millis(),"sampledPeakRssBytes":peak,"samples":samples,"samplingIntervalMs":10})
        }
        _ => return Err("参数：corpus DIR MANIFEST / inspect GIF / compare GIF GIF / optimize TOOL INPUT OUTPUT LEVEL careful|default".into()),
    };
    println!("{report}");
    Ok(())
}
