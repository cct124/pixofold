//! 当前用户的首次无备份覆盖确认。只保存固定标记，不接收IPC路径或任务参数。
//! 缺失、损坏或不可读均视为尚未确认；原子写入成功后才允许继续启动任务。

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const FILE_NAME: &str = "overwrite-consent-v1.json";
const CONTENT: &[u8] = b"{\"version\":1,\"overwriteWithoutBackupAcknowledged\":true}\n";

pub(crate) struct OverwriteConsent {
    directory: Option<PathBuf>,
    #[cfg(any(test, feature = "bundle-check"))]
    memory: Option<std::sync::atomic::AtomicBool>,
}

fn ordinary(path: &Path, directory: bool) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    if metadata.file_type().is_symlink()
        || reparse
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(())
}

impl OverwriteConsent {
    pub(crate) fn new(directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            #[cfg(any(test, feature = "bundle-check"))]
            memory: None,
        }
    }

    // 部署工具不经过交互式UI；其显式测试设置不能成为生产IPC绕过入口。
    #[cfg(any(test, feature = "bundle-check"))]
    pub(crate) fn memory(acknowledged: bool) -> Self {
        Self {
            directory: None,
            memory: Some(std::sync::atomic::AtomicBool::new(acknowledged)),
        }
    }

    pub(crate) fn acknowledged(&self) -> bool {
        #[cfg(any(test, feature = "bundle-check"))]
        if let Some(memory) = &self.memory {
            return memory.load(std::sync::atomic::Ordering::Acquire);
        }
        self.read().unwrap_or(false)
    }

    fn read(&self) -> io::Result<bool> {
        let directory = self.directory.as_ref().ok_or(io::ErrorKind::NotFound)?;
        ordinary(directory, true)?;
        let path = directory.join(FILE_NAME);
        ordinary(&path, false)?;
        let mut content = Vec::new();
        File::open(path)?
            .take(CONTENT.len() as u64 + 1)
            .read_to_end(&mut content)?;
        Ok(content == CONTENT)
    }

    pub(crate) fn acknowledge(&self) -> io::Result<()> {
        #[cfg(any(test, feature = "bundle-check"))]
        if let Some(memory) = &self.memory {
            memory.store(true, std::sync::atomic::Ordering::Release);
            return Ok(());
        }
        self.write_record(FILE_NAME, CONTENT)
    }

    #[cfg(all(windows, any(test, not(debug_assertions))))]
    pub(crate) fn record_logs(&self, logs: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        let units: Vec<_> = logs.as_os_str().encode_wide().collect();
        if units.is_empty() || units.len() >= 32768 || units.contains(&0) || !logs.is_absolute() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut bytes = Vec::with_capacity(12 + units.len() * 2);
        bytes.extend_from_slice(b"PFXLOG1\0");
        bytes.extend_from_slice(&(units.len() as u32).to_le_bytes());
        for unit in units {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        self.write_record("log-location-v1.bin", &bytes)
    }

    fn write_record(&self, name: &str, content: &[u8]) -> io::Result<()> {
        let directory = self.directory.as_ref().ok_or(io::ErrorKind::NotFound)?;
        // 根由桌面path resolver给出。已存在的链接/文件不能成为写入目标。
        if !directory.try_exists()? {
            fs::create_dir_all(directory)?;
        }
        ordinary(directory, true)?;
        let identity = same_file::Handle::from_path(directory)?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(content)?;
        temporary.as_file().sync_all()?;
        ordinary(directory, true)?;
        if identity != same_file::Handle::from_path(directory)? {
            return Err(io::ErrorKind::InvalidData.into());
        }
        // 替换目录项而非打开旧文件写入，旧标记为硬链接时也不会改写外部文件。
        temporary.persist(directory.join(name))?;
        ordinary(directory, true)?;
        if identity != same_file::Handle::from_path(directory)? {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn log_location_is_independent_of_confirmation_and_preserves_windows_path_units() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let temp = tempfile::tempdir().unwrap();
        let consent = OverwriteConsent::new(Some(temp.path().to_path_buf()));
        let mut units: Vec<_> = temp.path().as_os_str().encode_wide().collect();
        units.extend("\\".encode_utf16());
        units.push(0xd800);
        units.extend("\\pixofold-logs".encode_utf16());
        let path = PathBuf::from(std::ffi::OsString::from_wide(&units));
        consent.record_logs(&path).unwrap();
        assert!(!consent.acknowledged());
        let bytes = fs::read(temp.path().join("log-location-v1.bin")).unwrap();
        assert_eq!(&bytes[..8], b"PFXLOG1\0");
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
            units.len()
        );
        let actual: Vec<_> = bytes[12..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        assert_eq!(actual, units);
        consent.acknowledge().unwrap();
        assert!(consent.acknowledged());
    }

    #[test]
    fn confirmation_survives_reopen_and_deleting_data_restores_the_prompt() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("app-data");
        let consent = OverwriteConsent::new(Some(directory.clone()));
        assert!(!consent.acknowledged());
        consent.acknowledge().unwrap();
        assert!(OverwriteConsent::new(Some(directory.clone())).acknowledged());
        fs::remove_file(directory.join(FILE_NAME)).unwrap();
        assert!(!consent.acknowledged());
    }

    #[test]
    fn corrupt_unavailable_and_invalid_storage_never_grant_confirmation() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(FILE_NAME), b"true").unwrap();
        let consent = OverwriteConsent::new(Some(root.path().to_path_buf()));
        assert!(!consent.acknowledged());
        consent.acknowledge().unwrap();
        assert!(consent.acknowledged());
        let unavailable = OverwriteConsent::new(None);
        assert!(!unavailable.acknowledged());
        assert!(unavailable.acknowledge().is_err());
        let bad = OverwriteConsent::new(Some(root.path().join(FILE_NAME)));
        assert!(!bad.acknowledged());
        assert!(bad.acknowledge().is_err());
    }

    #[test]
    fn acknowledging_replaces_a_hard_link_without_changing_its_other_name() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original");
        fs::write(&original, b"untouched").unwrap();
        fs::hard_link(&original, root.path().join(FILE_NAME)).unwrap();
        let consent = OverwriteConsent::new(Some(root.path().to_path_buf()));
        consent.acknowledge().unwrap();
        assert!(consent.acknowledged());
        assert_eq!(fs::read(original).unwrap(), b"untouched");
    }
}
