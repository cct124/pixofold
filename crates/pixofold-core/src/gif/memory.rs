//! GIF验证缓冲纯计算；批量收紧和实际验证共用，溢出不能变成较小预算。
use super::GifMode;

const PLAYBACK_FIXED_BYTES: u64 = 1024 * 1024;
const COMPARISON_FIXED_BYTES: u64 = 2 * 1024 * 1024;

pub(super) fn playback_bytes(canvas: u64, frame: u64, input: u64) -> Option<u64> {
    canvas
        .checked_mul(8)?
        .checked_add(frame)?
        .checked_add(input.checked_mul(2)?)?
        .checked_add(PLAYBACK_FIXED_BYTES)
}

pub(super) fn comparison_bytes(
    pixels: u64,
    source: u64,
    target: u64,
    resident: u64,
) -> Option<u64> {
    pixels
        .checked_mul(24)?
        .checked_add(source.checked_add(target)?.checked_mul(4)?)?
        .checked_add(COMPARISON_FIXED_BYTES)?
        .checked_add(resident)
}

/// 使用实际执行的文件字节上限预测；Vec存活候选容量按最多两倍字节上限计入。
pub(crate) fn validation_bound(pixels: u64, file_limit: u64, mode: GifMode) -> Option<u64> {
    let lossless = playback_bytes(pixels, pixels, file_limit)?;
    match mode {
        GifMode::Lossy { quality } if quality.get() < 100 => Some(lossless.max(comparison_bytes(
            pixels,
            file_limit,
            file_limit,
            file_limit.checked_mul(2)?,
        )?)),
        _ => Some(lossless),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::QualityValue;
    #[test]
    fn lossy_bound_covers_both_candidates_and_highest_quality_uses_lossless() {
        let mode = GifMode::Lossy {
            quality: QualityValue::new(80).unwrap(),
        };
        let bound = validation_bound(1000, 2000, mode).unwrap();
        assert_eq!(bound, comparison_bytes(1000, 2000, 2000, 4000).unwrap());
        assert!(bound > validation_bound(1000, 2000, GifMode::Lossless).unwrap());
        assert_eq!(
            validation_bound(
                1000,
                2000,
                GifMode::Lossy {
                    quality: QualityValue::new(100).unwrap()
                }
            ),
            validation_bound(1000, 2000, GifMode::Lossless)
        );
        assert!(comparison_bytes(u64::MAX, 1, 1, 1).is_none());
        assert!(validation_bound(1, u64::MAX, mode).is_none());
    }
}
