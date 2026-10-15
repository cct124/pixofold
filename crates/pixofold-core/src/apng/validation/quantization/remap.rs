//! 固定RGBA色表内的alpha约束最近色检索；不改变色表或跨像素扩散alpha误差。
use super::*;
use imagequant::RGBA;
use score::{Metric, distance};

/// 修正次数按原始子帧像素计，不含验证器第二轮；失败像素保留供审阅但禁止接纳。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub corrected_pixels: u64,
    pub unrepresentable_pixels: u64,
    pub cache_hits: u64,
}
const CACHE_SIZE: usize = 4096;
#[derive(Clone, Copy)]
struct Entry {
    key: u32,
    index: u8,
}
pub(super) struct Mapper {
    valid: Vec<Vec<u8>>,
    palette: Vec<[f32; 4]>,
    alphas: Vec<u8>,
    metric: Metric,
    cache: Vec<Option<Entry>>,
    max_alpha_error: u8,
    pub stats: Stats,
}
impl Mapper {
    pub fn new(
        palette: &[RGBA],
        gamma: f64,
        max_alpha_error: u8,
        budget: &mut Budget,
    ) -> Result<Self> {
        budget.search(palette.len() * 256)?;
        let metric = Metric::new(gamma);
        let valid = (0..=255)
            .map(|a| {
                palette
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| allowed(a, p.a, max_alpha_error).then_some(i as u8))
                    .collect()
            })
            .collect();
        Ok(Self {
            valid,
            palette: palette.iter().map(|p| metric.pixel(*p)).collect(),
            alphas: palette.iter().map(|p| p.a).collect(),
            metric,
            cache: vec![None; CACHE_SIZE],
            max_alpha_error,
            stats: Stats::default(),
        })
    }
    pub fn apply(
        &mut self,
        pixels: &[RGBA],
        indices: &mut [u8],
        guard: &Guard<'_>,
        budget: &mut Budget,
    ) -> Result<()> {
        require(pixels.len() == indices.len(), "pixel_count")?;
        budget.work(pixels.len() as u64)?;
        for (n, (pixel, index)) in pixels.iter().zip(indices).enumerate() {
            if n.is_multiple_of(1024) {
                guard.check()?;
            }
            let alpha = *self
                .alphas
                .get(*index as usize)
                .ok_or(Error("palette_index"))?;
            if allowed(pixel.a, alpha, self.max_alpha_error) {
                continue;
            }
            let choices = &self.valid[pixel.a as usize];
            if choices.is_empty() {
                self.stats.unrepresentable_pixels += 1;
                continue;
            }
            let key = u32::from_le_bytes([pixel.r, pixel.g, pixel.b, pixel.a]);
            let slot = key.wrapping_mul(2654435761) as usize % CACHE_SIZE;
            let corrected = if let Some(entry) = self.cache[slot].filter(|e| e.key == key) {
                self.stats.cache_hits += 1;
                entry.index
            } else {
                budget.search(choices.len())?;
                let source = self.metric.pixel(*pixel);
                let mut best = choices[0];
                let mut error = f32::INFINITY;
                for candidate in choices {
                    let current = distance(source, self.palette[*candidate as usize]);
                    if current < error {
                        error = current;
                        best = *candidate;
                    }
                }
                self.cache[slot] = Some(Entry { key, index: best });
                best
            };
            *index = corrected;
            self.stats.corrected_pixels += 1;
        }
        guard.check()
    }
}
fn allowed(source: u8, output: u8, max_error: u8) -> bool {
    match source {
        0 => output == 0,
        255 => output == 255,
        _ => output > 0 && output < 255 && source.abs_diff(output) <= max_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_is_bounded_and_actual_search_work_is_enforced() {
        let source = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/apng/default-in.png"),
        )
        .unwrap();
        let token = CancellationToken::default();
        let limits = Limits::default();
        let guard = Guard::new(&limits, &token);
        let parsed = structure::parse(&source, &limits, &guard).unwrap();
        let mut budget = Budget::new(
            &parsed,
            source.len(),
            Options {
                max_palette_searches: 4 * 256 + 2,
                ..Options::default()
            },
        )
        .unwrap();
        let palette = [
            RGBA::new(255, 0, 0, 0),
            RGBA::new(255, 0, 0, 128),
            RGBA::new(0, 0, 255, 128),
            RGBA::new(0, 0, 255, 255),
        ];
        let mut mapper = Mapper::new(&palette, 0.45455, 8, &mut budget).unwrap();
        let pixels = vec![RGBA::new(255, 0, 0, 128); 10];
        let mut indices = vec![0; 10];
        mapper
            .apply(&pixels, &mut indices, &guard, &mut budget)
            .unwrap();
        assert_eq!(indices, vec![1; 10]);
        assert_eq!(mapper.stats.cache_hits, 9);
        assert_eq!(budget.palette_searches, 1026);
        assert_eq!(
            mapper
                .apply(&[RGBA::new(0, 0, 255, 128)], &mut [0], &guard, &mut budget)
                .unwrap_err()
                .0,
            "palette_search_budget"
        );
        token.cancel();
        assert_eq!(
            mapper
                .apply(&pixels, &mut indices, &guard, &mut budget)
                .unwrap_err()
                .0,
            "cancelled"
        );
    }
    #[test]
    fn alpha_endpoints_and_partial_range_are_never_relaxed() {
        for source in 0..=255 {
            for output in 0..=255 {
                let acceptable = allowed(source, output, 8);
                if acceptable {
                    assert!(source.abs_diff(output) <= 8);
                    assert_eq!(source == 0, output == 0);
                    assert_eq!(source == 255, output == 255);
                }
                assert_eq!(allowed(source, output, 0), source == output);
            }
        }
    }
}
