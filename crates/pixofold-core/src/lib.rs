//! PixoFold 核心入口，不依赖 Tauri、窗口或前端状态。
//! 静态 PNG 单文件处理独立于 IPC；桌面工作台经任务服务接入。

pub mod batch;
mod codecs;
pub mod import;
pub mod model;
mod output;
pub mod pipeline;
pub mod probe;
pub mod quality;

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
        compression_available: true,
    }
}
