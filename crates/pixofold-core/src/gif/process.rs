//! GIF进程、三个管道线程及临时目录的唯一所有者；先回收再返回，不保留stderr原文。
use super::{GifError, io_error};
use crate::model::CancellationToken;
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn capture(mut stream: impl Read, limit: u64, keep: bool) -> Result<Vec<u8>, GifError> {
    let mut bytes = Vec::new();
    let mut count = 0;
    let mut block = [0; 8192];
    loop {
        let read = stream
            .read(&mut block)
            .map_err(|error| io_error("读取管道", error))?;
        if read == 0 {
            break;
        }
        count += read as u64;
        if count > limit {
            return Err(GifError::ResourceLimit("GIF工具管道"));
        }
        if keep {
            bytes.extend_from_slice(&block[..read]);
        }
    }
    Ok(bytes)
}
pub(super) fn run(
    command: &mut Command,
    input: &[u8],
    limit: u64,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, GifError> {
    cancel.check()?;
    let workspace = tempfile::Builder::new()
        .prefix("pixofold-gif-")
        .tempdir()
        .map_err(|error| io_error("创建引擎目录", error))?;
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
        command.creation_flags(0x08000000);
    }
    let result = run_inner(command, input, limit, timeout, cancel);
    let temporary = workspace.path().to_owned();
    match workspace.close() {
        Ok(()) => result,
        Err(source) => Err(GifError::Cleanup {
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
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, GifError> {
    let started = Instant::now();
    thread::scope(|scope| {
        // child后于scope创建，线程创建失败也先kill/wait，再等待scope回收其余管道。
        let mut child = OwnedChild(
            command
                .spawn()
                .map_err(|error| io_error("启动进程", error))?,
        );
        #[cfg(test)]
        observe(Event::Spawned(
            child.0.id(),
            command.get_current_dir().unwrap().to_owned(),
        ));
        let mut stdin = child.0.stdin.take().ok_or(GifError::ValidationFailed)?;
        let stdout = child.0.stdout.take().ok_or(GifError::ValidationFailed)?;
        let stderr = child.0.stderr.take().ok_or(GifError::ValidationFailed)?;
        let (sender, receiver) = mpsc::channel();
        let writer = thread::Builder::new()
            .name("gif-stdin".into())
            .spawn_scoped(scope, move || {
                let result = stdin
                    .write_all(input)
                    .map_err(|error| io_error("写入管道", error));
                drop(stdin);
                // BrokenPipe可先于退出码可见，等待真实wait，保留原生超限86分类。
                result
            })
            .map_err(|error| io_error("创建输入线程", error))?;
        let send = sender.clone();
        let reader = thread::Builder::new()
            .name("gif-stdout".into())
            .spawn_scoped(scope, move || {
                let result = capture(stdout, limit, true);
                if result.is_err() {
                    let _ = send.send(());
                }
                result
            })
            .map_err(|error| io_error("创建输出线程", error))?;
        let diagnostics = thread::Builder::new()
            .name("gif-stderr".into())
            .spawn_scoped(scope, move || {
                let result = capture(stderr, 16 * 1024, false);
                if result.is_err() {
                    let _ = sender.send(());
                }
                result
            })
            .map_err(|error| io_error("创建诊断线程", error))?;
        let mut failure = None;
        loop {
            if cancel.is_cancelled() {
                failure = Some(GifError::Cancelled);
                break;
            }
            if started.elapsed() >= timeout {
                failure = Some(GifError::Timeout);
                break;
            }
            match child.0.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {}
                Err(error) => {
                    failure = Some(io_error("检查进程", error));
                    break;
                }
            }
            if receiver.recv_timeout(Duration::from_millis(5)).is_ok() {
                break;
            }
        }
        if !matches!(child.0.try_wait(), Ok(Some(_))) {
            let _ = child.0.kill();
        }
        let status = child.0.wait().map_err(|error| io_error("回收进程", error));
        #[cfg(test)]
        observe(Event::Waited(child.0.id(), status.is_ok()));
        let writer = writer.join().map_err(|_| GifError::ValidationFailed);
        let reader = reader.join().map_err(|_| GifError::ValidationFailed);
        let diagnostics = diagnostics.join().map_err(|_| GifError::ValidationFailed);
        let status = status?;
        if let Some(error) = failure {
            return Err(error);
        }
        if status.code() == Some(86) {
            return Err(GifError::ResourceLimit("GIF原生分配"));
        }
        let bytes = reader??;
        diagnostics??;
        if !status.success() {
            return Err(GifError::ToolExit(status.code()));
        }
        writer??;
        cancel.check()?;
        Ok(bytes)
    })
}
#[cfg(test)]
#[derive(Debug)]
enum Event {
    Spawned(u32, std::path::PathBuf),
    Waited(u32, bool),
}
#[cfg(test)]
thread_local! { static OBSERVER: std::cell::RefCell<Option<mpsc::Sender<Event>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
fn observe(event: Event) {
    OBSERVER.with(|observer| {
        if let Some(sender) = observer.borrow().as_ref() {
            let _ = sender.send(event);
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args([
            "--exact",
            "gif::process::tests::child_fixture",
            "--nocapture",
        ]);
        command.env("PIXOFOLD_GIF_PROCESS_CASE", mode);
        command
    }
    #[test]
    fn child_fixture() {
        let Ok(mode) = std::env::var("PIXOFOLD_GIF_PROCESS_CASE") else {
            return;
        };
        match mode.as_str() {
            "block" => loop {
                thread::park();
            },
            "stdout" => {
                let _ = std::io::stdout().write_all(&vec![0; 1024 * 1024]);
            }
            "stderr" => {
                let _ = std::io::stderr().write_all(&vec![0; 1024 * 1024]);
            }
            "quota" => std::process::exit(86),
            _ => std::process::exit(7),
        }
        std::process::exit(0);
    }
    fn observed_run(
        mode: &str,
        cancel: &CancellationToken,
        timeout: Duration,
        stopper: bool,
    ) -> Result<Vec<u8>, GifError> {
        let (sender, receiver) = mpsc::channel();
        OBSERVER.with(|observer| *observer.borrow_mut() = Some(sender));
        let token = cancel.clone();
        thread::scope(|scope| {
            let watcher = scope.spawn(move || {
                let Event::Spawned(pid, directory) = receiver.recv().unwrap() else {
                    panic!("缺少真实spawn");
                };
                if stopper {
                    token.cancel();
                }
                let Event::Waited(reaped, success) = receiver.recv().unwrap() else {
                    panic!("缺少真实wait");
                };
                assert_eq!(pid, reaped);
                assert!(success);
                directory
            });
            let result = run(
                &mut fixture(mode),
                &vec![0; 1024 * 1024],
                4096,
                timeout,
                cancel,
            );
            OBSERVER.with(|observer| *observer.borrow_mut() = None);
            let directory = watcher.join().unwrap();
            assert!(!directory.exists());
            result
        })
    }
    #[test]
    fn blocked_input_cancel_and_timeout_wait_for_child_and_remove_workspace() {
        assert!(matches!(
            observed_run(
                "block",
                &CancellationToken::default(),
                Duration::from_secs(10),
                true
            ),
            Err(GifError::Cancelled)
        ));
        assert!(matches!(
            observed_run(
                "block",
                &CancellationToken::default(),
                Duration::from_millis(50),
                false
            ),
            Err(GifError::Timeout)
        ));
    }
    #[test]
    fn native_exit_and_pipe_overflow_keep_domains_after_actual_reaping() {
        assert!(matches!(
            observed_run(
                "quota",
                &CancellationToken::default(),
                Duration::from_secs(10),
                false
            ),
            Err(GifError::ResourceLimit("GIF原生分配"))
        ));
        for mode in ["stdout", "stderr"] {
            assert!(matches!(
                observed_run(
                    mode,
                    &CancellationToken::default(),
                    Duration::from_secs(10),
                    false
                ),
                Err(GifError::ResourceLimit("GIF工具管道"))
            ));
        }
        assert!(matches!(
            observed_run(
                "exit",
                &CancellationToken::default(),
                Duration::from_secs(10),
                false
            ),
            Err(GifError::ToolExit(Some(7)))
        ));
    }
}
