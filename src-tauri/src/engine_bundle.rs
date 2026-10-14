//! 启动时分别验证随包引擎；一种引擎不可用不撤销另一种及PNG能力。
use crate::{gif_bundle, jpeg_bundle};
use pixofold_core::batch::ImageEngines;
use std::path::Path;

pub(crate) fn load(root: Option<&Path>) -> ImageEngines {
    let jpeg = root.ok_or("resource_directory").and_then(|root| {
        jpeg_bundle::load(root).map_err(|error| match error {
            pixofold_core::jpeg::JpegError::ToolIo { .. } => "tool_io",
            _ => "tool_identity",
        })
    });
    let engines = match jpeg {
        Ok(engine) => {
            tracing::info!(target: "pixofold", event = "jpeg_engine_verified");
            ImageEngines::with_jpeg(engine)
        }
        Err(code) => {
            tracing::warn!(target: "pixofold", event = "jpeg_engine_unavailable", error_code = code);
            ImageEngines::default()
        }
    }.add_apng();
    let gif = root.ok_or("resource_directory").and_then(|root| {
        gif_bundle::load(root).map_err(|error| match error {
            pixofold_core::gif::GifError::ToolIo { .. } => "tool_io",
            _ => "tool_identity",
        })
    });
    match gif {
        Ok(engine) => {
            tracing::info!(target: "pixofold", event = "gif_engine_verified");
            engines.add_gif(engine)
        }
        Err(code) => {
            tracing::warn!(target: "pixofold", event = "gif_engine_unavailable", error_code = code);
            engines
        }
    }
}
