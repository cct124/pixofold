//! 仅用于识别图片的静态PNG小预览，不参与压缩、色彩保真判断或最终输出。
//! 单次受文件/尺寸/解码预算约束，跳过文本/ICC解压；不改变源文件和凭据。

use super::{AssetError, ThumbnailDto};
use std::io::Cursor;

pub(super) const MAX_WIDTH: u32 = 128;
pub(super) const MAX_HEIGHT: u32 = 96;
pub(super) const MAX_PNG_BYTES: usize = 65_536;
pub(super) const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PIXELS: u64 = 8 * 1024 * 1024;
const DECODE_BYTES: usize = 32 * 1024 * 1024;
const MAX_EDGE: u32 = 16_384;

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
                    let offset = sy as usize * frame.line_size + sx as usize * channels;
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
