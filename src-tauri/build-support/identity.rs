//! 仅受控CI且工作树干净时声明提交来源；普通本地构建明确unknown，避免陈旧SHA。
#[cfg(not(test))]
use std::{path::Path, process::Command};

#[cfg(not(test))]
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
fn verified(sha: Option<String>, head: Option<String>, status: Option<String>) -> Option<String> {
    sha.filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .filter(|s| head.as_ref() == Some(s))
        .filter(|_| status.as_deref() == Some(""))
        .map(|s| format!("git:{s}"))
}
#[cfg(not(test))]
pub fn generate() {
    println!("cargo:rerun-if-env-changed=GITHUB_ACTIONS");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    let root = Path::new("..");
    let identity = if std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
        // 监视源目录中的新增文件及现有跟踪文件；忽略目录没有构建来源保证。
        for dir in [
            "src",
            "src-tauri",
            "crates",
            "native",
            "tools",
            "tests",
            "docs",
            ".github",
            ".git",
        ] {
            println!("cargo:rerun-if-changed=../{dir}");
        }
        if let Some(files) = git(root, &["ls-files"]) {
            for file in files.lines() {
                println!("cargo:rerun-if-changed=../{file}");
            }
        }
        verified(
            std::env::var("GITHUB_SHA").ok(),
            git(root, &["rev-parse", "HEAD"]),
            git(root, &["status", "--porcelain", "--untracked-files=all"]),
        )
    } else {
        None
    };
    println!(
        "cargo:rustc-env=PIXOFOLD_BUILD_SOURCE={}",
        identity.as_deref().unwrap_or("unknown")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dirty_missing_or_mismatched_git_never_claims_a_commit_build() {
        let sha = "0123456789abcdef0123456789abcdef01234567".to_owned();
        assert_eq!(
            verified(Some(sha.clone()), Some(sha.clone()), Some(String::new())),
            Some(format!("git:{sha}"))
        );
        for status in [None, Some(" M src/a.rs".into()), Some("?? new.rs".into())] {
            assert!(verified(Some(sha.clone()), Some(sha.clone()), status).is_none());
        }
        assert!(verified(None, Some(sha.clone()), Some(String::new())).is_none());
        assert!(verified(Some(sha.clone()), Some("f".repeat(40)), Some(String::new())).is_none());
        assert!(
            verified(
                Some("PRIVATE/path".into()),
                Some("PRIVATE/path".into()),
                Some(String::new())
            )
            .is_none()
        );
    }
}
