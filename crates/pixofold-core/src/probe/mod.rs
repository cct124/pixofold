//! 内容识别与有界解码；先检查 PNG 结构、尺寸及动画标记，再分配解码缓冲区。
//! 扩展名不参与判断；压缩文本和 ICC 按原始 chunk 保留，不在此展开元数据。

use std::io::Cursor;

use crate::model::{ImageInfo, PngColorType, ProcessingError, ResourceLimits};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_CHUNKS: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Chunk<'a> {
    pub name: [u8; 4],
    pub data: &'a [u8],
    pub encoded: &'a [u8],
}

pub(crate) struct DecodedPng {
    pub info: ImageInfo,
    pub pixels: Vec<u8>,
}

/// 从内存识别并完整解码静态 PNG，不修改输入，无文件或网络副作用。
///
/// # Errors
/// 非 PNG、动画、损坏数据或资源超限均返回可识别错误；不根据扩展名放行。
pub fn inspect_png(data: &[u8], limits: ResourceLimits) -> Result<ImageInfo, ProcessingError> {
    Ok(decode(data, limits)?.info)
}

pub(crate) fn chunks(data: &[u8]) -> Result<Vec<Chunk<'_>>, ProcessingError> {
    if !data.starts_with(PNG_SIGNATURE) {
        return Err(ProcessingError::UnsupportedFormat);
    }
    let mut result = Vec::new();
    let mut offset = PNG_SIGNATURE.len();
    let mut seen_idat = false;
    let mut closed_idat = false;
    while offset < data.len() {
        if result.len() == MAX_CHUNKS {
            return Err(ProcessingError::ResourceLimit("PNG chunk 数量"));
        }
        let header = data
            .get(offset..offset + 8)
            .ok_or(ProcessingError::InvalidPng("chunk 头截断"))?;
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let name = [header[4], header[5], header[6], header[7]];
        if len > 0x7fff_ffff || !name.iter().all(u8::is_ascii_alphabetic) || name[2] & 32 != 0 {
            return Err(ProcessingError::InvalidPng("chunk 类型或长度无效"));
        }
        let end = offset
            .checked_add(12)
            .and_then(|n| n.checked_add(len))
            .filter(|end| *end <= data.len())
            .ok_or(ProcessingError::InvalidPng("chunk 数据截断"))?;
        let payload = &data[offset + 8..end - 4];
        let crc = &data[end - 4..end];
        if crc32fast::hash(&data[offset + 4..end - 4]).to_be_bytes() != crc {
            return Err(ProcessingError::InvalidPng("chunk CRC 不匹配"));
        }
        if result.is_empty() && (name != *b"IHDR" || len != 13) {
            return Err(ProcessingError::InvalidPng("首个 chunk 必须为 IHDR"));
        }
        if name == *b"IHDR" && !result.is_empty() {
            return Err(ProcessingError::InvalidPng("重复 IHDR"));
        }
        if matches!(&name, b"acTL" | b"fcTL" | b"fdAT") {
            return Err(ProcessingError::UnsupportedAnimation);
        }
        if name[0] & 32 == 0 && !matches!(&name, b"IHDR" | b"PLTE" | b"IDAT" | b"IEND") {
            return Err(ProcessingError::InvalidPng("未知关键 chunk"));
        }
        if name == *b"IDAT" {
            if closed_idat {
                return Err(ProcessingError::InvalidPng("IDAT 必须连续"));
            }
            seen_idat = true;
        } else if seen_idat {
            closed_idat = true;
        }
        result.push(Chunk {
            name,
            data: payload,
            encoded: &data[offset..end],
        });
        if name == *b"IEND" {
            if len != 0 || !seen_idat || end != data.len() {
                return Err(ProcessingError::InvalidPng("IEND 无效或含尾随数据"));
            }
            return Ok(result);
        }
        offset = end;
    }
    Err(ProcessingError::InvalidPng("缺少 IEND"))
}

pub(crate) fn decode(data: &[u8], limits: ResourceLimits) -> Result<DecodedPng, ProcessingError> {
    limits.validate()?;
    if data.len() as u64 > limits.max_input_bytes.0 {
        return Err(ProcessingError::ResourceLimit("输入文件字节数"));
    }
    let parsed = chunks(data)?;
    let header = parsed[0].data;
    let width = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let height = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    if width == 0 || height == 0 {
        return Err(ProcessingError::InvalidPng("尺寸必须非零"));
    }
    if width > limits.max_dimension || height > limits.max_dimension {
        return Err(ProcessingError::ResourceLimit("图片单边尺寸"));
    }
    if u64::from(width) * u64::from(height) > limits.max_pixels {
        return Err(ProcessingError::ResourceLimit("图片像素数"));
    }
    let (color_type, channels) = match header[9] {
        0 => (PngColorType::Grayscale, 1_u64),
        2 => (PngColorType::Rgb, 3),
        3 => (PngColorType::Indexed, 1),
        4 => (PngColorType::GrayscaleAlpha, 2),
        6 => (PngColorType::Rgba, 4),
        _ => return Err(ProcessingError::InvalidPng("色型无效")),
    };
    // IDENTITY 解码保持 1/2/4-bit 的 packed 样本与 16-bit 大端字节，不截断精度。
    let valid_depth = match header[9] {
        0 => matches!(header[8], 1 | 2 | 4 | 8 | 16),
        3 => matches!(header[8], 1 | 2 | 4 | 8),
        _ => matches!(header[8], 8 | 16),
    };
    if !valid_depth || header[10] != 0 || header[11] != 0 || header[12] > 1 {
        return Err(ProcessingError::InvalidPng("位深或编码方法无效"));
    }
    let decoded_bytes = (u64::from(width) * channels * u64::from(header[8]))
        .div_ceil(8)
        .checked_mul(u64::from(height))
        .ok_or(ProcessingError::ResourceLimit("解码大小溢出"))?;
    if decoded_bytes > limits.max_decoded_bytes.0 {
        return Err(ProcessingError::ResourceLimit("解码像素字节数"));
    }
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_limits(png::Limits {
        bytes: limits.max_decoded_bytes.0 as usize,
    });
    decoder.set_transformations(png::Transformations::IDENTITY);
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    let mut reader = decoder.read_info().map_err(decode_error)?;
    let size = reader
        .output_buffer_size()
        .ok_or(ProcessingError::ResourceLimit("解码缓冲区"))?;
    if size as u64 > limits.max_decoded_bytes.0 {
        return Err(ProcessingError::ResourceLimit("解码缓冲区"));
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(size)
        .map_err(|_| ProcessingError::ResourceLimit("解码内存分配"))?;
    pixels.resize(size, 0);
    let frame = reader.next_frame(&mut pixels).map_err(decode_error)?;
    pixels.truncate(frame.buffer_size());
    reader.finish().map_err(decode_error)?;
    Ok(DecodedPng {
        info: ImageInfo {
            width,
            height,
            bit_depth: header[8],
            color_type,
            interlaced: header[12] == 1,
        },
        pixels,
    })
}

fn decode_error(error: png::DecodingError) -> ProcessingError {
    match error {
        png::DecodingError::LimitsExceeded => ProcessingError::ResourceLimit("PNG 解码器"),
        other => ProcessingError::Decode(Box::new(other)),
    }
}
