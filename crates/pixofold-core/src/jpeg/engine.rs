//! 固定工具身份和字节协议。目录/哈希必须来自应用受信构建配置，绝不是用户IPC参数。

use super::{JpegError, JpegLimits, format::Parsed, io_error, process};
use crate::{model::CancellationToken, output};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

const BINARY: &str = if cfg!(windows) {
    "pixofold-jpeg-helper.exe"
} else {
    "pixofold-jpeg-helper"
};
const MAX_TOOL_BYTES: u64 = 64 * 1024 * 1024;

/// 受信的固定单线程字节工具；可在有界图片worker间共享。不从PATH寻找或自动下载。
/// J1a由核心宿主显式提供；桌面尚未构建/分发此引擎。
pub struct JpegEngine {
    executable: PathBuf,
    sha256: [u8; 32],
    identity: same_file::Handle,
}

impl JpegEngine {
    /// 从受信的绝对目录加载固定工具名并核对构建方提供的SHA256。
    /// 不应从该目录中可被用户替换的清单自行读取期望哈希；每次调用再次复查。
    /// 此身份检查不是对可被恶意写入的安装目录提供执行文件系统CAS保证。
    ///
    /// # Errors
    /// 相对目录、缺失/链接/非普通工具、哈希不符或I/O错误均拒绝，不回退PATH。
    pub fn load(directory: &Path, sha256: [u8; 32]) -> Result<Self, JpegError> {
        if !directory.is_absolute() {
            return Err(JpegError::ToolIdentity);
        }
        let directory = directory
            .canonicalize()
            .map_err(|_| JpegError::ToolIdentity)?;
        let executable = directory.join(BINARY);
        let file = File::open(&executable).map_err(|_| JpegError::ToolIdentity)?;
        let identity = same_file::Handle::from_file(file).map_err(|_| JpegError::ToolIdentity)?;
        let engine = Self {
            executable,
            sha256,
            identity,
        };
        engine.verify()?;
        Ok(engine)
    }

    fn verify(&self) -> Result<(), JpegError> {
        let metadata =
            output::regular_metadata(&self.executable).map_err(|_| JpegError::ToolIdentity)?;
        if metadata.len() > MAX_TOOL_BYTES {
            return Err(JpegError::ToolIdentity);
        }
        let mut file = File::open(&self.executable).map_err(|_| JpegError::ToolIdentity)?;
        let identity =
            same_file::Handle::from_file(file.try_clone().map_err(|_| JpegError::ToolIdentity)?)
                .map_err(|_| JpegError::ToolIdentity)?;
        if identity != self.identity {
            return Err(JpegError::ToolIdentity);
        }
        let mut hash = Sha256::new();
        let mut block = [0; 8192];
        let mut total = 0;
        loop {
            let n = file
                .read(&mut block)
                .map_err(|e| io_error("核对工具字节", e))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > MAX_TOOL_BYTES {
                return Err(JpegError::ToolIdentity);
            }
            hash.update(&block[..n]);
        }
        if <[u8; 32]>::from(hash.finalize()) != self.sha256 {
            return Err(JpegError::ToolIdentity);
        }
        Ok(())
    }

    fn execute(
        &self,
        operation: &str,
        input: &[u8],
        limit: u64,
        keep: bool,
        limits: JpegLimits,
        cancel: &CancellationToken,
    ) -> Result<process::Capture, JpegError> {
        cancel.check()?;
        self.verify()?;
        let mut command = Command::new(&self.executable);
        command.env_clear().env("LC_ALL", "C");
        // Windows系统DLL/CRT需要系统目录，但不继承PATH、注入变量或用户JPEG配置。
        #[cfg(windows)]
        for key in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.args([
            operation,
            &limits.resources.max_dimension.to_string(),
            &limits.resources.max_pixels.to_string(),
            &limits.resources.max_decoded_bytes.0.to_string(),
            &limits.max_scans.to_string(),
        ]);
        process::run(
            &mut command,
            input,
            limit,
            keep,
            limits.process_timeout,
            cancel,
        )
    }

    pub(super) fn optimize(
        &self,
        input: &[u8],
        limits: JpegLimits,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>, JpegError> {
        Ok(self
            .execute(
                "optimize",
                input,
                limits.resources.max_input_bytes.0,
                true,
                limits,
                cancel,
            )?
            .bytes)
    }

    pub(super) fn fingerprint(
        &self,
        input: &[u8],
        parsed: &Parsed<'_>,
        limits: JpegLimits,
        cancel: &CancellationToken,
    ) -> Result<[u8; 32], JpegError> {
        let result = self.execute(
            "coefficients",
            input,
            parsed.coefficient_bytes,
            false,
            limits,
            cancel,
        )?;
        if result.length != parsed.coefficient_bytes || result.prefix != b"PFJC1" {
            return Err(JpegError::ValidationFailed);
        }
        Ok(result.digest)
    }
}
