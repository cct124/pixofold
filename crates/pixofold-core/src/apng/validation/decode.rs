//! 逐子帧双解码：原始样本摘要保留位深/隐藏RGB，EXPAND只用于有界RGBA16合成。
use super::{
    structure::{self, Frame, Parsed},
    *,
};
use std::io::Cursor;

fn static_png(p: &Parsed<'_>, frame: &Frame<'_>) -> Vec<u8> {
    let mut out = structure::SIGNATURE.to_vec();
    let mut header = p.ihdr.to_vec();
    header[..4].copy_from_slice(&frame.control.width.to_be_bytes());
    header[4..8].copy_from_slice(&frame.control.height.to_be_bytes());
    structure::chunk(&mut out, b"IHDR", &header);
    for (kind, data) in &p.palette {
        structure::chunk(&mut out, kind, data);
    }
    for data in &frame.data {
        structure::chunk(&mut out, b"IDAT", data);
    }
    structure::chunk(&mut out, b"IEND", &[]);
    out
}
fn read(
    data: &[u8],
    expand: bool,
    limits: &Limits,
    guard: &Guard<'_>,
) -> Result<(Vec<u8>, png::OutputInfo)> {
    guard.check()?;
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(data),
        png::Limits {
            bytes: limits
                .max_validation_bytes
                .try_into()
                .map_err(|_| Error("validation_budget"))?,
        },
    );
    decoder.set_transformations(if expand {
        png::Transformations::EXPAND
    } else {
        png::Transformations::IDENTITY
    });
    let mut reader = decoder.read_info().map_err(|_| Error("decode"))?;
    let size = reader
        .output_buffer_size()
        .ok_or(Error("validation_budget"))?;
    require(
        size as u64 <= limits.max_validation_bytes,
        "validation_budget",
    )?;
    let mut output = vec![0; size];
    let info = reader
        .next_frame(&mut output)
        .map_err(|_| Error("decode"))?;
    reader.finish().map_err(|_| Error("decode"))?;
    output.truncate(info.buffer_size());
    guard.check()?;
    Ok((output, info))
}
pub(super) fn decode(
    p: &Parsed<'_>,
    frame: &Frame<'_>,
    limits: &Limits,
    guard: &Guard<'_>,
) -> Result<(String, Vec<[u16; 4]>)> {
    strict_zlib(p, frame, guard)?;
    let bytes = static_png(p, frame);
    let (raw, _) = read(&bytes, false, limits, guard)?;
    if p.ihdr[9] == 3 {
        let entries = p
            .palette
            .iter()
            .find(|(k, _)| k == b"PLTE")
            .ok_or(Error("palette"))?
            .1
            .len()
            / 3;
        require(raw.iter().all(|v| usize::from(*v) < entries), "decode")?;
    }
    let raw_hash = hash(&raw);
    drop(raw);
    let (expanded, info) = read(&bytes, true, limits, guard)?;
    let wide = info.bit_depth == png::BitDepth::Sixteen;
    require(wide || info.bit_depth == png::BitDepth::Eight, "decode")?;
    let channels = info.color_type.samples();
    let stride = channels * if wide { 2 } else { 1 };
    let mut pixels = Vec::with_capacity(expanded.len() / stride);
    for (index, pixel) in expanded.chunks_exact(stride).enumerate() {
        if index.is_multiple_of(4096) {
            guard.check()?;
        }
        let channel = |n: usize| -> u16 {
            if wide {
                u16::from_be_bytes([pixel[n * 2], pixel[n * 2 + 1]])
            } else {
                u16::from(pixel[n]) * 257
            }
        };
        pixels.push(match info.color_type {
            png::ColorType::Grayscale => [channel(0), channel(0), channel(0), 65535],
            png::ColorType::GrayscaleAlpha => [channel(0), channel(0), channel(0), channel(1)],
            png::ColorType::Rgb => [channel(0), channel(1), channel(2), 65535],
            png::ColorType::Rgba => [channel(0), channel(1), channel(2), channel(3)],
            png::ColorType::Indexed => return Err(Error("decode")),
        });
    }
    Ok((raw_hash, pixels))
}
fn strict_zlib(p: &Parsed<'_>, frame: &Frame<'_>, guard: &Guard<'_>) -> Result<()> {
    let channels = match p.ihdr[9] {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Err(Error("decode")),
    };
    let bpp = channels * usize::from(p.ihdr[8] / 8);
    let (w, h) = (frame.control.width as usize, frame.control.height as usize);
    let adam = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    let plain = [(0, 0, 1, 1)];
    let passes = if p.ihdr[12] == 1 {
        &adam[..]
    } else {
        &plain[..]
    };
    let expected: usize = passes
        .iter()
        .filter(|&&(x, y, _, _)| x < w && y < h)
        .map(|&(x, y, dx, dy)| ((w - x).div_ceil(dx) * bpp + 1) * (h - y).div_ceil(dy))
        .sum();
    let mut output = vec![0; expected + 1];
    let mut decoder = fdeflate::Decompressor::new();
    let mut written = 0;
    let compressed = frame.data.concat();
    let mut at = 0;
    // fdeflate会提前读取最多7字节。最后8字节逐字节喂入，防止尾随数据被预读吞掉。
    // 更早结束的流尚有未交给解码器的尾部，同样被精确消耗检查拒绝。
    let tail = compressed.len().saturating_sub(8);
    while at < compressed.len() {
        guard.check()?;
        let end = if at < tail {
            (at + 65536).min(tail)
        } else {
            at + 1
        };
        let (used, produced) = decoder
            .read(
                &compressed[at..end],
                &mut output,
                written,
                end == compressed.len(),
            )
            .map_err(|_| Error("decode"))?;
        at += used;
        written += produced;
        require(written <= expected && (used > 0 || produced > 0), "decode")?;
        if decoder.is_done() {
            require(at == compressed.len(), "decode")?;
        }
    }
    guard.check()?;
    require(decoder.is_done() && written == expected, "decode")
}
pub(super) fn over(source: [u16; 4], dest: [u16; 4]) -> [u16; 4] {
    if source[3] == 0 {
        return dest;
    }
    let m = 65535u64;
    let (a, b) = (u64::from(source[3]), u64::from(dest[3]));
    let denominator = a * m + b * (m - a);
    let mut out = [0; 4];
    for n in 0..3 {
        out[n] =
            ((u64::from(source[n]) * a * m + u64::from(dest[n]) * b * (m - a) + denominator / 2)
                / denominator) as u16;
    }
    out[3] = ((denominator + m / 2) / m) as u16;
    out
}
fn canvas_hash(canvas: &[[u16; 4]]) -> String {
    let mut digest = Sha256::new();
    for pixel in canvas {
        for channel in pixel {
            digest.update(channel.to_be_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

pub(super) fn preview(
    p: &Parsed<'_>,
    input_bytes: usize,
    limits: &Limits,
    guard: &Guard<'_>,
) -> Result<super::super::ApngPreview> {
    let width = structure::u32_at(p.ihdr, 0);
    let height = structure::u32_at(p.ihdr, 4);
    let frame = p.frames.first().ok_or(Error("frame_count"))?;
    let c = &frame.control;
    let canvas_pixels = u64::from(width) * u64::from(height);
    let frame_pixels = u64::from(c.width) * u64::from(c.height);
    require(
        frame_pixels <= limits.max_total_pixels,
        "total_pixel_budget",
    )?;
    require(
        frame_pixels * 25 <= limits.max_decoded_bytes,
        "decoded_budget",
    )?;
    // 共享双解码/严格zlib，先按其最坏缓冲及压缩流副本计费再分配；不解码独立默认图。
    let bound = canvas_pixels * 4
        + frame_pixels * 48
        + input_bytes as u64 * 3
        + p.frames.len() as u64 * 8192
        + (1 << 20);
    require(bound <= limits.max_validation_bytes, "validation_budget")?;
    let (_, pixels) = decode(p, frame, limits, guard)?;
    let mut canvas = vec![0; canvas_pixels as usize * 4];
    for y in 0..c.height {
        guard.check()?;
        for x in 0..c.width {
            let source = pixels[(y * c.width + x) as usize];
            let value = if c.blend == 0 {
                source
            } else {
                over(source, [0; 4])
            };
            let pos = ((y + c.y) * width + x + c.x) as usize * 4;
            for n in 0..4 {
                canvas[pos + n] = ((u32::from(value[n]) + 128) / 257) as u8;
            }
        }
    }
    guard.check()?;
    Ok(super::super::ApngPreview {
        width,
        height,
        pixels: canvas,
    })
}
pub(super) fn inspect(
    p: &Parsed<'_>,
    input_bytes: usize,
    limits: &Limits,
    guard: &Guard<'_>,
) -> Result<Summary> {
    let width = structure::u32_at(p.ihdr, 0);
    let height = structure::u32_at(p.ihdr, 4);
    let canvas_pixels = u64::from(width) * u64::from(height);
    let mut total = if p.default_participates {
        0
    } else {
        canvas_pixels
    };
    let mut largest = canvas_pixels;
    for frame in &p.frames {
        let pixels = u64::from(frame.control.width) * u64::from(frame.control.height);
        let plays = if p.plays == 1 { 1 } else { 2 };
        total = total
            .checked_add(pixels * plays)
            .ok_or(Error("total_pixel_budget"))?;
        largest = largest.max(pixels);
    }
    require(largest <= limits.max_frame_pixels, "frame_pixel_budget")?;
    require(total <= limits.max_total_pixels, "total_pixel_budget")?;
    // 两次解码的最坏原始/展开通道字节；16位为每通道两字节，Adam7由png按原样解码。
    let sample_bytes = if p.ihdr[8] == 16 { 2 } else { 1 };
    let channels = match p.ihdr[9] {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Err(Error("decode")),
    };
    // 严格zlib预检也计费；每像素额外1字节保守覆盖行/Adam7滤波字节。
    let decoded = total
        .checked_mul((channels * 2 + 4) * sample_bytes + 1)
        .ok_or(Error("decoded_budget"))?;
    require(decoded <= limits.max_decoded_bytes, "decoded_budget")?;
    // 两画布、原始/展开/规范像素/解码scratch、重建流及有界帧摘要。不是编码器RSS硬上限。
    let bound = canvas_pixels * 16
        + largest * 48
        + (input_bytes as u64) * 3
        + (p.frames.len() as u64) * 8192
        + (1 << 20);
    require(bound <= limits.max_validation_bytes, "validation_budget")?;
    let mut canvas = vec![[0u16; 4]; canvas_pixels as usize];
    let mut default_hash = String::new();
    if !p.default_participates {
        let frame = Frame {
            control: Control {
                width,
                height,
                x: 0,
                y: 0,
                delay_num: 0,
                delay_den: 0,
                dispose: 0,
                blend: 0,
            },
            data: p.default.clone(),
        };
        default_hash = decode(p, &frame, limits, guard)?.0;
    }
    let mut frames = Vec::with_capacity(p.frames.len());
    let mut final_hash = String::new();
    let mut second_play_canvases = Vec::new();
    for play in 0..if p.plays == 1 { 1 } else { 2 } {
        // 真正重放第二轮，清除上一轮最后的显示/dispose状态；摘要用于人工语料回归。
        canvas.fill([0; 4]);
        for (number, frame) in p.frames.iter().enumerate() {
            guard.check()?;
            let c = &frame.control;
            let (raw, pixels) = decode(p, frame, limits, guard)?;
            if number == 0 && p.default_participates {
                default_hash = raw.clone();
            }
            // PREVIOUS首帧按BACKGROUND；每轮重新创建透明黑色画布，绝不携带默认图或上一轮残留。
            let previous = if c.dispose == 2 && number > 0 {
                Some(canvas.clone())
            } else {
                None
            };
            for y in 0..c.height {
                guard.check()?;
                for x in 0..c.width {
                    let pos = ((y + c.y) * width + x + c.x) as usize;
                    let source = pixels[(y * c.width + x) as usize];
                    canvas[pos] = if c.blend == 0 {
                        source
                    } else {
                        over(source, canvas[pos])
                    };
                }
            }
            let digest = canvas_hash(&canvas);
            final_hash = digest.clone();
            if play == 0 {
                frames.push(FrameSummary {
                    control: c.clone(),
                    effective_delay: [
                        c.delay_num,
                        if c.delay_den == 0 { 100 } else { c.delay_den },
                    ],
                    raw_sha256: raw,
                    canvas_sha256: digest,
                    canvas: (canvas_pixels <= 64).then(|| canvas.clone()),
                });
            } else {
                second_play_canvases.push(digest);
            }
            // 有限播放结束停在最后显示帧，摘要在dispose前取得；dispose只用于准备下一帧。
            if let Some(previous) = previous {
                canvas = previous;
            } else if c.dispose == 1 || c.dispose == 2 {
                for y in c.y..c.y + c.height {
                    guard.check()?;
                    canvas[(y * width + c.x) as usize..(y * width + c.x + c.width) as usize]
                        .fill([0; 4]);
                }
            }
        }
    }
    guard.check()?;
    Ok(Summary {
        width,
        height,
        depth: p.ihdr[8],
        color: p.ihdr[9],
        interlace: p.ihdr[12],
        plays: p.plays,
        default_participates: p.default_participates,
        default_raw_sha256: default_hash,
        metadata_sha256: p.metadata_hash.clone(),
        frames,
        final_canvas_sha256: final_hash,
        second_play_canvases,
        loop_start: "transparent_black_each_play",
        decoded_bytes: decoded,
        decoded_pixels: total,
        validation_bound: bound,
    })
}
