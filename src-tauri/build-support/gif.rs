//! GIF构建身份：完整对应源码、配方和资源共同验证；运行时只信内嵌helper哈希。

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{error::Error, fs, path::Path};

const SOURCES: &[&str] = &[
    "native/gif/budget.h",
    "native/gif/budget.c",
    "native/gif/entry.c",
    "tools/gif-lab/engine.json",
    "tools/gif-lab/build.mjs",
    "tools/gif-lab/CMakeLists.txt",
    "tools/gif-bundle.mjs",
];
const DOCUMENTS: &[&str] = &[
    "gifsicle-source.tar.gz",
    "Gifsicle-COPYING",
    "Gifsicle-README.md",
    "PixoFold-LICENSE",
    "NOTICE.txt",
    "source/native/gif/README.md",
];

#[cfg(not(test))]
pub fn generate() -> Result<(), Box<dyn Error>> {
    let root = std::env::current_dir()?;
    let repository = root.parent().ok_or("缺少仓库根目录")?;
    println!("cargo:rerun-if-changed=resources/gif");
    for source in SOURCES {
        println!("cargo:rerun-if-changed=../{source}");
    }
    let hash = verify(
        repository,
        &root.join("resources/gif/runtime"),
        &std::env::var("TARGET")?,
        std::env::var("PROFILE")? != "debug",
    )?;
    fs::write(
        Path::new(&std::env::var_os("OUT_DIR").ok_or("缺少OUT_DIR")?).join("gif_identity.rs"),
        format!("pub(crate) const GIF_SHA256: Option<[u8; 32]> = {hash:?};\n"),
    )?;
    Ok(())
}

fn regular(path: &Path, limit: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("GIF构建输入不是普通文件或超过上限".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("GIF构建输入不接受reparse point".into());
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
            return Err("正式构建缺少GIF资源".into());
        }
        return Ok(None);
    }
    let manifest: Value = serde_json::from_slice(&regular(&manifest, 64 * 1024)?)?;
    if manifest["schema"] != 1 || manifest["target"] != target {
        return Err("GIF资源版本或目标架构不匹配".into());
    }
    let windows = target.ends_with("-windows-msvc");
    let binary = if windows {
        "pixofold-gif-helper.exe"
    } else {
        "pixofold-gif-helper"
    };
    if manifest["binary"] != binary
        || manifest["runtime"] != if windows { "static-msvc" } else { "system" }
    {
        return Err("GIF工具名或运行库配置不匹配".into());
    }
    for source in SOURCES {
        let expected = hash(&regular(&repository.join(source), 2 * 1024 * 1024)?);
        let staged = format!("source/{source}");
        if manifest["sources"][*source] != expected
            || manifest["files"][&staged] != expected
            || hash(&regular(&directory.join(staged), 2 * 1024 * 1024)?) != expected
        {
            return Err("GIF源码/配方已变化或随包源码不完整，必须重新prepare".into());
        }
    }
    let engine: Value = serde_json::from_slice(&regular(
        &repository.join("tools/gif-lab/engine.json"),
        64 * 1024,
    )?)?;
    if manifest["engine"] != engine
        || engine["sha256"]
            != hash(&regular(
                &directory.join("gifsicle-source.tar.gz"),
                64 * 1024 * 1024,
            )?)
    {
        return Err("GIF上游身份或完整源码归档不匹配".into());
    }
    for file in DOCUMENTS.iter().copied().chain(std::iter::once(binary)) {
        if manifest["files"][file] != hash(&regular(&directory.join(file), 64 * 1024 * 1024)?) {
            return Err("GIF资源或许可文件字节不匹配".into());
        }
    }
    Ok(Some(
        Sha256::digest(regular(&directory.join(binary), 64 * 1024 * 1024)?).into(),
    ))
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
    fn staged_source_archive_recipe_target_and_binary_are_required() {
        let repo = tempfile::tempdir().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let engine = serde_json::json!({"sha256":hash(b"archive")});
        let target = "x86_64-unknown-linux-gnu";
        let binary = "pixofold-gif-helper";
        let mut manifest = serde_json::json!({"schema":1,"target":target,"runtime":"system","binary":binary,"engine":engine,"sources":{},"files":{}});
        for source in SOURCES {
            let bytes = if *source == "tools/gif-lab/engine.json" {
                serde_json::to_vec(&engine).unwrap()
            } else {
                b"recipe".to_vec()
            };
            let staged = format!("source/{source}");
            for path in [repo.path().join(source), directory.path().join(&staged)] {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, &bytes).unwrap();
            }
            manifest["sources"][*source] = hash(&bytes).into();
            manifest["files"][&staged] = hash(&bytes).into();
        }
        for file in DOCUMENTS.iter().copied().chain(std::iter::once(binary)) {
            let path = directory.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"archive").unwrap();
            manifest["files"][file] = hash(b"archive").into();
        }
        fs::write(
            directory.path().join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(
            verify(repo.path(), directory.path(), target, true)
                .unwrap()
                .is_some()
        );
        assert!(verify(repo.path(), directory.path(), "aarch64-apple-darwin", true).is_err());
        for file in [
            binary,
            "gifsicle-source.tar.gz",
            "source/native/gif/budget.c",
        ] {
            let path = directory.path().join(file);
            let original = fs::read(&path).unwrap();
            fs::write(&path, b"replaced").unwrap();
            assert!(
                verify(repo.path(), directory.path(), target, true).is_err(),
                "{file}"
            );
            fs::write(&path, original).unwrap();
        }
        fs::write(repo.path().join(SOURCES[0]), b"new recipe").unwrap();
        assert!(verify(repo.path(), directory.path(), target, true).is_err());
    }
}
