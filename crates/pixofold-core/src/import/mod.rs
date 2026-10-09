//! 纯Rust只读导入与输出规划：扫描结束才允许生成批次，不持有图片像素或工作线程。
//! 文件选择、目录选择及未来拖放共用scan；Tauri应在有界后台线程调用，不能堵塞UI。
//! 冻结的是清单/归属，不是文件内容锁；实际处理仍由batch/pipeline重新校验当前输入。
//!
//! ```no_run
//! use pixofold_core::{batch::{BatchConfig, BatchParameters, BatchService},
//!     import::{scan, CopyLayout, ImportOutput, ScanOptions}, model::CancellationToken};
//! use std::{path::PathBuf, time::Duration};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let found = scan(&[PathBuf::from("images")], ScanOptions::default(),
//!     &CancellationToken::default(), |_| {})?;
//! // 无候选、取消或达到全局上限时拒绝；副本目标冲突仅该行失败，不拒绝其他行。
//! // output须是已存在的目录，副本保留原文件名。
//! let output = ImportOutput::CopyTo { directory: PathBuf::from("output"), layout: CopyLayout::Flat };
//! let request = found.plan(&output, BatchParameters::default())?;
//! let mut service = BatchService::new(BatchConfig::default())?;
//! let id = service.start(request)?;
//! let result = service.wait(id, Duration::from_secs(60));
//! service.shutdown()?; // 若等待超时，显式取消并等待真实计算结束。
//! let finished = result?;
//! println!("成功 {} 项", finished.summary.succeeded);
//! # Ok(())
//! # }
//! ```

mod model;
mod planning;
mod scan;

pub use model::*;
pub use planning::copy_beside;
pub use scan::{scan, scan_with_engines};
