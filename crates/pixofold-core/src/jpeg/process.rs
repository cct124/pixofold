//! 自有单一子进程的有界管道与回收。可信helper不派生子进程，不接受脚本/shell。
//! 三个I/O线程只搬运字节；所有返回路径先kill/wait，再join，最后清理工作目录。

use super::{JpegError, io_error};
use crate::model::CancellationToken;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub(super) struct Capture {
    pub bytes: Vec<u8>,
    pub digest: [u8; 32],
    pub length: u64,
    pub prefix: Vec<u8>,
}

struct OwnedChild(Child);
impl OwnedChild {
    fn reap(&mut self, stop: bool) -> Result<std::process::ExitStatus, JpegError> {
        if stop
            && self
                .0
                .try_wait()
                .map_err(|e| io_error("检查进程", e))?
                .is_none()
        {
            // kill与进程自退存在竞争；以wait确认回收，不把已退出误报为kill失败。
            // 若wait本身失败仍显式报告，Drop再做最后回收尝试。
            let _ = self.0.kill();
            let status = self.0.wait().map_err(|e| io_error("回收进程", e))?;
            return Ok(status);
        }
        self.0.wait().map_err(|e| io_error("回收进程", e))
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        // panic/线程创建失败的最后防线；正常错误通过reap显式报告。
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _status = self.0.wait();
        #[cfg(test)]
        observe::reaped(self.0.id(), _status);
    }
}

fn capture(mut reader: impl Read, limit: u64, keep: bool) -> Result<Capture, JpegError> {
    let mut result = Capture {
        bytes: Vec::new(),
        digest: [0; 32],
        length: 0,
        prefix: Vec::new(),
    };
    let mut hash = Sha256::new();
    let mut block = [0u8; 8192];
    loop {
        let count = reader
            .read(&mut block)
            .map_err(|e| io_error("读取管道", e))?;
        if count == 0 {
            break;
        }
        result.length += count as u64;
        if result.length > limit {
            return Err(JpegError::ResourceLimit("进程输出管道"));
        }
        if result.prefix.len() < 21 {
            result
                .prefix
                .extend_from_slice(&block[..count.min(21 - result.prefix.len())]);
        }
        hash.update(&block[..count]);
        if keep {
            result.bytes.extend_from_slice(&block[..count]);
        }
    }
    result.digest = hash.finalize().into();
    Ok(result)
}

pub(super) fn run(
    command: &mut Command,
    input: &[u8],
    stdout_limit: u64,
    keep: bool,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Capture, JpegError> {
    cancel.check()?;
    let workspace = tempfile::Builder::new()
        .prefix("pixofold-jpeg-")
        .tempdir()
        .map_err(|e| io_error("创建引擎工作目录", e))?;
    command
        .current_dir(workspace.path())
        .env("TMP", workspace.path())
        .env("TEMP", workspace.path())
        .env("TMPDIR", workspace.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW，不闪现控制台。
    }
    let result = run_inner(command, input, stdout_limit, keep, timeout, cancel);
    let temporary = workspace.path().to_owned();
    match workspace.close() {
        Ok(()) => result,
        Err(source) => Err(JpegError::Cleanup {
            original: result.err().map(Box::new),
            source,
            temporary,
        }),
    }
}

fn run_inner(
    command: &mut Command,
    input: &[u8],
    limit: u64,
    keep: bool,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Capture, JpegError> {
    let started = Instant::now();
    thread::scope(|scope| {
        // child在scope闭包内创建，提前返回时先drop子进程，再由scope等候I/O。
        let mut child = OwnedChild(command.spawn().map_err(|e| io_error("启动进程", e))?);
        #[cfg(test)]
        observe::notify(ProcessEvent::Spawned {
            pid: child.0.id(),
            workspace: command
                .get_current_dir()
                .expect("run sets the workspace")
                .to_owned(),
        });
        let mut stdin = child.0.stdin.take().ok_or(JpegError::ValidationFailed)?;
        let stdout = child.0.stdout.take().ok_or(JpegError::ValidationFailed)?;
        let stderr = child.0.stderr.take().ok_or(JpegError::ValidationFailed)?;
        let (errors, receiver) = mpsc::channel();
        let send = errors.clone();
        let writer = thread::Builder::new()
            .name("jpeg-stdin".into())
            .spawn_scoped(scope, move || {
                let result = stdin.write_all(input).map_err(|e| io_error("写入管道", e));
                drop(stdin);
                if result.is_err() {
                    let _ = send.send(());
                }
                result
            })
            .map_err(|e| io_error("创建输入线程", e))?;
        let send = errors.clone();
        let reader = thread::Builder::new()
            .name("jpeg-stdout".into())
            .spawn_scoped(scope, move || {
                let result = capture(stdout, limit, keep);
                if result.is_err() {
                    let _ = send.send(());
                }
                result
            })
            .map_err(|e| io_error("创建输出线程", e))?;
        let diagnostics = thread::Builder::new()
            .name("jpeg-stderr".into())
            .spawn_scoped(scope, move || {
                // stderr即使包含私人内容也从不保留/打印；超限立即通知所有者终结进程。
                let result = capture(stderr, 16 * 1024, false).map(|_| ());
                if result.is_err() {
                    let _ = errors.send(());
                }
                result
            })
            .map_err(|e| io_error("创建诊断线程", e))?;
        let mut failure = None;
        let stop = loop {
            if cancel.is_cancelled() {
                failure = Some(JpegError::Cancelled);
                break true;
            }
            if started.elapsed() >= timeout {
                failure = Some(JpegError::Timeout);
                break true;
            }
            match child.0.try_wait() {
                Ok(Some(_)) => break false,
                Ok(None) => {}
                Err(error) => {
                    failure = Some(io_error("检查进程", error));
                    break true;
                }
            }
            match receiver.recv_timeout(Duration::from_millis(5)) {
                Ok(()) => break true,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    thread::sleep(Duration::from_millis(1));
                }
            }
        };
        let status = child.reap(stop);
        // 即使第一个join返回错误，也先收回其余线程；scope同时提供panic兜底。
        let write_result = writer.join().map_err(|_| JpegError::ValidationFailed);
        let read_result = reader.join().map_err(|_| JpegError::ValidationFailed);
        let diagnostic_result = diagnostics.join().map_err(|_| JpegError::ValidationFailed);
        let status = status?;
        if let Some(error) = failure {
            return Err(error);
        }
        let output = read_result??;
        diagnostic_result??;
        if !status.success() {
            return Err(JpegError::ToolExit(status.code()));
        }
        write_result??;
        cancel.check()?;
        Ok(output)
    })
}

/// 仅测试观察实际所有者的原生wait结果，不用全局PID快照代替原进程退出证据。
#[cfg(test)]
#[derive(Debug)]
pub(crate) enum ProcessEvent {
    Spawned {
        pid: u32,
        workspace: std::path::PathBuf,
    },
    Reaped {
        pid: u32,
        status: std::io::Result<std::process::ExitStatus>,
        resume: Option<mpsc::Sender<()>>,
    },
}

#[cfg(test)]
pub(crate) struct ProcessObserver {
    pub events: mpsc::Sender<ProcessEvent>,
    pub hold_reaped: bool,
}

#[cfg(test)]
pub(crate) mod observe {
    use super::{ProcessEvent, ProcessObserver};
    use std::{cell::RefCell, sync::mpsc};

    thread_local! {
        static EVENTS: RefCell<Option<ProcessObserver>> = const { RefCell::new(None) };
    }

    pub(super) fn notify(event: ProcessEvent) {
        EVENTS.with_borrow(|observer| {
            if let Some(observer) = observer {
                let _ = observer.events.send(event);
            }
        });
    }

    pub(super) fn reaped(pid: u32, status: std::io::Result<std::process::ExitStatus>) {
        let receiver = EVENTS.with_borrow(|observer| {
            let observer = observer.as_ref()?;
            let (resume, receiver) = mpsc::channel();
            observer
                .events
                .send(ProcessEvent::Reaped {
                    pid,
                    status,
                    resume: observer.hold_reaped.then_some(resume),
                })
                .ok()?;
            observer.hold_reaped.then_some(receiver)
        });
        // 只在测试门闩上等待；接收者/许可被丢弃时立即放行，允许panic安全收尾。
        if let Some(receiver) = receiver {
            let _ = receiver.recv();
        }
    }

    /// 观察仅绑定当前worker，panic也恢复原观察者，不影响并行测试或生产构建。
    pub(crate) fn with<T>(observer: ProcessObserver, run: impl FnOnce() -> T) -> T {
        struct Reset(Option<ProcessObserver>);
        impl Drop for Reset {
            fn drop(&mut self) {
                EVENTS.with_borrow_mut(|slot| *slot = self.0.take());
            }
        }
        let _reset = Reset(EVENTS.with_borrow_mut(|slot| slot.replace(observer)));
        run()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "jpeg::process::tests::child_fixture",
                "--nocapture",
            ])
            .env("PIXOFOLD_PROCESS_FIXTURE", mode);
        command
    }
    #[test]
    fn child_fixture() {
        let Ok(mode) = std::env::var("PIXOFOLD_PROCESS_FIXTURE") else {
            return;
        };
        if let Some(ready) = std::env::var_os("PIXOFOLD_PROCESS_READY") {
            // 完整帧才表示ready；读者不能把部分PID当成另一个有效进程。
            std::fs::write(ready, format!("ready:{}\n", std::process::id())).unwrap();
        }
        match mode.as_str() {
            "exit" => std::process::exit(7),
            "stdout" => loop {
                if std::io::stdout().write_all(&[42; 8192]).is_err() {
                    break;
                }
            },
            "stderr" => loop {
                if std::io::stderr().write_all(&[42; 8192]).is_err() {
                    break;
                }
            },
            "hang" => loop {
                thread::park();
            },
            _ => panic!("unknown fixture"),
        }
    }
    #[test]
    fn nonzero_and_both_pipe_limits_are_reported() {
        let cancel = CancellationToken::default();
        let result = run(
            &mut fixture("exit"),
            &[],
            4096,
            true,
            Duration::from_secs(10),
            &cancel,
        );
        assert!(matches!(result, Err(JpegError::ToolExit(Some(7)))));
        for mode in ["stdout", "stderr"] {
            let result = run(
                &mut fixture(mode),
                &[],
                4096,
                true,
                Duration::from_secs(10),
                &cancel,
            );
            assert!(matches!(result, Err(JpegError::ResourceLimit(_))), "{mode}");
        }
    }
    #[test]
    fn timeout_closes_blocked_input_and_reaps_child() {
        let mut command = fixture("hang");
        let result = run(
            &mut command,
            &vec![0; 1024 * 1024],
            4096,
            true,
            Duration::from_millis(100),
            &CancellationToken::default(),
        );
        assert!(matches!(result, Err(JpegError::Timeout)));
        assert!(!command.get_current_dir().unwrap().exists());
    }
    #[test]
    fn cancelled_before_spawn_and_during_blocked_input() {
        let cancel = CancellationToken::default();
        cancel.cancel();
        assert!(matches!(
            run(
                &mut fixture("hang"),
                &[],
                1024,
                true,
                Duration::from_secs(10),
                &cancel
            ),
            Err(JpegError::Cancelled)
        ));
        // 子进程明确写出ready后才取消，证明覆盖运行中的阻塞stdin而不是只测预取消。
        let cancel = CancellationToken::default();
        let directory = tempfile::tempdir().unwrap();
        let ready = directory.path().join("ready");
        thread::scope(|scope| {
            let token = &cancel;
            let ready_path = &ready;
            let handle = scope.spawn(move || {
                let mut command = fixture("hang");
                command.env("PIXOFOLD_PROCESS_READY", ready_path);
                let result = run(
                    &mut command,
                    &vec![0; 1024 * 1024],
                    4096,
                    false,
                    Duration::from_secs(10),
                    token,
                );
                (result, command.get_current_dir().unwrap().to_owned())
            });
            let deadline = Instant::now();
            while !ready.exists() && deadline.elapsed() < Duration::from_secs(10) {
                thread::yield_now();
            }
            cancel.cancel();
            let (result, workspace) = handle.join().unwrap();
            assert!(ready.exists(), "测试子进程必须实际启动");
            assert!(matches!(result, Err(JpegError::Cancelled)));
            assert!(!workspace.exists());
        });
    }

    #[test]
    fn missing_process_cleans_its_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let mut command = Command::new(directory.path().join("absent-helper"));
        assert!(matches!(
            run(
                &mut command,
                &[],
                10,
                false,
                Duration::from_secs(1),
                &CancellationToken::default()
            ),
            Err(JpegError::ToolIo { .. })
        ));
        assert!(!command.get_current_dir().unwrap().exists());
    }
}
