//! 可复现观感样本：整数调色板动画；照片平移衍生自随仓库保存的公开静态GIF。
use std::{
    borrow::Cow,
    error::Error,
    fs::OpenOptions,
    io::{Cursor, Write},
    path::Path,
};

fn dark_noise() -> Result<Vec<u8>, Box<dyn Error>> {
    // 低亮度32阶确定性噪声，供单文件核心和桌面混批共同验证质量回退。
    let palette: Vec<u8> = (0..32u8).flat_map(|n| [n, n, n]).collect();
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut bytes, 128, 128, &palette)?;
        encoder.set_repeat(gif::Repeat::Infinite)?;
        let mut state = 0x12345678u32;
        for _ in 0..3 {
            let pixels = (0..128 * 128)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    (state & 31) as u8
                })
                .collect();
            encoder.write_frame(&gif::Frame {
                width: 128,
                height: 128,
                delay: 10,
                dispose: gif::DisposalMethod::Keep,
                buffer: Cow::Owned(pixels),
                ..Default::default()
            })?;
        }
    }
    Ok(bytes)
}

fn write(
    directory: &Path,
    name: &str,
    width: u16,
    height: u16,
    palette: &[u8],
    frames: Vec<Vec<u8>>,
    transparent: Option<u8>,
) -> Result<(), Box<dyn Error>> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))?;
    let mut encoder = gif::Encoder::new(file, width, height, palette)?;
    encoder.set_repeat(gif::Repeat::Infinite)?;
    for pixels in frames {
        let frame = gif::Frame {
            width,
            height,
            delay: 10,
            dispose: gif::DisposalMethod::Background,
            transparent,
            buffer: Cow::Owned(pixels),
            ..gif::Frame::default()
        };
        encoder.write_frame(&frame)?;
    }
    Ok(())
}

pub fn generate(directory: &Path, photo_gif: &[u8]) -> Result<(), Box<dyn Error>> {
    // 三像素循环最小样本：最后一帧若被改成Keep，第二轮首帧会残留右侧红点。
    write(
        directory,
        "transparent-seam.gif",
        3,
        1,
        &[0, 0, 0, 255, 0, 0],
        vec![vec![1, 0, 0], vec![0, 0, 1]],
        Some(0),
    )?;
    let mut palette = Vec::new();
    for i in 0..256u16 {
        palette.extend_from_slice(&[
            ((i >> 5) * 255 / 7) as u8,
            (((i >> 2) & 7) * 255 / 7) as u8,
            ((i & 3) * 85) as u8,
        ]);
    }
    let (width, height) = (192u16, 128u16);
    for kind in ["gradient-motion", "transparent-illustration", "ui-motion"] {
        let mut frames = Vec::new();
        for frame in 0..12usize {
            let mut pixels = Vec::new();
            for y in 0..usize::from(height) {
                for x in 0..usize::from(width) {
                    let value = match kind {
                        "gradient-motion" => {
                            (((x + frame * 5) % 192) * 7 / 191 * 32
                                + y * 7 / 127 * 4
                                + (x + y + frame) % 4) as u8
                        }
                        "transparent-illustration" => {
                            let dx = x as i32 - (40 + frame as i32 * 8);
                            let dy = y as i32 - 64;
                            if dx * dx + dy * dy < 900 {
                                ((x + y) % 223 + 32) as u8
                            } else {
                                0
                            }
                        }
                        _ => {
                            // 合成录屏式UI：工具栏、细线文字、列表选中及鼠标运动；不冒称真实录屏。
                            if y < 18 {
                                37
                            } else if (32..108).contains(&y)
                                && (y - 32) / 12 == frame % 6
                                && (12..180).contains(&x)
                            {
                                219
                            } else if (18..150).contains(&x)
                                && (30..110).contains(&y)
                                && y % 12 < 6
                                && x % 7 < 4
                            {
                                36
                            } else if x.abs_diff(30 + frame * 11) < 3 && y.abs_diff(70) < 12 {
                                224
                            } else {
                                255
                            }
                        }
                    };
                    pixels.push(value);
                }
            }
            frames.push(pixels);
        }
        write(
            directory,
            &format!("{kind}.gif"),
            width,
            height,
            &palette,
            frames,
            (kind == "transparent-illustration").then_some(0),
        )?;
    }
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = options.read_info(Cursor::new(photo_gif))?;
    let global = decoder.global_palette().ok_or("照片缺少调色板")?.to_vec();
    let frame = decoder.read_next_frame()?.ok_or("照片缺少帧")?;
    if frame.width != 128 || frame.height != 128 || frame.transparent.is_some() {
        return Err("照片身份/尺寸不符".into());
    }
    let mut frames = Vec::new();
    for step in 0..12usize {
        let mut pixels = Vec::new();
        for y in 0..96usize {
            for x in 0..96usize {
                pixels.push(frame.buffer[(y + step * 2) * 128 + x + step * 2]);
            }
        }
        frames.push(pixels);
    }
    write(directory, "photo-pan.gif", 96, 96, &global, frames, None)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("dark-noise.gif"))?
        .write_all(&dark_noise()?)?;
    Ok(())
}
