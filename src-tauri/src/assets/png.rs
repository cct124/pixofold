//! PNG预览解码，以及PNG/JPEG共用的有界方向采样和小PNG编码。
//! 单次受文件/尺寸/解码预算约束，跳过文本/ICC解压；不改变源文件和凭据。

use super::{AssetError, ThumbnailDto};
use std::io::Cursor;

pub(super) const MAX_WIDTH: u32 = 128;
pub(super) const MAX_HEIGHT: u32 = 96;
pub(super) const MAX_PNG_BYTES: usize = 65_536;
pub(super) const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
pub(super) const MAX_PIXELS: u64 = 8 * 1024 * 1024;
pub(super) const DECODE_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_EDGE: u32 = 16_384;

pub(super) fn decode(input: &[u8]) -> Result<ThumbnailDto, AssetError> {
    if input.len() as u64 > MAX_INPUT_BYTES {
        return Err(AssetError::ResourceLimit);
    }
    // 在png解析可变长元数据之前收紧尺寸；库仍会验证CRC、色型和完整图像。
    if input.len() < 33 || &input[..8] != b"\x89PNG\r\n\x1a\n" || &input[12..16] != b"IHDR" {
        return Err(AssetError::DecodeFailed);
    }
    let width = u32::from_be_bytes(
        input[16..20]
            .try_into()
            .map_err(|_| AssetError::DecodeFailed)?,
    );
    let height = u32::from_be_bytes(
        input[20..24]
            .try_into()
            .map_err(|_| AssetError::DecodeFailed)?,
    );
    if width == 0 || height == 0 {
        return Err(AssetError::DecodeFailed);
    }
    if width > MAX_EDGE || height > MAX_EDGE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(AssetError::ResourceLimit);
    }
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(input),
        png::Limits {
            bytes: DECODE_BYTES,
        },
    );
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(decode_error)?;
    if reader.info().animation_control.is_some() {
        return Err(AssetError::Unavailable);
    }
    let size = reader
        .output_buffer_size()
        .filter(|size| *size <= DECODE_BYTES)
        .ok_or(AssetError::ResourceLimit)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(size)
        .map_err(|_| AssetError::ResourceLimit)?;
    pixels.resize(size, 0);
    let frame = reader.next_frame(&mut pixels).map_err(decode_error)?;
    reader.finish().map_err(decode_error)?;
    if reader.info().animation_control.is_some() {
        return Err(AssetError::Unavailable);
    }
    if frame.bit_depth != png::BitDepth::Eight || frame.width != width || frame.height != height {
        return Err(AssetError::DecodeFailed);
    }
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err(AssetError::DecodeFailed),
    };
    from_pixels(&pixels[..frame.buffer_size()], width, height, channels, 1)
}

/// 输入必须为紧密排列的8位灰度/灰度alpha/RGB/RGBA；Exif方向只作用于预览。
pub(super) fn from_pixels(
    pixels: &[u8],
    source_width: u32,
    source_height: u32,
    channels: usize,
    orientation: u8,
) -> Result<ThumbnailDto, AssetError> {
    if source_width == 0
        || source_height == 0
        || !(1..=4).contains(&channels)
        || !(1..=8).contains(&orientation)
    {
        return Err(AssetError::DecodeFailed);
    }
    let pixel_count = u64::from(source_width) * u64::from(source_height);
    if source_width > MAX_EDGE
        || source_height > MAX_EDGE
        || pixel_count > MAX_PIXELS
        || pixel_count * channels as u64 > DECODE_BYTES as u64
    {
        return Err(AssetError::ResourceLimit);
    }
    if pixel_count * channels as u64 != pixels.len() as u64 {
        return Err(AssetError::DecodeFailed);
    }
    let (width, height) = if orientation >= 5 {
        (source_height, source_width)
    } else {
        (source_width, source_height)
    };
    let scale = (f64::from(MAX_WIDTH) / f64::from(width))
        .min(f64::from(MAX_HEIGHT) / f64::from(height))
        .min(1.0);
    let out_width = (f64::from(width) * scale).floor().max(1.0) as u32;
    let out_height = (f64::from(height) * scale).floor().max(1.0) as u32;
    let mut rgba = Vec::with_capacity((out_width * out_height * 4) as usize);
    // 有界box采样使用预乘alpha求色彩均值，避免透明边缘被隐藏RGB染黑。
    for y in 0..out_height {
        for x in 0..out_width {
            let mut sums = [0_u64; 4];
            let mut count = 0_u64;
            for sy in y * height / out_height..(y + 1) * height / out_height {
                for sx in x * width / out_width..(x + 1) * width / out_width {
                    let (source_x, source_y) = match orientation {
                        1 => (sx, sy),
                        2 => (source_width - 1 - sx, sy),
                        3 => (source_width - 1 - sx, source_height - 1 - sy),
                        4 => (sx, source_height - 1 - sy),
                        5 => (sy, sx),
                        6 => (sy, source_height - 1 - sx),
                        7 => (source_width - 1 - sy, source_height - 1 - sx),
                        _ => (source_width - 1 - sy, sx),
                    };
                    let offset =
                        (source_y as usize * source_width as usize + source_x as usize) * channels;
                    let pixel = &pixels[offset..offset + channels];
                    let (r, g, b, a) = match channels {
                        1 => (pixel[0], pixel[0], pixel[0], 255),
                        2 => (pixel[0], pixel[0], pixel[0], pixel[1]),
                        3 => (pixel[0], pixel[1], pixel[2], 255),
                        _ => (pixel[0], pixel[1], pixel[2], pixel[3]),
                    };
                    for (sum, value) in sums.iter_mut().zip([r, g, b]) {
                        *sum += u64::from(value) * u64::from(a);
                    }
                    sums[3] += u64::from(a);
                    count += 1;
                }
            }
            for sum in &sums[..3] {
                rgba.push(sum.checked_div(sums[3]).unwrap_or(0) as u8);
            }
            rgba.push((sums[3] / count) as u8);
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, out_width, out_height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder
            .write_header()
            .map_err(|_| AssetError::DecodeFailed)?;
        writer
            .write_image_data(&rgba)
            .map_err(|_| AssetError::DecodeFailed)?;
        writer.finish().map_err(|_| AssetError::DecodeFailed)?;
    }
    if png.len() > MAX_PNG_BYTES {
        return Err(AssetError::ResourceLimit);
    }
    Ok(ThumbnailDto {
        width: out_width,
        height: out_height,
        png,
    })
}

fn decode_error(error: png::DecodingError) -> AssetError {
    match error {
        png::DecodingError::LimitsExceeded => AssetError::ResourceLimit,
        _ => AssetError::DecodeFailed,
    }
}
