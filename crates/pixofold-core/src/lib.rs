//! PixoFold 核心入口，不依赖 Tauri、窗口或前端状态。
//! 当前只定义项目能力描述；扫描、编码与输出随后按模块接入。

pub mod model;

use model::{AppInfo, ImageFormat};

/// 返回当前构建的信息。规划格式不代表编码器已经可用。
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "PixoFold".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        planned_formats: vec![
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::Gif,
            ImageFormat::Apng,
        ],
        compression_available: false,
    }
}
