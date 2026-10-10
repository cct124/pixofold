//! 单图预约与执行上限：读取PNG固定头或JPEG有界标记前缀，不解码、不缓存全文件、不持调度锁I/O。
//! 收紧的上限必须传入整条pipeline，排队后变大的源图不能按旧小图预算执行。

use super::{FormatOptions, ImageRequest};
use std::{fs::File, io::Read};

use crate::{
    model::{ByteCount, PngRequest, ResourceLimits},
    probe,
};

const BUFFER_MARGIN: u64 = 1024 * 1024;
// 四分量JPEG为候选熵编码预留32 bytes/pixel，另留两份最大元数据余量；仍受用户输入上限限制。
const JPEG_CANDIDATE_BYTES_PER_PIXEL: u64 = 32;

pub(super) fn execution_limits(
    request: &PngRequest,
    input_bytes: Option<ByteCount>,
) -> ResourceLimits {
    let original = request.limits;
    let Some(input_bytes) = input_bytes else {
        return original;
    };
    let mut header = [0; probe::PNG_HEADER_LEN];
    let info = File::open(&request.source)
        .and_then(|mut file| file.read_exact(&mut header))
        .ok()
        .and_then(|()| probe::inspect_header(&header, original).ok());
    let Some(info) = info else {
        // 头部损坏/读取失败不能据不可信尺寸少预约；仍用原上限并由pipeline给出原始错误。
        return original;
    };
    let pixels = u64::from(info.width) * u64::from(info.height);
    // 按RGBA16最坏缓冲计费，覆盖索引/RGB展开及可能的输出色型；不降低PNG解码精度。
    let raw_bytes = pixels.saturating_mul(8);
    ResourceLimits {
        // 候选文件可能比源文件大，不能简单把字节上限设为源长度，否则无收益会误报失败。
        max_input_bytes: ByteCount(
            original.max_input_bytes.0.min(
                input_bytes
                    .0
                    .saturating_add(raw_bytes)
                    .saturating_add(BUFFER_MARGIN),
            ),
        ),
        max_decoded_bytes: ByteCount(
            original
                .max_decoded_bytes
                .0
                .min(raw_bytes.max(BUFFER_MARGIN)),
        ),
        max_pixels: original.max_pixels.min(pixels),
        max_dimension: original.max_dimension.min(info.width.max(info.height)),
    }
}

pub(super) fn image_limits(
    request: &ImageRequest,
    input_bytes: Option<ByteCount>,
) -> ResourceLimits {
    if let FormatOptions::Gif(options) = request.options {
        let original = options.limits(request.limits).resources;
        let mut header = [0; 13];
        let pixels = File::open(&request.source)
            .and_then(|mut file| file.read_exact(&mut header))
            .ok()
            .and_then(|()| {
                if !header.starts_with(b"GIF87a") && !header.starts_with(b"GIF89a") {
                    return None;
                }
                let width = u16::from_le_bytes([header[6], header[7]]) as u64;
                let height = u16::from_le_bytes([header[8], header[9]]) as u64;
                let pixels = width * height;
                (width > 0 && height > 0 && pixels <= original.max_pixels)
                    .then_some((width, height, pixels))
            });
        return pixels.map_or(original, |(width, height, pixels)| ResourceLimits {
            max_pixels: original.max_pixels.min(pixels),
            max_dimension: original.max_dimension.min(width.max(height) as u32),
            max_decoded_bytes: ByteCount(
                original
                    .max_decoded_bytes
                    .0
                    .min(pixels * 9 + original.max_input_bytes.0 * 2 + BUFFER_MARGIN),
            ),
            ..original
        });
    }
    let FormatOptions::Jpeg(options) = request.options else {
        return request
            .png()
            .map_or(request.limits, |png| execution_limits(&png, input_bytes));
    };
    let original = request.limits;
    let Some(size) = input_bytes else {
        return original;
    };
    let info = File::open(&request.source)
        .ok()
        .and_then(|file| crate::jpeg::probe_header(file, options.limits(original)));
    let Some((info, required)) = info else {
        return original;
    };
    let pixels = u64::from(info.width) * u64::from(info.height);
    ResourceLimits {
        max_input_bytes: ByteCount(
            original.max_input_bytes.0.min(
                size.0
                    .saturating_add(pixels.saturating_mul(JPEG_CANDIDATE_BYTES_PER_PIXEL))
                    .saturating_add(2 * BUFFER_MARGIN),
            ),
        ),
        max_decoded_bytes: ByteCount(
            original
                .max_decoded_bytes
                .0
                .min(required.max(BUFFER_MARGIN)),
        ),
        max_pixels: original.max_pixels.min(pixels),
        max_dimension: original.max_dimension.min(info.width.max(info.height)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        batch::{BatchParameters, estimate_working_set},
        model::{PngMode, QualityValue},
    };

    fn sample() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/png/rgb8.png"),
        )
        .unwrap()
    }

    #[test]
    fn valid_header_tightens_allocation_limits_without_changing_the_public_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.png");
        let bytes = sample();
        std::fs::write(&path, &bytes).unwrap();
        let request = PngRequest::new(path);
        let limits = execution_limits(&request, Some(ByteCount(bytes.len() as u64)));
        let info = probe::inspect_png(&bytes, limits).unwrap();
        assert_eq!(
            limits.max_pixels,
            u64::from(info.width) * u64::from(info.height)
        );
        assert!(limits.max_input_bytes.0 > bytes.len() as u64);
        assert!(limits.max_input_bytes < request.limits.max_input_bytes);
        assert!(limits.max_decoded_bytes < request.limits.max_decoded_bytes);
        let mode = PngMode::Lossy {
            quality: QualityValue::default(),
        };
        assert!(
            estimate_working_set(BatchParameters {
                limits,
                mode,
                ..BatchParameters::default()
            })
            .unwrap()
            .0 < 64 * BUFFER_MARGIN
        );
        assert_eq!(
            request.limits.max_pixels,
            ResourceLimits::default().max_pixels
        );
    }

    #[test]
    fn corrupt_or_missing_header_never_grants_a_smaller_reservation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.png");
        let request = PngRequest::new(&path);
        let mut bytes = sample();
        bytes[20] ^= 1; // 篡改高度但不改CRC；不得使用篡改后的小尺寸。
        for input in [&bytes[..], b"not png", &bytes[..20]] {
            std::fs::write(&path, input).unwrap();
            let limits = execution_limits(&request, Some(ByteCount(input.len() as u64)));
            assert_eq!(limits.max_pixels, request.limits.max_pixels);
            assert_eq!(limits.max_input_bytes, request.limits.max_input_bytes);
        }
        assert_eq!(
            execution_limits(&request, None).max_pixels,
            request.limits.max_pixels
        );
    }

    #[test]
    fn smaller_caller_limits_are_never_expanded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.png");
        let bytes = sample();
        std::fs::write(&path, &bytes).unwrap();
        let mut request = PngRequest::new(path);
        request.limits.max_input_bytes = ByteCount(bytes.len() as u64);
        request.limits.max_decoded_bytes = ByteCount(128 * 1024);
        let limits = execution_limits(&request, Some(ByteCount(bytes.len() as u64)));
        assert_eq!(limits.max_input_bytes, request.limits.max_input_bytes);
        assert!(limits.max_decoded_bytes <= request.limits.max_decoded_bytes);
    }
}
