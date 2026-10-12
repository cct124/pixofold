//! 静态首帧识别预览：复用严格结构预检和可取消LZW，仅合成首帧透明画布。
//! 不执行编码器，不解码后续帧；完整播放等价验证仍属于压缩pipeline。
use super::{GifError, GifLimits, Work, playback, structure};
use crate::model::CancellationToken;
use std::{io::Cursor, num::NonZeroU64, time::Instant};
use structure::{Code, LabError, check};

/// 紧密排列的8位RGBA首幅完整画布；未绘制区域透明，尺寸单位为像素。
#[derive(Debug)]
pub struct GifPreview {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

/// 在GIF支持子集内生成首幅完整画布，不修改输入或调用外部工具。
/// 使用validation_timeout与max_decoded_bytes约束本次预览，像素循环分片检查取消。
/// # Errors
/// 损坏/不支持/超限、取消或超时返回格式专属错误；成功不表示后续帧像素有效。
pub fn decode_preview(
    bytes: &[u8],
    limits: GifLimits,
    cancel: &CancellationToken,
) -> Result<GifPreview, GifError> {
    limits.validate()?;
    decode(
        bytes,
        limits,
        &Work {
            cancel,
            deadline: Instant::now() + limits.validation_timeout,
        },
    )
}

fn decode(bytes: &[u8], limits: GifLimits, work: &Work<'_>) -> Result<GifPreview, GifError> {
    work.check()?;
    let parsed = structure::parse(bytes, limits.validation_limits(), work)?;
    check(
        u32::from(parsed.width.max(parsed.height)) <= limits.resources.max_dimension,
        Code::ResourceLimit,
        "GIF预览单边尺寸",
    )?;
    let expected = parsed.frames.first().ok_or(LabError {
        code: Code::Decode,
        reason: "缺少首帧",
    })?;
    let canvas_pixels = u64::from(parsed.width) * u64::from(parsed.height);
    let frame_pixels = u64::from(expected.width) * u64::from(expected.height);
    // 单RGBA画布、索引、gif压缩Vec扩容和1MiB字典/行表/结构余量，分配前检查。
    check(
        canvas_pixels * 4 + frame_pixels + bytes.len() as u64 * 2 + 1024 * 1024
            <= limits.resources.max_decoded_bytes.0,
        Code::ResourceLimit,
        "GIF预览缓冲工作集",
    )?;
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    options.check_frame_consistency(true);
    options.check_lzw_end_code(true);
    options.skip_frame_decoding(true);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        NonZeroU64::new(limits.resources.max_decoded_bytes.0).ok_or(GifError::InvalidLimits)?,
    ));
    let decode_error = |_| {
        GifError::Validation(LabError {
            code: Code::Decode,
            reason: "GIF首帧解码失败",
        })
    };
    let mut decoder = options
        .read_info(Cursor::new(bytes))
        .map_err(decode_error)?;
    let global = decoder
        .global_palette()
        .map(<[u8]>::to_vec)
        .unwrap_or_default();
    let frame = decoder
        .read_next_frame()
        .map_err(decode_error)?
        .ok_or(LabError {
            code: Code::Decode,
            reason: "缺少首帧",
        })?;
    check(
        frame.width == expected.width
            && frame.height == expected.height
            && frame.left == expected.left
            && frame.top == expected.top
            && frame.transparent == expected.control.transparent,
        Code::Decode,
        "GIF首帧结构不一致",
    )?;
    let indices = playback::indices(frame, work)?;
    let palette = frame.palette.as_deref().unwrap_or(&global);
    let mut rows = Vec::with_capacity(usize::from(frame.height));
    if frame.interlaced {
        for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
            rows.extend((start..usize::from(frame.height)).step_by(step));
        }
    } else {
        rows.extend(0..usize::from(frame.height));
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(canvas_pixels as usize * 4)
        .map_err(|_| GifError::ResourceLimit("GIF预览分配"))?;
    pixels.resize(canvas_pixels as usize * 4, 0);
    for (index, color) in indices.into_iter().enumerate() {
        if index % 4096 == 0 {
            work.check()?;
        }
        let color_offset = usize::from(color) * 3;
        check(
            color_offset + 3 <= palette.len(),
            Code::Decode,
            "GIF首帧调色板索引",
        )?;
        if frame.transparent == Some(color) {
            continue;
        }
        let x = usize::from(frame.left) + index % usize::from(frame.width);
        let y = usize::from(frame.top) + rows[index / usize::from(frame.width)];
        let offset = (y * usize::from(parsed.width) + x) * 4;
        pixels[offset..offset + 3].copy_from_slice(&palette[color_offset..color_offset + 3]);
        pixels[offset + 3] = 255;
    }
    work.check()?;
    Ok(GifPreview {
        width: parsed.width,
        height: parsed.height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ByteCount;
    use sha2::{Digest, Sha256};
    use std::{fs, path::Path};

    fn fixture(name: &str) -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/gif")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn gif_first_canvas_matches_independently_authored_corpus() {
        let manifest: serde_json::Value =
            serde_json::from_slice(&fixture("manifest.json")).unwrap();
        for sample in manifest["samples"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|sample| sample["expect"] == "ok")
        {
            let name = sample["file"].as_str().unwrap();
            let bytes = fixture(name);
            let image = decode_preview(&bytes, GifLimits::default(), &CancellationToken::default())
                .unwrap();
            assert_eq!(
                image.width,
                sample["expected"]["width"].as_u64().unwrap() as u16,
                "{name}"
            );
            assert_eq!(
                image.height,
                sample["expected"]["height"].as_u64().unwrap() as u16,
                "{name}"
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(&image.pixels)),
                sample["expected"]["transparent"][0]["sha256"],
                "{name}"
            );
        }
    }

    #[test]
    fn gif_preview_rejects_bad_pixels_limits_cancel_and_expired_deadline() {
        let cancel = CancellationToken::default();
        for name in [
            "bad-lzw.gif",
            "invalid-palette-index.gif",
            "missing-lzw-end.gif",
            "unknown-application.gif",
            "outside-frame.gif",
        ] {
            assert!(
                decode_preview(&fixture(name), GifLimits::default(), &cancel).is_err(),
                "{name}"
            );
        }
        let bytes = fixture("static89.gif");
        let mut limits = GifLimits::default();
        limits.resources.max_decoded_bytes = ByteCount(1024);
        assert!(matches!(
            decode_preview(&bytes, limits, &cancel),
            Err(GifError::ResourceLimit(_))
        ));
        assert!(matches!(
            decode(
                &bytes,
                GifLimits::default(),
                &Work {
                    cancel: &cancel,
                    deadline: Instant::now()
                }
            ),
            Err(GifError::Timeout)
        ));
        cancel.cancel();
        assert!(matches!(
            decode_preview(&bytes, GifLimits::default(), &cancel),
            Err(GifError::Cancelled)
        ));
    }
}
