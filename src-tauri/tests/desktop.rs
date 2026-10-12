//! 显式桌面测试程序：加载同一份装配/单元测试，并通过build.rs携带Windows manifest。
//! 不跳过库测试，也不为测试开放额外的生产命令或权限。

#[path = "../src/lib.rs"]
pub mod desktop;
pub use desktop::tasks;
pub(crate) use desktop::{
    assets, diagnostics, gif_bundle, ingress, ipc, jpeg_bundle, lifecycle, subscriptions,
};
#[cfg(feature = "bundle-check")]
pub(crate) use desktop::{engine_bundle, resources};

#[path = "../build-support/jpeg.rs"]
mod jpeg_build;

#[path = "../build-support/gif.rs"]
mod gif_build;
