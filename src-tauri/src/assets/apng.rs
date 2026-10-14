//! APNG只显示首幅动画透明画布；复用应用单解码许可，不解码或显示独立默认图。
use super::{AssetError, ThumbnailDto, png};
use pixofold_core::{
    apng::{self, ApngError, ApngValidationLimits},
    model::CancellationToken,
};

pub(super) fn decode(input: &[u8], cancel: &CancellationToken) -> Result<ThumbnailDto, AssetError> {
    let limits = ApngValidationLimits {
        max_input_bytes: png::MAX_INPUT_BYTES as usize,
        max_canvas_pixels: 4 << 20,
        max_frame_pixels: 4 << 20,
        max_dimension: png::MAX_EDGE,
        max_validation_bytes: png::DECODE_BYTES as u64,
        deadline_ms: 5000,
        ..Default::default()
    };
    let decoded = apng::decode_preview(input, &limits, cancel).map_err(|error| match error {
        ApngError::ResourceLimit(_) => AssetError::ResourceLimit,
        ApngError::Cancelled => AssetError::StaleTask,
        ApngError::Validation(e)
            if matches!(e.0, "unsupported_metadata" | "unsupported_representation") =>
        {
            AssetError::Unavailable
        }
        _ => AssetError::DecodeFailed,
    })?;
    let image = png::from_pixels(&decoded.pixels, decoded.width, decoded.height, 4, 1)?;
    if cancel.is_cancelled() {
        return Err(AssetError::StaleTask);
    }
    Ok(image)
}
