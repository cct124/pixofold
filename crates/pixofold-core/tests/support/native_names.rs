//! 文件系统可创建的原始OS名称夹具；路径类型能表示非法编码不代表磁盘接受它。

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

/// 先尝试真实非Unicode名称；仅macOS的EILSEQ拒绝改用可创建的组合字符名称继续I/O验收。
/// 其他错误必须失败，拒绝创建也必须没有留下文件；纯名称契约另由核心单元测试覆盖。
pub fn write_native_name(directory: &Path, extension: &str, bytes: &[u8]) -> PathBuf {
    #[cfg(windows)]
    let mut name = {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0x0078, 0xd800])
    };
    #[cfg(unix)]
    let mut name = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(b"x\xff".to_vec())
    };
    name.push(extension);
    assert!(name.to_str().is_none());
    let source = directory.join(&name);
    match fs::write(&source, bytes) {
        Ok(()) => source,
        #[cfg(target_os = "macos")]
        Err(error) if error.raw_os_error() == Some(MACOS_EILSEQ) => {
            assert_eq!(fs::read_dir(directory).unwrap().count(), 0);
            eprintln!("当前文件系统拒绝非UTF-8名称（EILSEQ）；继续验证组合字符原名的真实I/O");
            let mut supported = OsString::from("原名 e\u{301} 🌄");
            supported.push(extension);
            let source = directory.join(supported);
            fs::write(&source, bytes).expect("支持名称的夹具必须可创建");
            source
        }
        Err(error) => panic!("创建原始OS名称夹具失败：{error}"),
    }
}

// Darwin errno.h中的EILSEQ；当前macOS CI在创建非法UTF-8名称时返回该错误。
#[cfg(target_os = "macos")]
const MACOS_EILSEQ: i32 = 92;
