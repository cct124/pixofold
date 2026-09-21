//! 仅向标准输出生成 IPC 类型；文件写入与一致性检查由 tools 脚本负责。

use pixofold_core::model::{AppInfo, ImageFormat};
use ts_rs::{Config, TS};

fn main() {
    let config = Config::default();
    println!("// 由 Rust 数据模型生成；请运行 pnpm types:generate，勿手工编辑。");
    println!("export {}", ImageFormat::decl(&config));
    println!("export {}", AppInfo::decl(&config));
}
