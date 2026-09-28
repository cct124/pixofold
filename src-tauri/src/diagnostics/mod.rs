//! 本地诊断日志所有者：tracing只接收受控字段，业务线程有界入队，唯一后台线程写盘。
//! 诊断失败不改变图片结果；丢失计数和写入状态可查询，日志不作为授权或恢复状态。

mod layer;
mod storage;
#[cfg(test)]
mod tests;

use serde::Serialize;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use storage::{Directory, Store};
use tracing_subscriber::prelude::*;

const QUEUE_SIZE: usize = 1024;
const MAX_EVENT_BYTES: usize = 16_384;
static LOGGER: OnceLock<Arc<Logger>> = OnceLock::new();
static INSTANCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub(crate) enum LogState {
    Starting,
    Ready,
    Unavailable,
    Busy,
    Stopped,
}

/// 只暴露状态，不返回原生目录、私人路径或错误原文。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogStatus {
    state: LogState,
    dropped_events: String,
    write_failures: String,
    can_open: bool,
}

struct Health {
    state: AtomicU8,
    dropped: AtomicU64,
    failures: AtomicU64,
    closing: AtomicBool,
}
impl Health {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(0),
            dropped: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            closing: AtomicBool::new(false),
        }
    }
    fn failure(&self, error: &std::io::Error) {
        self.failures.fetch_add(1, Ordering::Relaxed);
        self.state.store(
            if error.kind() == std::io::ErrorKind::WouldBlock {
                3
            } else {
                2
            },
            Ordering::Release,
        );
    }
}

pub(super) fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn directory_path() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../logs")
    }
    #[cfg(not(debug_assertions))]
    {
        std::env::temp_dir().join("pixofold-logs")
    }
}

pub(super) struct Logger {
    sender: mpsc::SyncSender<Vec<u8>>,
    health: Arc<Health>,
    directory: Option<Arc<Directory>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    finished: Mutex<mpsc::Receiver<()>>,
    session: String,
    started: Instant,
    sequence: AtomicU64,
}

impl Logger {
    fn start(path: PathBuf, console: bool) -> Arc<Self> {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_SIZE);
        let (done, finished) = mpsc::channel();
        let health = Arc::new(Health::new());
        let session = format!(
            "{}-{}-{}",
            unix_millis(),
            std::process::id(),
            INSTANCE.fetch_add(1, Ordering::Relaxed)
        );
        let directory = match Directory::create(path) {
            Ok(value) => Some(Arc::new(value)),
            Err(error) => {
                health.failure(&error);
                None
            }
        };
        let logger = Arc::new(Self {
            sender,
            health: health.clone(),
            directory: directory.clone(),
            thread: Mutex::new(None),
            finished: Mutex::new(finished),
            session,
            started: Instant::now(),
            sequence: AtomicU64::new(1),
        });
        let header = logger.encode(
            "INFO",
            json!({
                "event": "session_started", "version": env!("CARGO_PKG_VERSION"),
                "build_mode": if cfg!(debug_assertions) {"development"} else {"release"},
                "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
                "max_files": storage::MAX_FILES, "max_file_bytes": storage::MAX_BYTES,
            }),
        );
        match thread::Builder::new()
            .name("pixofold-log-writer".into())
            .spawn(move || {
                write_loop(receiver, directory, header, &health, console);
                let _ = done.send(());
            }) {
            Ok(thread) => {
                if let Ok(mut owner) = logger.thread.lock() {
                    *owner = Some(thread);
                }
            }
            Err(error) => {
                logger.health.failure(&error);
            }
        }
        logger
    }

    fn encode(&self, level: &str, fields: Value) -> Vec<u8> {
        let mut value = json!({
            "schema": "pixofold.log.v1", "sessionId": self.session,
            "timestamp": time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
            "elapsedMs": self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            "sequence": self.sequence.fetch_add(1, Ordering::Relaxed),
            "level": level, "thread": format!("{:?}", thread::current().id()),
        });
        if let (Some(target), Some(fields)) = (value.as_object_mut(), fields.as_object()) {
            target.extend(
                fields
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
        let mut bytes = serde_json::to_vec(&value).unwrap_or_default();
        if bytes.len() + 1 > MAX_EVENT_BYTES {
            // 字段过长时保留合法JSON和关联信息，而不是截断UTF-8/JSON行。
            value = json!({"schema":"pixofold.log.v1", "sessionId":self.session,
                "event":"event_truncated", "sequence":value["sequence"]});
            bytes = serde_json::to_vec(&value).unwrap_or_default();
        }
        bytes.push(10);
        bytes
    }

    fn event(&self, level: &str, fields: Value) {
        if self.health.closing.load(Ordering::Acquire) {
            return;
        }
        let bytes = self.encode(level, fields);
        if self.sender.try_send(bytes).is_err() {
            self.health.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn shutdown(&self) {
        self.health.closing.store(true, Ordering::Release);
        // 调用方已停止任务；正常排空并同步磁盘。慢盘最多等待2秒，不阻塞应用退出。
        let thread = self.thread.lock().ok().and_then(|mut owner| owner.take());
        if let Some(thread) = thread {
            let finished = self.finished.lock().ok().is_some_and(|done| {
                !matches!(
                    done.recv_timeout(Duration::from_secs(2)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                )
            });
            if !finished || thread.join().is_err() {
                self.health.failures.fetch_add(1, Ordering::Relaxed);
                self.health.state.store(2, Ordering::Release);
            }
        }
    }
}

fn write_loop(
    receiver: mpsc::Receiver<Vec<u8>>,
    directory: Option<Arc<Directory>>,
    header: Vec<u8>,
    health: &Health,
    console: bool,
) {
    // 写入器和IPC共享启动时的目录身份，绝不重新接受被替换的目录。
    let mut store = directory.map(|dir| Store::new(dir, header));
    let mut dropped_reported = 0;
    loop {
        let line = if health.closing.load(Ordering::Acquire) {
            receiver.try_recv().ok()
        } else {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => Some(line),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => None,
            }
        };
        let Some(line) = line else {
            break;
        };
        if console {
            eprint!("{}", String::from_utf8_lossy(&line));
        }
        if let Some(store) = &mut store {
            match store.append(&line) {
                Ok(()) => {
                    health.state.store(1, Ordering::Release);
                    let dropped = health.dropped.load(Ordering::Relaxed);
                    if dropped != dropped_reported {
                        let bytes = health_line(store.header(), "logging_loss_summary", health);
                        match store.append(&bytes) {
                            Ok(()) => dropped_reported = dropped,
                            Err(error) => health.failure(&error),
                        }
                    }
                }
                Err(error) => {
                    health.failure(&error);
                    health.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        } else {
            health.dropped.fetch_add(1, Ordering::Relaxed);
        }
        let dropped = health.dropped.load(Ordering::Relaxed);
        if console && dropped != dropped_reported {
            eprintln!("PixoFold 日志降级：累计丢失事件={dropped}");
        }
    }
    if let Some(store) = &mut store {
        // 原header包含session/version；末条保持同一会话，不记录路径或原始错误。
        let bytes = health_line(store.header(), "session_finished", health);
        if let Err(error) = store.append(&bytes).and_then(|()| store.close()) {
            health.failure(&error);
        } else {
            health.state.store(4, Ordering::Release);
        }
    }
}

fn health_line(header: &[u8], event: &str, health: &Health) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(header).unwrap_or_default();
    value["event"] = json!(event);
    value["timestamp"] = json!(
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()
    );
    if let Some(object) = value.as_object_mut() {
        object.remove("sequence");
        object.remove("elapsedMs");
    }
    value["dropped_events"] = json!(health.dropped.load(Ordering::Relaxed));
    value["write_failures"] = json!(health.failures.load(Ordering::Relaxed));
    let mut bytes = serde_json::to_vec(&value).unwrap_or_default();
    bytes.push(10);
    bytes
}

/// 在启动任务线程前装配；重复调用不创建第二个日志池。
pub(crate) fn initialize() {
    LOGGER.get_or_init(|| {
        let logger = Logger::start(directory_path(), cfg!(debug_assertions));
        if tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(layer::EventLayer(logger.clone())),
        )
        .is_err()
        {
            logger.health.state.store(2, Ordering::Release);
            logger.health.failures.fetch_add(1, Ordering::Relaxed);
        }
        logger.event("INFO", json!({"event":"application_starting"}));
        logger
    });
}

pub(crate) fn shutdown() {
    if let Some(logger) = LOGGER.get() {
        logger.shutdown();
    }
}

pub(crate) fn status() -> LogStatus {
    LOGGER.get().map_or(
        LogStatus {
            state: LogState::Unavailable,
            dropped_events: "0".into(),
            write_failures: "0".into(),
            can_open: false,
        },
        |logger| {
            let state = match logger.health.state.load(Ordering::Acquire) {
                0 => LogState::Starting,
                1 => LogState::Ready,
                3 => LogState::Busy,
                4 => LogState::Stopped,
                _ => LogState::Unavailable,
            };
            LogStatus {
                state,
                dropped_events: logger.health.dropped.load(Ordering::Relaxed).to_string(),
                write_failures: logger.health.failures.load(Ordering::Relaxed).to_string(),
                can_open: logger.directory.is_some(),
            }
        },
    )
}

pub(crate) fn open_directory() -> Result<(), &'static str> {
    let directory = LOGGER
        .get()
        .and_then(|logger| logger.directory.as_ref())
        .ok_or("logs_unavailable")?;
    directory.verify().map_err(|_| "logs_unavailable")?;
    // 固定后端路径、无shell、无前端参数，不向WebView暴露私人路径。
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(&directory.path);
    let (sender, receiver) = mpsc::channel();
    // 启动结果回传；独立线程回收打开器子进程，不阻塞WebView或遗留Unix僵尸进程。
    thread::Builder::new()
        .name("pixofold-log-opener".into())
        .spawn(move || match command.spawn() {
            Ok(mut child) => {
                let _ = sender.send(Ok(()));
                let result = child.wait();
                // Explorer可能把目录交给已有实例后返回非零，不能据此断言打开失败。
                if result.is_err()
                    || (!cfg!(windows) && !result.is_ok_and(|status| status.success()))
                {
                    tracing::warn!(target: "pixofold", event = "open_log_directory_failed");
                }
            }
            Err(_) => {
                let _ = sender.send(Err("open_logs_failed"));
            }
        })
        .map_err(|_| "open_logs_failed")?;
    receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or(Err("open_logs_failed"))
}

#[cfg(feature = "bindings")]
pub(crate) fn declarations() -> String {
    use ts_rs::TS;
    let config = ts_rs::Config::default();
    format!(
        "export {};\nexport {};\n",
        LogState::decl(&config),
        LogStatus::decl(&config)
    )
}
