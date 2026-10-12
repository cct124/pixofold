//! 固定资源布局与构建期可信身份；应用和无GUI部署验收共用，不搜索PATH。
//! 加载后由应用共享同一引擎实例，贯穿能力、扫描、任务及受限预览。

use pixofold_core::gif::{GifEngine, GifError};
use std::{fs, path::Path};

include!(concat!(env!("OUT_DIR"), "/gif_identity.rs"));

/// resource_root只能来自Tauri资源解析或显式开发验收，不来自IPC/图片输入。
pub(crate) fn load(resource_root: &Path) -> Result<GifEngine, GifError> {
    load_expected(resource_root, GIF_SHA256.ok_or(GifError::ToolIdentity)?)
}

fn load_expected(resource_root: &Path, hash: [u8; 32]) -> Result<GifEngine, GifError> {
    if !resource_root.is_absolute() {
        return Err(GifError::ToolIdentity);
    }
    let gif = resource_root.join("gif");
    let directory = gif.join("runtime");
    for path in [resource_root, gif.as_path(), directory.as_path()] {
        let metadata = fs::symlink_metadata(path).map_err(|_| GifError::ToolIdentity)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(GifError::ToolIdentity);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(GifError::ToolIdentity);
            }
        }
    }
    GifEngine::load(&directory, hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn runtime_manifest_cannot_authorize_a_replaced_helper() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("gif/runtime");
        fs::create_dir_all(&directory).unwrap();
        let binary = directory.join(if cfg!(windows) {
            "pixofold-gif-helper.exe"
        } else {
            "pixofold-gif-helper"
        });
        fs::write(&binary, b"trusted bytes").unwrap();
        let hash = Sha256::digest(b"trusted bytes").into();
        assert!(load_expected(root.path(), hash).is_ok());
        fs::write(&binary, b"other bytes").unwrap();
        fs::write(directory.join("manifest.json"), serde_json::json!({"files":{binary.file_name().unwrap().to_str().unwrap():format!("{:x}", Sha256::digest(b"other bytes"))}}).to_string()).unwrap();
        assert!(matches!(
            load_expected(root.path(), hash),
            Err(GifError::ToolIdentity)
        ));
        fs::remove_file(binary).unwrap();
        assert!(matches!(
            load_expected(root.path(), hash),
            Err(GifError::ToolIdentity)
        ));
        assert!(matches!(
            load_expected(Path::new("relative"), hash),
            Err(GifError::ToolIdentity)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn linked_runtime_directory_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("gif")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("gif/runtime")).unwrap();
        assert!(matches!(
            load_expected(root.path(), [0; 32]),
            Err(GifError::ToolIdentity)
        ));
    }
}
