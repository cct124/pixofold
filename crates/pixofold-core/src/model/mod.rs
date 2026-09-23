//! 领域模型与 IPC 模型。只有 IPC DTO 通过 bindings 工具生成前端类型。

mod compression;
mod quality;

pub use compression::*;
pub use quality::*;

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

/// 构建版本与能力信息，不包含任务状态或私人路径。
/// compression_available表示至少一种格式可压缩，当前仅静态PNG，不等于支持全部规划格式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub planned_formats: Vec<ImageFormat>,
    pub compression_available: bool,
}
