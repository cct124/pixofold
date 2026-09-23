//! 显式桌面测试程序：加载同一份装配/单元测试，并通过build.rs携带Windows manifest。
//! 不跳过库测试，也不为测试开放额外的生产命令或权限。

#[path = "../src/lib.rs"]
pub mod desktop;
pub use desktop::tasks;
pub(crate) use desktop::{ingress, ipc, lifecycle, subscriptions};
