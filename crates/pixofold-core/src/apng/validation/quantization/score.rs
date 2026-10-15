//! 实际像素对的质量评分，按imagequant 4.4.1的pal.rs/quant.rs标量公式改编。
//! libimagequant © 2009–2018 Kornel Lesiński, GPL-3.0-or-later；完整COPYRIGHT：
//! tools/apng-lossy-lab/imagequant-COPYRIGHT.txt。本适配使用安全Rust及f64累计，
//! 对实际整数色表重解码后的像素评分，不复用量化前/抖动前的误差。
use imagequant::RGBA;

pub(super) struct Metric {
    gamma: [f32; 256],
}
impl Metric {
    pub fn new(gamma: f64) -> Self {
        Self {
            gamma: std::array::from_fn(|i| (i as f32 / 255.0).powf((0.57 / gamma) as f32)),
        }
    }
    pub fn pixel(&self, p: RGBA) -> [f32; 4] {
        let a = f32::from(p.a) / 255.0;
        [
            a * 0.625,
            self.gamma[p.r as usize] * 0.5 * a,
            self.gamma[p.g as usize] * a,
            self.gamma[p.b as usize] * 0.45 * a,
        ]
    }
    pub fn wide(&self, p: [u16; 4]) -> [f32; 4] {
        self.pixel(RGBA::new(
            (p[0] / 257) as u8,
            (p[1] / 257) as u8,
            (p[2] / 257) as u8,
            (p[3] / 257) as u8,
        ))
    }
}
pub(super) fn distance(a: [f32; 4], b: [f32; 4]) -> f32 {
    let alpha = b[0] - a[0];
    let mut sum = 0.0;
    for c in 1..4 {
        let black = a[c] - b[c];
        let white = black + alpha;
        sum += (black * black).max(white * white);
    }
    sum
}
fn threshold(q: u8) -> f64 {
    if q == 100 {
        return 0.0;
    }
    let q = f64::from(q);
    0.45 * ((0.016 / (0.001 + q) - 0.001).max(0.0)
        + 2.5 / (210.0 + q).powf(1.2) * (100.1 - q) / 100.0)
}
pub(super) fn quality(sum: f64, pixels: usize) -> u8 {
    let mse = sum / pixels as f64;
    (1..=100)
        .rev()
        .find(|q| mse <= threshold(*q) + 0.000001)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measured_quality_agrees_with_fixed_palette_public_api() {
        for gamma in [0.45455, 0.5] {
            let mut attr = imagequant::new();
            attr.set_quality(0, 100).unwrap();
            let mut hist = imagequant::Histogram::new(&attr);
            // 单色固定色表使像素配对唯一，API评分和本地实际像素评分可独立对照。
            let palette = RGBA::new(96, 128, 160, 128);
            hist.add_fixed_color(palette, gamma).unwrap();
            let mut quant = hist.quantize(&attr).unwrap();
            quant.set_output_gamma(gamma).unwrap();
            quant.set_dithering_level(0.0).unwrap();
            for delta in [0, 1, 8, 32, 96] {
                let pixels = vec![RGBA::new(96 + delta, 128, 160, 128); 32];
                let mut image = attr.new_image_borrowed(&pixels, 8, 4, gamma).unwrap();
                let (actual, indices) = quant.remapped(&mut image).unwrap();
                let metric = Metric::new(gamma);
                let sum = pixels
                    .iter()
                    .zip(indices)
                    .map(|(p, i)| {
                        f64::from(distance(metric.pixel(*p), metric.pixel(actual[i as usize])))
                    })
                    .sum();
                // API内部未量化浮点色表与最终8位色表有至多一分的舍入差。
                assert!(
                    quality(sum, pixels.len()).abs_diff(quant.remapping_quality().unwrap()) <= 1
                );
            }
        }
    }
}
