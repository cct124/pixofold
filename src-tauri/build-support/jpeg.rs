//! 构建期读取受控暂存产物，校验目标/来源/字节后把预期哈希编入Rust。
//! 普通debug检查无需原生工具；release缺少工具必失败。运行时不读取清单。

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{error::Error, fs, path::Path};

const SOURCES: &[&str] = &[
    "native/jpeg/helper.c",
    "tools/jpeg-lab/build.mjs",
    "tools/jpeg-lab/CMakeLists.txt",
    "tools/jpeg-lab/engine.json",
    "tools/jpeg-bundle.mjs",
];

#[cfg(not(test))]
pub fn generate() -> Result<(), Box<dyn Error>> {
    let root = std::env::current_dir()?;
    let repository = root.parent().ok_or("缺少仓库根目录")?;
    let directory = root.join("resources/jpeg/runtime");
    println!("cargo:rerun-if-changed=resources/jpeg");
    for source in SOURCES {
        println!("cargo:rerun-if-changed=../{source}");
    }
    let target = std::env::var("TARGET")?;
    let required = std::env::var("PROFILE")? != "debug";
    let hash = verify(repository, &directory, &target, required)?;
    let generated = format!("pub(crate) const JPEG_SHA256: Option<[u8; 32]> = {hash:?};\n");
    fs::write(
        Path::new(&std::env::var_os("OUT_DIR").ok_or("缺少OUT_DIR")?).join("jpeg_identity.rs"),
        generated,
    )?;
    Ok(())
}

fn regular(path: &Path, limit: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("构建输入不是普通文件或超过上限".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("构建输入不接受reparse point".into());
        }
    }
    Ok(fs::read(path)?)
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verify(
    repository: &Path,
    directory: &Path,
    target: &str,
    required: bool,
) -> Result<Option<[u8; 32]>, Box<dyn Error>> {
    let manifest = directory.join("manifest.json");
    if !manifest.try_exists()? {
        if required {
            return Err("正式构建缺少JPEG资源".into());
        }
        return Ok(None);
    }
    let manifest: Value = serde_json::from_slice(&regular(&manifest, 64 * 1024)?)?;
    if manifest["schema"] != 1 || manifest["target"] != target {
        return Err("JPEG资源版本或目标架构不匹配".into());
    }
    let windows = target.ends_with("-windows-msvc");
    let binary = if windows {
        "pixofold-jpeg-helper.exe"
    } else {
        "pixofold-jpeg-helper"
    };
    if manifest["binary"] != binary
        || manifest["runtime"] != if windows { "static-msvc" } else { "system" }
    {
        return Err("JPEG工具名或运行库配置不匹配".into());
    }
    for source in SOURCES {
        if manifest["sources"][*source]
            != hash(&regular(&repository.join(source), 2 * 1024 * 1024)?)
        {
            return Err("JPEG源码/配方已变化，必须重新prepare".into());
        }
    }
    let engine: Value = serde_json::from_slice(&regular(
        &repository.join("tools/jpeg-lab/engine.json"),
        64 * 1024,
    )?)?;
    if manifest["engine"] != engine {
        return Err("JPEG上游身份不匹配".into());
    }
    let mut binary_hash = None;
    for file in [
        binary,
        "MozJPEG-LICENSE.md",
        "README.ijg",
        "PixoFold-LICENSE",
        "NOTICE.txt",
    ] {
        let digest = Sha256::digest(regular(&directory.join(file), 64 * 1024 * 1024)?);
        if manifest["files"][file] != format!("{digest:x}") {
            return Err("JPEG工具或许可文件字节不匹配".into());
        }
        if file == binary {
            binary_hash = Some(digest.into());
        }
    }
    Ok(binary_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_resources_are_optional_only_for_debug() {
        let directory = tempfile::tempdir().unwrap();
        assert!(
            verify(directory.path(), directory.path(), "test", false)
                .unwrap()
                .is_none()
        );
        assert!(verify(directory.path(), directory.path(), "test", true).is_err());
    }

    #[test]
    fn staged_bytes_target_and_sources_must_all_match() {
        let directory = tempfile::tempdir().unwrap();
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let target = "x86_64-unknown-linux-gnu";
        let files = [
            "pixofold-jpeg-helper",
            "MozJPEG-LICENSE.md",
            "README.ijg",
            "PixoFold-LICENSE",
            "NOTICE.txt",
        ];
        let mut manifest = serde_json::json!({"schema":1,"target":target,"runtime":"system","binary":files[0],"sources":{},"files":{}});
        for source in SOURCES {
            manifest["sources"][*source] = hash(&fs::read(repository.join(source)).unwrap()).into();
        }
        manifest["engine"] = serde_json::from_slice(
            &fs::read(repository.join("tools/jpeg-lab/engine.json")).unwrap(),
        )
        .unwrap();
        for file in files {
            fs::write(directory.path().join(file), b"test identity").unwrap();
            manifest["files"][file] = hash(b"test identity").into();
        }
        fs::write(
            directory.path().join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(
            verify(repository, directory.path(), target, true)
                .unwrap()
                .is_some()
        );
        assert!(verify(repository, directory.path(), "aarch64-apple-darwin", true).is_err());
        fs::write(directory.path().join(files[0]), b"replaced helper").unwrap();
        assert!(verify(repository, directory.path(), target, true).is_err());
        fs::write(directory.path().join(files[0]), b"test identity").unwrap();
        manifest["sources"][SOURCES[0]] = "stale".into();
        fs::write(
            directory.path().join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(verify(repository, directory.path(), target, true).is_err());
    }
}
