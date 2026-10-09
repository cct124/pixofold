//! 用应用共享的可信JPEG引擎生成识别预览；解码、管道与输出各自有界。
//! 不转ICC/CMYK，不暴露原始像素或元数据；取消/失效交给同一进程所有者回收。

use super::{AssetError, ThumbnailDto, png};
use pixofold_core::{
    batch::ImageEngines,
    jpeg::{self, JpegError, JpegLimits},
    model::{ByteCount, CancellationToken, ResourceLimits},
};
use std::time::Duration;

pub(super) fn decode(
    input: &[u8],
    engines: &ImageEngines,
    cancel: &CancellationToken,
) -> Result<ThumbnailDto, AssetError> {
    let engine = engines.jpeg().ok_or(AssetError::Unavailable)?;
    let limits = JpegLimits {
        resources: ResourceLimits {
            max_input_bytes: ByteCount(png::MAX_INPUT_BYTES),
            max_pixels: png::MAX_PIXELS,
            max_dimension: png::MAX_EDGE,
            max_decoded_bytes: ByteCount(png::DECODE_BYTES as u64),
        },
        process_timeout: Duration::from_secs(5),
        ..JpegLimits::default()
    };
    let decoded =
        jpeg::decode_preview(input, engine, limits, cancel).map_err(|error| match error {
            JpegError::ResourceLimit(_) => AssetError::ResourceLimit,
            JpegError::Cancelled => AssetError::StaleTask,
            JpegError::UnsupportedJpeg
            | JpegError::ProtectedMetadata(_)
            | JpegError::ToolIdentity => AssetError::Unavailable,
            _ => AssetError::DecodeFailed,
        })?;
    png::from_pixels(
        &decoded.pixels,
        decoded.width,
        decoded.height,
        usize::from(decoded.channels),
        decoded.orientation,
    )
}
