//! 纯Rust只读导入与输出规划：扫描结束才允许生成批次，不持有图片像素或工作线程。
//! 文件选择、目录选择及未来拖放共用scan；Tauri应在有界后台线程调用，不能堵塞UI。
//! 冻结的是清单/归属，不是文件内容锁；实际处理仍由batch/pipeline重新校验当前输入。
//!
//! ```no_run
//! use pixofold_core::{batch::{BatchConfig, BatchParameters, BatchService},
//!     import::{scan, ImportOutput, ScanOptions}, model::CancellationToken};
//! use std::{path::PathBuf, time::Duration};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let found = scan(&[PathBuf::from("images")], ScanOptions::default(),
//!     &CancellationToken::default(), |_| {})?;
//! // 无候选、取消、达到全局上限或目标冲突时返回错误；found仍可用于展示/重新规划。
//! let request = found.plan(&ImportOutput::CopyBeside, BatchParameters::default())?;
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
pub use scan::scan;
