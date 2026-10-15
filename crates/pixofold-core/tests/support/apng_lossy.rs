//! 确定性多色动画：真实RGBA→共享色表路径，不依赖网络或外部原件。
pub fn animation() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 128, 96);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(4, 3).unwrap();
        encoder.set_frame_delay(7, 100).unwrap();
        let mut writer = encoder.write_header().unwrap();
        let mut random = 0x12345678u32;
        for _ in 0..4 {
            let mut pixels = Vec::with_capacity(128 * 96 * 4);
            for _ in 0..128 * 96 {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                let value = (random >> 24) as u8;
                pixels.extend_from_slice(&[
                    value,
                    value.saturating_add(((random >> 16) % 8) as u8),
                    value.saturating_sub(((random >> 8) % 8) as u8),
                    255,
                ]);
            }
            writer.write_image_data(&pixels).unwrap();
        }
        writer.finish().unwrap();
    }
    bytes
}
