//! IPC 可复用的数据模型。Rust 定义通过可选的 bindings 工具生成前端类型。

use serde::Serialize;

/// 规划接入的图片格式，不承诺当前已实现压缩。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Apng,
}

/// 启动页使用的构建版本与能力信息，不包含任务状态或私人路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub planned_formats: Vec<ImageFormat>,
    pub compression_available: bool,
}
