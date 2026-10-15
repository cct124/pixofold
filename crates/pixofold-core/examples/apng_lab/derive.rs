//! 固定公开PNG的衍生动画生成器；缩放/平移属于测试输入构造，不计入压缩收益。
use serde_json::json;
use std::{io::Cursor, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run(input: &Path, output: &Path) -> Result<()> {
    let source = super::bounded(input, 16 << 20)?;
    let before = super::hash(&source);
    let mut decoder =
        png::Decoder::new_with_limits(Cursor::new(&source), png::Limits { bytes: 16 << 20 });
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || u64::from(info.width) * u64::from(info.height) > 4 << 20
        || info.bit_depth != png::BitDepth::Eight
        || info.animation_control.is_some()
    {
        return Err("fixture source must be bounded static 8-bit PNG".into());
    }
    let size = reader.output_buffer_size().ok_or("fixture buffer")?;
    if size > 16 << 20 {
        return Err("fixture buffer budget".into());
    }
    let mut pixels = vec![0; size];
    let info = reader.next_frame(&mut pixels)?;
    reader.finish()?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err("fixture RGB/RGBA required".into()),
    };
    let (w, h) = (192u32, 144u32);
    let (canvas_w, canvas_h) = (w + 16, h + 8);
    let frame = |number: u32| {
        let mut frame = vec![0; (canvas_w * canvas_h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let src =
                    (((y * info.height / h) * info.width + x * info.width / w) as usize) * channels;
                let at = (((y + number % 3 * 2) * canvas_w + x + number % 5 * 4) * 4) as usize;
                frame[at..at + 3].copy_from_slice(&pixels[src..src + 3]);
                frame[at + 3] = if channels == 4 { pixels[src + 3] } else { 255 };
            }
        }
        frame
    };
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, canvas_w, canvas_h);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_animated(6, 0)?;
        encoder.set_sep_def_img(true)?;
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&frame(0))?;
        for n in 0..6 {
            writer.set_frame_delay(n as u16 + 1, 30)?;
            writer.write_image_data(&frame(n))?;
        }
        writer.finish()?;
    }
    pixofold_core::apng::validate_apng(&bytes, &Default::default(), &Default::default())?;
    if super::hash(&super::bounded(input, 16 << 20)?) != before {
        return Err("fixture source changed".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    std::io::Write::write_all(&mut file, &bytes)?;
    println!(
        "{}",
        json!({"sourceSha256":before,"sha256":super::hash(&bytes),"bytes":bytes.len(),"sourceUnchanged":true,"width":canvas_w,"height":canvas_h,"frames":6,"defaultParticipates":false,"derivation":"nearest-neighbor 192x144, six full SOURCE frames translated on 208x152 transparent canvas; separate poster; metadata omitted for this synthetic scene; savings measured only against derived APNG"})
    );
    Ok(())
}
