//! 可信宿主显式注入的有界Gifsicle helper；每次执行复查身份和SHA256，不读取旁置期望清单。
use super::{GifError, GifLimits, io_error, process};
use crate::{model::CancellationToken, output};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};
const NAME: &str = if cfg!(windows) {
    "pixofold-gif-helper.exe"
} else {
    "pixofold-gif-helper"
};
/// 固定单线程GIF无损引擎，可由有界worker共享，最后一个所有者释放身份句柄。
pub struct GifEngine {
    executable: PathBuf,
    sha256: [u8; 32],
    identity: same_file::Handle,
}
impl GifEngine {
    /// 加载受信绝对目录的固定工具名；期望SHA256来自构建配置，绝不能来自任意IPC/可替换清单。
    /// # Errors
    /// 非绝对目录、工具缺失/链接/替换/哈希不符或I/O失败均拒绝。
    pub fn load(directory: &Path, sha256: [u8; 32]) -> Result<Self, GifError> {
        if !directory.is_absolute() {
            return Err(GifError::ToolIdentity);
        }
        let directory = directory
            .canonicalize()
            .map_err(|_| GifError::ToolIdentity)?;
        let executable = directory.join(NAME);
        output::regular_metadata(&executable).map_err(|_| GifError::ToolIdentity)?;
        let identity = same_file::Handle::from_file(
            File::open(&executable).map_err(|_| GifError::ToolIdentity)?,
        )
        .map_err(|_| GifError::ToolIdentity)?;
        let engine = Self {
            executable,
            sha256,
            identity,
        };
        engine.verify()?;
        Ok(engine)
    }
    fn verify(&self) -> Result<(), GifError> {
        let metadata =
            output::regular_metadata(&self.executable).map_err(|_| GifError::ToolIdentity)?;
        if metadata.len() > 64 * 1024 * 1024 {
            return Err(GifError::ToolIdentity);
        }
        let mut file = File::open(&self.executable).map_err(|_| GifError::ToolIdentity)?;
        let current =
            same_file::Handle::from_file(file.try_clone().map_err(|_| GifError::ToolIdentity)?)
                .map_err(|_| GifError::ToolIdentity)?;
        if current != self.identity {
            return Err(GifError::ToolIdentity);
        }
        let mut hash = Sha256::new();
        let mut block = [0; 8192];
        let mut total = 0;
        loop {
            let count = file
                .read(&mut block)
                .map_err(|error| io_error("核对工具字节", error))?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > 64 * 1024 * 1024 {
                return Err(GifError::ToolIdentity);
            }
            hash.update(&block[..count]);
        }
        if <[u8; 32]>::from(hash.finalize()) != self.sha256 {
            return Err(GifError::ToolIdentity);
        }
        Ok(())
    }
    pub(super) fn optimize(
        &self,
        bytes: &[u8],
        limits: GifLimits,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>, GifError> {
        cancel.check()?;
        self.verify()?;
        let mut command = Command::new(&self.executable);
        command.env_clear().env("LC_ALL", "C");
        #[cfg(windows)]
        for key in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .arg("--memory-bytes")
            .arg(limits.max_native_bytes.0.to_string());
        command.args([
            "--careful",
            "-O2",
            "--same-comments",
            "--same-extensions",
            "-",
        ]);
        process::run(
            &mut command,
            bytes,
            limits.resources.max_input_bytes.0,
            limits.process_timeout,
            cancel,
        )
    }
}
