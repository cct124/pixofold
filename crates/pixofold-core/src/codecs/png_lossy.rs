//! PNG 调色板量化：只生成内存候选，颜色/透明边界不满足时明确请求无损回退。
//! ICC/HDR 转换及高位深降级尚未实现；绝不隐式改变这些输入的颜色解释。

use std::io::Cursor;

use imagequant::RGBA;

use crate::{
    model::{
        CancellationToken, ImageInfo, LossyFallbackReason, PngMode, PngProcessing,
        PngQualityMapping, ProcessingError, ResourceLimits,
    },
    probe::{self, DecodedPng},
    quality,
};

use super::png as lossless;

const QUANTIZATION_SPEED: i32 = 4;
const DITHERING_LEVEL: f32 = 1.0;
// 质量尺度允许颜色损失，但独立约束 alpha 误差，避免低 q 把透明图铺底。
const MAX_ALPHA_ERROR: u8 = 8;

pub(crate) struct Candidate {
    pub bytes: Vec<u8>,
    pub processing: PngProcessing,
}

enum Attempt {
    Quantized {
        bytes: Vec<u8>,
        measured_quality: u8,
    },
    Fallback(LossyFallbackReason),
}

pub(crate) fn prepare(
    data: &[u8],
    original: &DecodedPng,
    mode: PngMode,
    limits: ResourceLimits,
    cancel: &CancellationToken,
) -> Result<Candidate, ProcessingError> {
    let lossless = lossless::optimize(data, limits)?;
    cancel.check()?;
    let PngMode::Lossy { quality } = mode else {
        return Ok(Candidate {
            bytes: lossless,
            processing: PngProcessing::Lossless,
        });
    };
    let mapping = quality::png_quality(quality);
    let attempt = quantize(data, &original.info, mapping, limits, cancel)?;
    let reason = match attempt {
        Attempt::Quantized {
            bytes,
            measured_quality,
        } if bytes.len() < lossless.len() => {
            return Ok(Candidate {
                bytes,
                processing: PngProcessing::Lossy {
                    mapping,
                    measured_quality,
                },
            });
        }
        Attempt::Quantized { .. } => LossyFallbackReason::NoSizeBenefit,
        Attempt::Fallback(reason) => reason,
    };
    Ok(Candidate {
        bytes: lossless,
        processing: PngProcessing::LosslessFallback { mapping, reason },
    })
}

fn quantize(
    data: &[u8],
    info: &ImageInfo,
    mapping: PngQualityMapping,
    limits: ResourceLimits,
    cancel: &CancellationToken,
) -> Result<Attempt, ProcessingError> {
    if info.bit_depth == 16 {
        return Ok(Attempt::Fallback(LossyFallbackReason::HighBitDepth));
    }
    let gamma = match input_gamma(data)? {
        Ok(gamma) => gamma,
        Err(reason) => return Ok(Attempt::Fallback(reason)),
    };
    let pixels = rgba8(data, info, limits)?;
    cancel.check()?;
    let mut attributes = imagequant::new();
    attributes
        .set_speed(QUANTIZATION_SPEED)
        .map_err(quant_error)?;
    attributes
        .set_quality(mapping.minimum, mapping.target)
        .map_err(quant_error)?;
    let token = cancel.clone();
    attributes.set_progress_callback(move |_| {
        if token.is_cancelled() {
            imagequant::ControlFlow::Break
        } else {
            imagequant::ControlFlow::Continue
        }
    });
    let mut image = attributes
        .new_image_borrowed(&pixels, info.width as usize, info.height as usize, gamma)
        .map_err(quant_error)?;
    let mut result = attributes.quantize(&mut image).map_err(quant_error)?;
    // input_gamma 已验证 0 < gamma < 1，输出仍使用相同 transfer curve。
    result.set_output_gamma(gamma).map_err(quant_error)?;
    result
        .set_dithering_level(DITHERING_LEVEL)
        .map_err(quant_error)?;
    let token = cancel.clone();
    result.set_progress_callback(move |_| {
        if token.is_cancelled() {
            imagequant::ControlFlow::Break
        } else {
            imagequant::ControlFlow::Continue
        }
    });
    let (palette, indices) = result.remapped(&mut image).map_err(quant_error)?;
    cancel.check()?;
    let measured = result.remapping_quality();
    let Some(measured_quality) = measured.filter(|score| *score >= mapping.target) else {
        return Ok(Attempt::Fallback(LossyFallbackReason::QualityBelowTarget {
            measured,
        }));
    };
    if palette.is_empty() || palette.len() > 256 || indices.len() != pixels.len() {
        return Err(ProcessingError::ValidationFailed("量化结果长度无效"));
    }
    for (source, index) in pixels.iter().zip(&indices) {
        let output = palette
            .get(usize::from(*index))
            .ok_or(ProcessingError::ValidationFailed("量化索引越界"))?;
        if !alpha_acceptable(source.a, output.a) {
            return Ok(Attempt::Fallback(LossyFallbackReason::TransparencyGuard));
        }
    }
    drop(image);
    let encoded = encode_palette(info, &palette, &indices)?;
    let merged = merge_metadata(data, &encoded)?;
    let bytes = lossless::optimize(&merged, limits)?;
    cancel.check()?;
    // 同时优于源文件和无损候选才可能被采纳；无收益绝不能报告并不存在的索引色输出。
    if bytes.len() >= data.len() {
        return Ok(Attempt::Fallback(LossyFallbackReason::NoSizeBenefit));
    }
    let output_info = probe::inspect_png(&bytes, limits)?;
    let actual = rgba8(&bytes, &output_info, limits)?;
    if output_info.width != info.width
        || output_info.height != info.height
        || actual.len() != indices.len()
        || actual
            .iter()
            .zip(&indices)
            .any(|(pixel, index)| *pixel != palette[usize::from(*index)])
    {
        return Err(ProcessingError::ValidationFailed(
            "调色板 PNG 编码与量化像素不一致",
        ));
    }
    Ok(Attempt::Quantized {
        bytes,
        measured_quality,
    })
}

fn alpha_acceptable(source: u8, output: u8) -> bool {
    match source {
        0 => output == 0,
        255 => output == 255,
        _ => output > 0 && output < 255 && source.abs_diff(output) <= MAX_ALPHA_ERROR,
    }
}

fn input_gamma(data: &[u8]) -> Result<Result<f64, LossyFallbackReason>, ProcessingError> {
    let mut gamma = 0.45455;
    let mut has_srgb = false;
    for chunk in probe::chunks(data)? {
        match &chunk.name {
            b"iCCP" | b"cHRM" | b"cICP" | b"mDCV" | b"cLLI" => {
                return Ok(Err(LossyFallbackReason::ColorMetadata));
            }
            b"sBIT" | b"bKGD" | b"hIST" | b"sPLT" => {
                return Ok(Err(LossyFallbackReason::RepresentationMetadata));
            }
            b"sRGB" => {
                if chunk.data.len() != 1 || chunk.data[0] > 3 {
                    return Ok(Err(LossyFallbackReason::ColorMetadata));
                }
                has_srgb = true;
            }
            b"gAMA" => {
                let Ok(bytes) = <[u8; 4]>::try_from(chunk.data) else {
                    return Ok(Err(LossyFallbackReason::ColorMetadata));
                };
                gamma = f64::from(u32::from_be_bytes(bytes)) / 100_000.0;
                if gamma <= 0.0 || gamma >= 1.0 {
                    return Ok(Err(LossyFallbackReason::ColorMetadata));
                }
            }
            _ => {}
        }
    }
    if has_srgb && (gamma - 0.45455).abs() > f64::EPSILON {
        return Ok(Err(LossyFallbackReason::ColorMetadata));
    }
    Ok(Ok(gamma))
}

fn rgba8(
    data: &[u8],
    info: &ImageInfo,
    limits: ResourceLimits,
) -> Result<Vec<RGBA>, ProcessingError> {
    let rgba_bytes = u64::from(info.width)
        .checked_mul(u64::from(info.height))
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes <= limits.max_decoded_bytes.0)
        .ok_or(ProcessingError::ResourceLimit("量化 RGBA 缓冲区"))?;
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_limits(png::Limits {
        bytes: limits.max_decoded_bytes.0 as usize,
    });
    decoder.set_transformations(png::Transformations::EXPAND);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_ignore_text_chunk(true);
    let mut reader = decoder.read_info().map_err(decode_error)?;
    let size = reader
        .output_buffer_size()
        .filter(|size| *size as u64 <= limits.max_decoded_bytes.0)
        .ok_or(ProcessingError::ResourceLimit("展开 PNG 缓冲区"))?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(size)
        .map_err(|_| ProcessingError::ResourceLimit("展开 PNG 内存"))?;
    raw.resize(size, 0);
    let frame = reader.next_frame(&mut raw).map_err(decode_error)?;
    reader.finish().map_err(decode_error)?;
    if frame.bit_depth != png::BitDepth::Eight
        || frame.width != info.width
        || frame.height != info.height
    {
        return Err(ProcessingError::ValidationFailed(
            "展开 PNG 格式不符合量化边界",
        ));
    }
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(ProcessingError::ValidationFailed("PNG 索引色未展开")),
    };
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact((rgba_bytes / 4) as usize)
        .map_err(|_| ProcessingError::ResourceLimit("量化 RGBA 内存"))?;
    for pixel in raw[..frame.buffer_size()].chunks_exact(channels) {
        pixels.push(match frame.color_type {
            png::ColorType::Grayscale => RGBA::new(pixel[0], pixel[0], pixel[0], 255),
            png::ColorType::GrayscaleAlpha => RGBA::new(pixel[0], pixel[0], pixel[0], pixel[1]),
            png::ColorType::Rgb => RGBA::new(pixel[0], pixel[1], pixel[2], 255),
            png::ColorType::Rgba => RGBA::new(pixel[0], pixel[1], pixel[2], pixel[3]),
            _ => return Err(ProcessingError::ValidationFailed("PNG 色型未展开")),
        });
    }
    Ok(pixels)
}

fn encode_palette(
    info: &ImageInfo,
    palette: &[RGBA],
    indices: &[u8],
) -> Result<Vec<u8>, ProcessingError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, info.width, info.height);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_compression(png::Compression::Balanced);
        encoder.set_palette(
            palette
                .iter()
                .flat_map(|pixel| [pixel.r, pixel.g, pixel.b])
                .collect::<Vec<_>>(),
        );
        if palette.iter().any(|pixel| pixel.a != 255) {
            encoder.set_trns(palette.iter().map(|pixel| pixel.a).collect::<Vec<_>>());
        }
        let mut writer = encoder.write_header().map_err(png_error)?;
        writer.write_image_data(indices).map_err(png_error)?;
        writer.finish().map_err(png_error)?;
    }
    Ok(bytes)
}

fn merge_metadata(original: &[u8], encoded: &[u8]) -> Result<Vec<u8>, ProcessingError> {
    let source = probe::chunks(original)?;
    let palette_png = probe::chunks(encoded)?;
    let mut output = Vec::with_capacity(original.len().max(encoded.len()));
    output.extend_from_slice(&encoded[..8]);
    output.extend_from_slice(palette_png[0].encoded);
    let mut past_idat = false;
    // 与色型有关的 PLTE/tRNS 由量化器重新生成，其余数据按原来的 IDAT 前后位置保留。
    for chunk in &source {
        if chunk.name == *b"IDAT" {
            past_idat = true;
        }
        if !past_idat && !matches!(&chunk.name, b"IHDR" | b"PLTE" | b"tRNS") {
            output.extend_from_slice(chunk.encoded);
        }
    }
    for chunk in &palette_png {
        if !matches!(&chunk.name, b"IHDR" | b"IEND") {
            output.extend_from_slice(chunk.encoded);
        }
    }
    past_idat = false;
    for chunk in &source {
        if chunk.name == *b"IDAT" {
            past_idat = true;
        }
        if past_idat && !matches!(&chunk.name, b"IDAT" | b"IEND" | b"PLTE" | b"tRNS") {
            output.extend_from_slice(chunk.encoded);
        }
    }
    let end = palette_png
        .last()
        .ok_or(ProcessingError::ValidationFailed("编码结果缺少 IEND"))?;
    output.extend_from_slice(end.encoded);
    Ok(output)
}

fn quant_error(error: imagequant::Error) -> ProcessingError {
    match error {
        imagequant::Error::Aborted => ProcessingError::Cancelled,
        imagequant::Error::OutOfMemory => ProcessingError::ResourceLimit("imagequant 内存"),
        other => ProcessingError::Encode(Box::new(other)),
    }
}

fn png_error(error: png::EncodingError) -> ProcessingError {
    ProcessingError::Encode(Box::new(error))
}

fn decode_error(error: png::DecodingError) -> ProcessingError {
    match error {
        png::DecodingError::LimitsExceeded => ProcessingError::ResourceLimit("量化解码器"),
        other => ProcessingError::Decode(Box::new(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::alpha_acceptable;

    #[test]
    fn alpha_guard_preserves_endpoints_and_bounds_partial_transparency() {
        assert!(alpha_acceptable(0, 0));
        assert!(alpha_acceptable(255, 255));
        assert!(alpha_acceptable(128, 136));
        assert!(!alpha_acceptable(0, 1));
        assert!(!alpha_acceptable(255, 254));
        assert!(!alpha_acceptable(1, 0));
        assert!(!alpha_acceptable(254, 255));
        assert!(!alpha_acceptable(128, 137));
    }
}
