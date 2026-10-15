//! 源/候选各自双解码并独立合成。指标来自实际字节，覆盖默认图、两轮及循环边界。
use super::*;

/// 误差单位均为8位编码值；不宣称感知或显示器色彩管理。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameQuality {
    /// None表示独立默认图；动画轮次从0开始。
    pub play: Option<u8>,
    pub frame: Option<usize>,
    pub rmse: f64,
    pub max_channel_error: f64,
    pub temporal_rmse: Option<f64>,
}
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QualityMetrics {
    /// 所有显示快照等权，不引入播放器的零延时钳制。逐帧最坏值另行检查。
    pub rmse: f64,
    pub worst_frame_rmse: f64,
    /// 相邻显示快照的误差差值RMS；相同误差的重复帧不稀释分母。
    pub temporal_error_rmse: f64,
    pub worst_temporal_rmse: f64,
    pub max_channel_error: f64,
    pub frames: Vec<FrameQuality>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    /// 按实际重解码子帧像素计算；独立默认图在首项，动画只计第一轮。
    pub pixel_qualities: Vec<u8>,
    pub minimum_pixel_quality: u8,
    pub width: u32,
    pub height: u32,
    pub frames: usize,
    pub plays: u32,
    pub default_participates: bool,
    pub checked_plays: u8,
    pub black: QualityMetrics,
    pub white: QualityMetrics,
    /// 原始子帧及显示画布的观测次数，重复轮次也计数。
    pub alpha_class_changes: u64,
    pub max_alpha_error: f64,
    pub raw_alpha_class_changes: u64,
    pub max_raw_alpha_error: f64,
}
#[derive(Default)]
struct Alpha {
    changes: u64,
    max_error: f64,
}
impl Alpha {
    fn add(&mut self, a: u16, b: u16) {
        let class = |v| match v {
            0 => 0,
            65535 => 2,
            _ => 1,
        };
        self.changes += u64::from(class(a) != class(b));
        self.max_error = self.max_error.max(f64::from(a.abs_diff(b)) / 257.0);
    }
}
#[derive(Default)]
struct Measure {
    quality: QualityMetrics,
    sum: f64,
    channels: u64,
    temporal_sum: f64,
    temporal_channels: u64,
    previous: Vec<[f32; 3]>,
    has_previous: bool,
}
impl Measure {
    fn new(pixels: usize) -> Self {
        Self {
            previous: vec![[0.0; 3]; pixels],
            ..Self::default()
        }
    }
    fn observe(
        &mut self,
        a: &[[u16; 4]],
        b: &[[u16; 4]],
        background: f64,
        play: Option<u8>,
        frame: Option<usize>,
        guard: &Guard<'_>,
    ) -> Result<()> {
        let (mut sum, mut maximum, mut temporal) = (0.0f64, 0.0f64, 0.0f64);
        for (i, (source, candidate)) in a.iter().zip(b).enumerate() {
            if i.is_multiple_of(4096) {
                guard.check()?;
            }
            for c in 0..3 {
                let flatten = |p: &[u16; 4]| {
                    (f64::from(p[c]) * f64::from(p[3]) / 65535.0
                        + background * (65535.0 - f64::from(p[3])))
                        / 257.0
                };
                // 与存储精度相同，重复误差的差值必须严格为零，不能稀释时域分母。
                let error = f64::from((flatten(candidate) - flatten(source)) as f32);
                sum += error * error;
                maximum = maximum.max(error.abs());
                if self.has_previous {
                    let delta = error - f64::from(self.previous[i][c]);
                    temporal += delta * delta;
                }
                self.previous[i][c] = error as f32;
            }
        }
        let count = (a.len() * 3) as u64;
        let rmse = (sum / count as f64).sqrt();
        let temporal_rmse = self.has_previous.then(|| (temporal / count as f64).sqrt());
        self.sum += sum;
        self.channels += count;
        self.quality.worst_frame_rmse = self.quality.worst_frame_rmse.max(rmse);
        self.quality.max_channel_error = self.quality.max_channel_error.max(maximum);
        if self.has_previous && temporal > 1e-12 {
            self.temporal_sum += temporal;
            self.temporal_channels += count;
            self.quality.worst_temporal_rmse = self
                .quality
                .worst_temporal_rmse
                .max((temporal / count as f64).sqrt());
        }
        self.quality.frames.push(FrameQuality {
            play,
            frame,
            rmse,
            max_channel_error: maximum,
            temporal_rmse,
        });
        // 独立默认图与动画没有播放相邻关系；第二轮首帧保留上一轮最后显示误差。
        self.has_previous = play.is_some();
        Ok(())
    }
    fn finish(mut self) -> QualityMetrics {
        self.quality.rmse = (self.sum / self.channels.max(1) as f64).sqrt();
        self.quality.temporal_error_rmse =
            (self.temporal_sum / self.temporal_channels.max(1) as f64).sqrt();
        self.quality
    }
}
pub(super) fn run(
    source: &[u8],
    candidate: &[u8],
    limits: &Limits,
    guard: &Guard<'_>,
    budget: &mut Budget,
) -> Result<Comparison> {
    require(
        candidate.len() <= limits.max_candidate_bytes,
        "candidate_budget",
    )?;
    let a = structure::parse(source, limits, guard)?;
    let b = structure::parse(
        candidate,
        &Limits {
            max_input_bytes: limits.max_candidate_bytes,
            ..*limits
        },
        guard,
    )?;
    let gamma = supported(&a, source)?;
    supported(&b, candidate)?;
    require(b.ihdr[8] == 8 && b.ihdr[9] == 3, "candidate_representation")?;
    require(
        a.ihdr[..8] == b.ihdr[..8]
            && a.ihdr[10..] == b.ihdr[10..]
            && a.plays == b.plays
            && a.default_participates == b.default_participates
            && a.frames.len() == b.frames.len()
            && a.frames
                .iter()
                .zip(&b.frames)
                .all(|(a, b)| a.control == b.control),
        "animation_changed",
    )?;
    require(metadata(source) == metadata(candidate), "metadata_changed")?;
    let (width, height) = (structure::u32_at(a.ihdr, 0), structure::u32_at(a.ihdr, 4));
    let count = width as usize * height as usize;
    let mut black = Measure::new(count);
    let mut white = Measure::new(count);
    let mut alpha = Alpha::default();
    let mut raw = Alpha::default();
    let metric = score::Metric::new(gamma);
    let mut pixel_qualities = Vec::new();
    if !a.default_participates {
        let ap = budget.read(&a, &poster(&a), guard)?;
        let bp = budget.read(&b, &poster(&b), guard)?;
        budget.work(count as u64 * 3)?;
        raw_alpha(&ap, &bp, &mut raw, guard)?;
        pixel_qualities.push(pixel_quality(&ap, &bp, &metric, budget, guard)?);
        black.observe(&ap, &bp, 0.0, None, None, guard)?;
        white.observe(&ap, &bp, 1.0, None, None, guard)?;
    }
    let mut ac = vec![[0u16; 4]; count];
    let mut bc = vec![[0u16; 4]; count];
    let checked_plays = if a.plays == 1 { 1 } else { 2 };
    for play in 0..checked_plays {
        guard.check()?;
        budget.work(count as u64 * 2)?;
        ac.fill([0; 4]);
        bc.fill([0; 4]);
        for (number, (af, bf)) in a.frames.iter().zip(&b.frames).enumerate() {
            let ap = budget.read(&a, af, guard)?;
            let bp = budget.read(&b, bf, guard)?;
            budget.work(ap.len() as u64)?;
            raw_alpha(&ap, &bp, &mut raw, guard)?;
            if play == 0 {
                pixel_qualities.push(pixel_quality(&ap, &bp, &metric, budget, guard)?);
            }
            let control = &af.control;
            let previous = if control.dispose == 2 && number > 0 {
                budget.work(count as u64 * 2)?;
                Some((ac.clone(), bc.clone()))
            } else {
                None
            };
            budget.work(ap.len() as u64 * 2)?;
            draw(&mut ac, &ap, control, width, guard)?;
            draw(&mut bc, &bp, control, width, guard)?;
            drop(ap);
            drop(bp);
            budget.work(count as u64 * 3)?;
            raw_alpha(&ac, &bc, &mut alpha, guard)?;
            black.observe(&ac, &bc, 0.0, Some(play), Some(number), guard)?;
            white.observe(&ac, &bc, 1.0, Some(play), Some(number), guard)?;
            if let Some((ap, bp)) = previous {
                ac = ap;
                bc = bp;
            } else if control.dispose != 0 {
                budget.work(u64::from(control.width) * u64::from(control.height) * 2)?;
                for y in control.y..control.y + control.height {
                    guard.check()?;
                    let start = (y * width + control.x) as usize;
                    let end = start + control.width as usize;
                    ac[start..end].fill([0; 4]);
                    bc[start..end].fill([0; 4]);
                }
            }
        }
    }
    guard.check()?;
    Ok(Comparison {
        minimum_pixel_quality: pixel_qualities.iter().copied().min().unwrap_or(0),
        pixel_qualities,
        width,
        height,
        frames: a.frames.len(),
        plays: a.plays,
        default_participates: a.default_participates,
        checked_plays,
        black: black.finish(),
        white: white.finish(),
        alpha_class_changes: alpha.changes + raw.changes,
        max_alpha_error: alpha.max_error.max(raw.max_error),
        raw_alpha_class_changes: raw.changes,
        max_raw_alpha_error: raw.max_error,
    })
}
fn pixel_quality(
    a: &[[u16; 4]],
    b: &[[u16; 4]],
    metric: &score::Metric,
    budget: &mut Budget,
    guard: &Guard<'_>,
) -> Result<u8> {
    budget.work(a.len() as u64)?;
    let mut sum = 0.0;
    for (i, (a, b)) in a.iter().zip(b).enumerate() {
        if i.is_multiple_of(4096) {
            guard.check()?;
        }
        sum += f64::from(score::distance(metric.wide(*a), metric.wide(*b)));
    }
    Ok(score::quality(sum, a.len()))
}
fn draw(
    canvas: &mut [[u16; 4]],
    pixels: &[[u16; 4]],
    c: &Control,
    width: u32,
    guard: &Guard<'_>,
) -> Result<()> {
    for y in 0..c.height {
        guard.check()?;
        for x in 0..c.width {
            let at = ((y + c.y) * width + x + c.x) as usize;
            let p = pixels[(y * c.width + x) as usize];
            canvas[at] = if c.blend == 0 {
                p
            } else {
                decode::over(p, canvas[at])
            };
        }
    }
    Ok(())
}
fn raw_alpha(a: &[[u16; 4]], b: &[[u16; 4]], alpha: &mut Alpha, guard: &Guard<'_>) -> Result<()> {
    require(a.len() == b.len(), "pixel_count")?;
    for (i, (a, b)) in a.iter().zip(b).enumerate() {
        if i.is_multiple_of(4096) {
            guard.check()?;
        }
        alpha.add(a[3], b[3]);
    }
    Ok(())
}
// 排除允许重建的PLTE/tRNS，其他元数据保持内容、相对次序、默认图及帧锚点。
fn metadata(bytes: &[u8]) -> Vec<(&[u8], &[u8], bool, usize)> {
    let (mut data_seen, mut frames) = (false, 0usize);
    chunks(bytes)
        .filter_map(|(kind, data)| match kind {
            b"IHDR" | b"IEND" | b"acTL" | b"PLTE" | b"tRNS" | b"fdAT" => None,
            b"IDAT" => {
                data_seen = true;
                None
            }
            b"fcTL" => {
                frames += 1;
                None
            }
            _ => Some((kind, data, data_seen, frames)),
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_error_frames_do_not_dilute_temporal_measurement() {
        let token = CancellationToken::default();
        let guard = Guard::new(&Limits::default(), &token);
        let source = [[0, 0, 0, 65535]];
        let changed = [[2570, 2570, 2570, 65535]];
        let mut one = Measure::new(1);
        one.observe(&source, &source, 0.0, Some(0), Some(0), &guard)
            .unwrap();
        one.observe(&source, &changed, 0.0, Some(0), Some(1), &guard)
            .unwrap();
        for n in 2..10 {
            one.observe(&source, &changed, 0.0, Some(0), Some(n), &guard)
                .unwrap();
        }
        let result = one.finish();
        assert_eq!(result.temporal_error_rmse, 10.0);
        assert_eq!(result.worst_temporal_rmse, 10.0);
    }
}
