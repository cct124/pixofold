//! 固定日志目录、跨进程轮转及活动文件锁。仅操作精确命名且带本应用头的JSONL。
//! 锁协调合作的PixoFold实例，不把路径复查宣称为抵御所有恶意文件系统竞争。

use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) const MAX_FILES: usize = 10;
pub(super) const MAX_BYTES: u64 = 5_000_000;

pub(super) struct Directory {
    pub path: PathBuf,
    identity: same_file::Handle,
}

fn rejected() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "unsafe_log_directory_or_file",
    )
}

fn inspect(path: &Path, directory: bool) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.is_dir() != directory {
        return Err(rejected());
    }
    if !directory && !metadata.is_file() {
        return Err(rejected());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT；拒绝junction等非普通路径，不只判断symlink。
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(rejected());
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o077 != 0
            || (!directory && metadata.nlink() != 1)
        {
            return Err(rejected());
        }
    }
    Ok(())
}

impl Directory {
    pub fn create(path: PathBuf) -> io::Result<Self> {
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        // 父目录由系统或构建环境提供；不递归创建任意路径，也不修改既有目录权限。
        match builder.create(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        inspect(&path, true)?;
        let identity = same_file::Handle::from_path(&path)?;
        Ok(Self { path, identity })
    }

    pub fn verify(&self) -> io::Result<()> {
        inspect(&self.path, true)?;
        if same_file::Handle::from_path(&self.path)? != self.identity {
            return Err(rejected());
        }
        Ok(())
    }

    fn lock(&self) -> io::Result<File> {
        self.verify()?;
        let path = self.path.join(".pixofold-log.lock");
        let file = open_file(&path, true)?;
        // 不无限等待其他实例或外部程序持锁；日志降级不能阻塞压缩。
        file.try_lock()
            .map_err(|_| io::Error::from(io::ErrorKind::WouldBlock))?;
        self.verify()?;
        Ok(file)
    }
}

fn open_file(path: &Path, create: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(create);
    checked_open(path, options)
}

fn checked_open(path: &Path, mut options: OpenOptions) -> io::Result<File> {
    if path.try_exists()? {
        inspect(path, false)?;
    }
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT：打开链接本身，再由inspect拒绝。
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path)?;
    inspect(path, false)?;
    if same_file::Handle::from_file(file.try_clone()?)? != same_file::Handle::from_path(path)? {
        return Err(rejected());
    }
    Ok(file)
}

fn owned_name(name: &str) -> bool {
    let Some(body) = name
        .strip_prefix("run-")
        .and_then(|s| s.strip_suffix(".jsonl"))
    else {
        return false;
    };
    let Some((time, random)) = body.split_once('-') else {
        return false;
    };
    !time.is_empty()
        && time.bytes().all(|c| c.is_ascii_digit())
        && random.len() == 12
        && random.bytes().all(|c| c.is_ascii_alphanumeric())
}

fn owned_header(file: &File) -> bool {
    let mut line = String::new();
    let mut reader = BufReader::new(file.take(4096));
    if reader.read_line(&mut line).is_err() {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(&line).is_ok_and(|value| {
        value["schema"] == "pixofold.log.v1" && value["event"] == "session_started"
    })
}

struct ActiveFile {
    file: File,
    path: PathBuf,
    size: u64,
    // Windows整文件独占锁会阻止普通读取，故只锁独立的空sidecar，不锁JSONL正文。
    lease: File,
}

pub(super) struct Store {
    pub directory: Arc<Directory>,
    header: Vec<u8>,
    active: Option<ActiveFile>,
    max_files: usize,
    max_bytes: u64,
}

impl Store {
    pub fn new(directory: Arc<Directory>, header: Vec<u8>) -> Self {
        Self {
            directory,
            header,
            active: None,
            max_files: MAX_FILES,
            max_bytes: MAX_BYTES,
        }
    }

    pub fn header(&self) -> &[u8] {
        &self.header
    }

    pub fn append(&mut self, line: &[u8]) -> io::Result<()> {
        if line.len() as u64 + self.header.len() as u64 > self.max_bytes {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        self.directory.verify()?;
        if self
            .active
            .as_ref()
            .is_none_or(|active| active.size + line.len() as u64 > self.max_bytes)
        {
            self.rotate()?;
        }
        let ActiveFile {
            file,
            path,
            size,
            lease,
        } = self.active.as_mut().ok_or_else(rejected)?;
        // 目录/文件被清理或替换后不悄悄继续写孤立句柄，向界面报告不可用。
        inspect(path, false)?;
        let lease_path = path.with_extension("lock");
        inspect(&lease_path, false)?;
        if same_file::Handle::from_file(lease.try_clone()?)?
            != same_file::Handle::from_path(&lease_path)?
        {
            return Err(rejected());
        }
        if file.metadata()?.len() != *size {
            return Err(rejected());
        }
        if same_file::Handle::from_file(file.try_clone()?)? != same_file::Handle::from_path(path)? {
            return Err(rejected());
        }
        if let Err(error) = file.write_all(line) {
            // 短写不能让下一事件接到残缺JSON后面；恢复最后完整行，仍向调用方报告失败。
            let _ = file.set_len(*size);
            self.active.take();
            return Err(error);
        }
        *size += line.len() as u64;
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.close()?;
        let _lock = self.directory.lock()?;
        let mut files = Vec::new();
        for (index, entry) in fs::read_dir(&self.directory.path)?.enumerate() {
            let entry = entry?;
            if index >= 1024 {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            if entry.file_name().to_str().is_some_and(owned_name) {
                files.push(entry.path());
            }
        }
        files.sort(); // Unix毫秒启动/轮转时间，随机部分仅打破同毫秒平局。
        let mut count = files.len();
        for path in files {
            if count < self.max_files {
                break;
            }
            let lease_path = path.with_extension("lock");
            let Ok(lease) = open_file(&lease_path, true) else {
                continue;
            };
            if lease.metadata()?.len() != 0 || lease.try_lock().is_err() {
                continue;
            }
            let Ok(file) = open_file(&path, false) else {
                continue;
            };
            if file.try_lock().is_err() || !owned_header(&file) {
                continue;
            }
            self.directory.verify()?;
            if same_file::Handle::from_file(file.try_clone()?)?
                != same_file::Handle::from_path(&path)?
            {
                return Err(rejected());
            }
            // 持有活动锁直到删除完成；其他实例同样在目录协调锁内创建/清理。
            fs::remove_file(&path)?;
            drop(file);
            fs::remove_file(&lease_path)?;
            drop(lease);
            count -= 1;
        }
        if count >= self.max_files {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        self.directory.verify()?;
        let mut temp = tempfile::Builder::new()
            .prefix(&format!("run-{}-", super::unix_millis()))
            .rand_bytes(12)
            .suffix(".jsonl")
            .tempfile_in(&self.directory.path)?;
        // 头成功写入后才保留文件；短写失败由NamedTempFile清理本次新建文件。
        temp.write_all(&self.header)?;
        let lease_path = temp.path().with_extension("lock");
        let mut options = OpenOptions::new();
        options.create_new(true);
        let lease = checked_open(&lease_path, options)?;
        lease
            .try_lock()
            .map_err(|_| io::Error::from(io::ErrorKind::WouldBlock))?;
        let (file, path) = match temp.keep() {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_file(&lease_path);
                return Err(error.error);
            }
        };
        self.active = Some(ActiveFile {
            file,
            path,
            size: self.header.len() as u64,
            lease,
        });
        Ok(())
    }

    pub fn close(&mut self) -> io::Result<()> {
        if let Some(mut active) = self.active.take() {
            active.file.flush()?;
            active.file.sync_data()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
