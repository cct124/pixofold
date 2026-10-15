//! 对实际alpha分组分配全局256色预算。RGB分组量化，alpha取观测范围内中心；
//! 每组至少一色，透明/不透明独立，独立默认图和全部帧共享同一最终色表。
use super::*;
use imagequant::{Histogram, RGBA};

#[derive(Default)]
struct Group {
    count: u64,
    alpha_sum: u64,
    rgb_sum: [u64; 3],
    min: u8,
    max: u8,
    samples: Vec<RGBA>,
}
pub(super) struct Builder {
    groups: Vec<Group>,
    exact: bool,
}
pub(super) struct Palette {
    pub colors: Vec<RGBA>,
    pub groups: usize,
    pub representatives: usize,
}
impl Builder {
    pub fn new(quality: u8) -> Self {
        Self {
            groups: (0..256).map(|_| Group::default()).collect(),
            exact: quality == 100,
        }
    }
    fn group(&self, alpha: u8) -> usize {
        if self.exact {
            return alpha as usize;
        }
        match alpha {
            0 => 0,
            255 => 255,
            _ => usize::from((alpha - 1) / 8) + 1,
        }
    }
    pub fn observe(
        &mut self,
        pixels: &[[u16; 4]],
        guard: &Guard<'_>,
        budget: &mut Budget,
    ) -> Result<()> {
        budget.work(pixels.len() as u64)?;
        for (n, pixel) in pixels.iter().enumerate() {
            if n.is_multiple_of(4096) {
                guard.check()?;
            }
            let alpha = (pixel[3] / 257) as u8;
            let at = self.group(alpha);
            let group = &mut self.groups[at];
            if group.count == 0 {
                group.min = alpha;
                group.max = alpha;
            } else {
                group.min = group.min.min(alpha);
                group.max = group.max.max(alpha);
            }
            group.count += 1;
            group.alpha_sum += u64::from(alpha);
            for (sum, c) in group.rgb_sum.iter_mut().zip(pixel) {
                *sum += u64::from(c / 257);
            }
        }
        Ok(())
    }
    pub fn samples(&mut self, pixels: &[RGBA]) {
        for pixel in pixels {
            let group = self.group(pixel.a);
            self.groups[group].samples.push(*pixel);
        }
    }
    pub fn finish(
        mut self,
        attr: &imagequant::Attributes,
        gamma: f64,
        guard: &Guard<'_>,
        budget: &mut Budget,
    ) -> Result<Palette> {
        self.groups.retain(|g| g.count > 0);
        let mut limits = Vec::new();
        let mut weights = Vec::new();
        let mut representatives = 0;
        for group in &mut self.groups {
            guard.check()?;
            // 稀有alpha可能未被位置采样命中，固定全量统计的代表色补一项，不漏掉该组。
            if group.samples.is_empty() {
                representatives += 1;
                group.samples.push(RGBA::new(
                    (group.rgb_sum[0] / group.count) as u8,
                    (group.rgb_sum[1] / group.count) as u8,
                    (group.rgb_sum[2] / group.count) as u8,
                    (group.alpha_sum / group.count) as u8,
                ));
            }
            for p in &mut group.samples {
                p.a = 255;
            }
            let mut colors: Vec<_> = group.samples.iter().map(|p| [p.r, p.g, p.b]).collect();
            colors.sort_unstable();
            colors.dedup();
            let limit = if group.max == 0 {
                1
            } else {
                colors.len().min(256)
            };
            limits.push(limit);
            // 按全量面积、采样颜色数与可见alpha分配；稀有组保底一色，
            // 避免少量抗锯齿边缘占满色表而挤掉主体的不透明颜色。
            weights.push(
                (limit as f64 * group.count as f64).sqrt()
                    * (f64::from(center(group)) / 255.0).max(0.125),
            );
        }
        let mut sizes = vec![1usize; self.groups.len()];
        for _ in sizes.len()..256 {
            let next = (0..sizes.len())
                .filter(|i| sizes[*i] < limits[*i])
                .max_by(|a, b| {
                    let utility = |i: usize| weights[i] / (sizes[i] + 1) as f64;
                    utility(*a).total_cmp(&utility(*b)).then_with(|| b.cmp(a))
                });
            let Some(next) = next else {
                break;
            };
            sizes[next] += 1;
        }
        let mut palette = Vec::new();
        for (group, size) in self.groups.iter().zip(sizes) {
            guard.check()?;
            let alpha = center(group);
            budget.work(group.samples.len() as u64)?;
            if alpha == 0 {
                palette.push(RGBA::new(0, 0, 0, 0));
                continue;
            }
            if size == 1 {
                palette.push(RGBA::new(
                    (group.rgb_sum[0] / group.count) as u8,
                    (group.rgb_sum[1] / group.count) as u8,
                    (group.rgb_sum[2] / group.count) as u8,
                    alpha,
                ));
                continue;
            }
            let mut attr = attr.clone();
            attr.set_max_colors(size as u32)
                .map_err(|e| quant_error(e, guard))?;
            let mut hist = Histogram::new(&attr);
            let mut image = attr
                .new_image_borrowed(&group.samples, group.samples.len(), 1, gamma)
                .map_err(|e| quant_error(e, guard))?;
            hist.add_image(&attr, &mut image)
                .map_err(|e| quant_error(e, guard))?;
            let mut result = hist.quantize(&attr).map_err(|e| quant_error(e, guard))?;
            result
                .set_output_gamma(gamma)
                .map_err(|e| quant_error(e, guard))?;
            for p in result.palette() {
                palette.push(RGBA::new(p.r, p.g, p.b, alpha));
            }
        }
        require(!palette.is_empty() && palette.len() <= 256, "palette")?;
        guard.check()?;
        Ok(Palette {
            colors: palette,
            groups: self.groups.len(),
            representatives,
        })
    }
}
fn center(group: &Group) -> u8 {
    if group.min == group.max {
        return group.min;
    }
    let mean = ((group.alpha_sum + group.count / 2) / group.count) as u8;
    // 整组alpha跨度≤7，保留≤4的原始差，为多次OVER合成的8级检查留余量。
    mean.clamp(group.max.saturating_sub(4), group.min.saturating_add(4))
}
