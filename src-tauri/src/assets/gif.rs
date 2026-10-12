//! GIF仅输出首幅完整透明画布的静态PNG；复用独立Rust解码和应用单解码/缓存所有者。
use super::{AssetError, ThumbnailDto, png};
use pixofold_core::{
    gif::{self, GifError, GifLimits, GifValidationCode},
    model::{ByteCount, CancellationToken},
};
use std::time::Duration;

pub(super) fn decode(input: &[u8], cancel: &CancellationToken) -> Result<ThumbnailDto, AssetError> {
    let mut limits = GifLimits::default();
    limits.resources.max_input_bytes = ByteCount(png::MAX_INPUT_BYTES);
    limits.resources.max_dimension = png::MAX_EDGE;
    limits.resources.max_decoded_bytes = ByteCount(png::DECODE_BYTES as u64);
    limits.validation_timeout = Duration::from_secs(5);
    let decoded = gif::decode_preview(input, limits, cancel).map_err(|error| match error {
        GifError::ResourceLimit(_) => AssetError::ResourceLimit,
        GifError::Cancelled => AssetError::StaleTask,
        GifError::Validation(error)
            if matches!(
                error.code,
                GifValidationCode::UnsupportedMetadata
                    | GifValidationCode::UnsupportedInteraction
                    | GifValidationCode::UnsupportedTiming
            ) =>
        {
            AssetError::Unavailable
        }
        _ => AssetError::DecodeFailed,
    })?;
    let image = png::from_pixels(
        &decoded.pixels,
        u32::from(decoded.width),
        u32::from(decoded.height),
        4,
        1,
    )?;
    if cancel.is_cancelled() {
        return Err(AssetError::StaleTask);
    }
    Ok(image)
}
