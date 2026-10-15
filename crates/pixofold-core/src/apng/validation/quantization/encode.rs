//! 两遍逐帧量化，共享冻结色表；按原chunk位置重建，保留非色表元数据的动画锚点。
use super::*;
use imagequant::{ControlFlow, Histogram, RGBA};
use std::io::Write;

pub(super) struct Encoded {
    pub bytes: Vec<u8>,
    pub palette_entries: usize,
    pub histogram_samples: usize,
    pub minimum_remapping_quality: u8,
    pub remapping_stats: remap::Stats,
    pub alpha_groups: usize,
    pub representative_samples: usize,
}

pub(super) fn run(
    bytes: &[u8],
    p: &Parsed<'_>,
    gamma: f64,
    options: Options,
    guard: &Guard<'_>,
    budget: &mut Budget,
    on_stage: &mut impl FnMut(Stage),
) -> Result<Encoded> {
    let mut attr = imagequant::new();
    attr.set_speed(4).map_err(|e| quant_error(e, guard))?;
    attr.set_quality(0, options.quality)
        .map_err(|e| quant_error(e, guard))?;
    attr.set_progress_callback(progress(guard));
    let default = poster(p);
    let image_count = p.frames.len() + usize::from(!p.default_participates);
    require(
        image_count <= options.max_histogram_samples,
        "histogram_budget",
    )?;
    let mut hist = Histogram::new(&attr);
    let mut grouped = (options.remapping == Remapping::AlphaPalette)
        .then(|| palette::Builder::new(options.quality));
    let mut samples = 0;
    on_stage(Stage::Histogram);
    // 每幅图公平分配上限，按位置分层采样；小帧剩余名额不转给其他帧。
    // 采样可能漏掉稀有颜色，因此接纳必须由全部像素重映射及独立比较决定。
    let frames = (!p.default_participates)
        .then_some(&default)
        .into_iter()
        .chain(&p.frames);
    for (number, frame) in frames.enumerate() {
        guard.check()?;
        let pixels = budget.read(p, frame, guard)?;
        if let Some(builder) = &mut grouped {
            builder.observe(&pixels, guard, budget)?;
        }
        let quota = options.max_histogram_samples / image_count
            + usize::from(number < options.max_histogram_samples % image_count);
        let n = quota.min(pixels.len());
        let selected: Vec<_> = (0..n)
            .map(|i| {
                let at = ((2 * i as u64 + 1) * pixels.len() as u64 / (2 * n as u64)) as usize;
                rgba(pixels[at])
            })
            .collect();
        budget.work(n as u64)?;
        if let Some(builder) = &mut grouped {
            builder.samples(&selected);
        } else {
            let mut image = attr
                .new_image_borrowed(&selected, n, 1, gamma)
                .map_err(|e| quant_error(e, guard))?;
            hist.add_image(&attr, &mut image)
                .map_err(|e| quant_error(e, guard))?;
        }
        samples += n;
    }
    on_stage(Stage::Quantizing);
    guard.check()?;
    let (mut alpha_groups, mut representative_samples) = (0, 0);
    if let Some(builder) = grouped {
        let built = builder.finish(&attr, gamma, guard, budget)?;
        alpha_groups = built.groups;
        representative_samples = built.representatives;
        for color in built.colors {
            hist.add_fixed_color(color, gamma)
                .map_err(|e| quant_error(e, guard))?;
        }
        // 这些已经是最终色表；固定色专用直方图没有可再次聚类的权重。
        // 目标100让库直接冻结它们，实际质量仍由逐帧映射/重解码报告决定。
        attr.set_quality(0, 100)
            .map_err(|e| quant_error(e, guard))?;
    }
    let mut result = hist.quantize(&attr).map_err(|e| quant_error(e, guard))?;
    drop(hist);
    result
        .set_output_gamma(gamma)
        .map_err(|e| quant_error(e, guard))?;
    result
        .set_dithering_level(if options.dithering { 1.0 } else { 0.0 })
        .map_err(|e| quant_error(e, guard))?;
    result.set_progress_callback(progress(guard));
    let palette = result.palette().to_vec();
    require(!palette.is_empty() && palette.len() <= 256, "palette")?;
    let colors: Vec<u8> = palette.iter().flat_map(|c| [c.r, c.g, c.b]).collect();
    let alpha: Vec<u8> = palette.iter().map(|c| c.a).collect();
    let mut mapper = if options.remapping != Remapping::Standard {
        let max_alpha_error = if options.quality == 100 {
            0
        } else if options.remapping == Remapping::AlphaPalette {
            4
        } else {
            8
        };
        Some(remap::Mapper::new(
            &palette,
            gamma,
            max_alpha_error,
            budget,
        )?)
    } else {
        None
    };
    let mut output = structure::SIGNATURE.to_vec();
    let mut minimum = 100;
    let (mut sequence, mut frame_count, mut default_written, mut last_frame) =
        (0u32, 0usize, false, None);
    on_stage(Stage::Remapping);
    guard.check()?;
    for (kind, data) in chunks(bytes) {
        guard.check()?;
        match kind {
            b"IHDR" => {
                let mut header = data.to_vec();
                header[8] = 8;
                header[9] = 3;
                append(&mut output, b"IHDR", &header, options)?;
            }
            b"PLTE" | b"tRNS" => {}
            b"fcTL" => {
                let mut control = data.to_vec();
                control[..4].copy_from_slice(&sequence.to_be_bytes());
                sequence += 1;
                append(&mut output, b"fcTL", &control, options)?;
                frame_count += 1;
            }
            b"IDAT" | b"fdAT" => {
                let is_default = kind == b"IDAT";
                if is_default && default_written || !is_default && last_frame == Some(frame_count) {
                    continue;
                }
                let frame = if is_default {
                    &default
                } else {
                    &p.frames[frame_count - 1]
                };
                let pixels: Vec<_> = budget
                    .read(p, frame, guard)?
                    .into_iter()
                    .map(rgba)
                    .collect();
                budget.work(pixels.len() as u64)?;
                let mut image = attr
                    .new_image_borrowed(
                        &pixels,
                        frame.control.width as usize,
                        frame.control.height as usize,
                        gamma,
                    )
                    .map_err(|e| quant_error(e, guard))?;
                let (mapped_palette, mut indices) = result
                    .remapped(&mut image)
                    .map_err(|e| quant_error(e, guard))?;
                guard.check()?;
                require(
                    mapped_palette == palette && indices.len() == pixels.len(),
                    "palette_changed",
                )?;
                minimum = minimum.min(result.remapping_quality().ok_or(Error("missing_quality"))?);
                drop(image);
                if let Some(mapper) = &mut mapper {
                    mapper.apply(&pixels, &mut indices, guard, budget)?;
                }
                drop(pixels);
                let encoded =
                    static_png(&frame.control, &colors, &alpha, &indices, options, guard)?;
                if is_default {
                    append(&mut output, b"PLTE", &colors, options)?;
                    if alpha.iter().any(|a| *a != 255) {
                        append(&mut output, b"tRNS", &alpha, options)?;
                    }
                    default_written = true;
                } else {
                    last_frame = Some(frame_count);
                }
                // static_png由固定编码器生成；只抽取IDAT，不引入任何编码器元数据。
                for (k, compressed) in chunks(&encoded) {
                    if k != b"IDAT" {
                        continue;
                    }
                    if is_default {
                        append(&mut output, b"IDAT", compressed, options)?;
                    } else {
                        let mut data = sequence.to_be_bytes().to_vec();
                        sequence += 1;
                        data.extend_from_slice(compressed);
                        append(&mut output, b"fdAT", &data, options)?;
                    }
                }
            }
            _ => append(
                &mut output,
                kind.try_into().map_err(|_| Error("chunk_type"))?,
                data,
                options,
            )?,
        }
    }
    guard.check()?;
    Ok(Encoded {
        bytes: output,
        palette_entries: palette.len(),
        histogram_samples: samples,
        minimum_remapping_quality: minimum,
        remapping_stats: mapper.map(|m| m.stats).unwrap_or_default(),
        alpha_groups,
        representative_samples,
    })
}
fn progress(guard: &Guard<'_>) -> impl Fn(f32) -> ControlFlow + Send + Sync + 'static {
    let token = guard.cancel.clone();
    let deadline = guard.deadline;
    move |_| {
        if token.is_cancelled() || Instant::now() >= deadline {
            ControlFlow::Break
        } else {
            ControlFlow::Continue
        }
    }
}
fn rgba(p: [u16; 4]) -> RGBA {
    RGBA::new(
        (p[0] / 257) as u8,
        (p[1] / 257) as u8,
        (p[2] / 257) as u8,
        (p[3] / 257) as u8,
    )
}
fn append(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8], options: Options) -> Result<()> {
    require(
        output.len() + data.len() + 12 <= options.limits.max_candidate_bytes,
        "candidate_budget",
    )?;
    structure::chunk(output, kind, data);
    Ok(())
}
struct Bounded {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            self.exceeded = true;
            return Err(std::io::Error::other("candidate budget"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn static_png(
    c: &Control,
    colors: &[u8],
    alpha: &[u8],
    indices: &[u8],
    options: Options,
    guard: &Guard<'_>,
) -> Result<Vec<u8>> {
    let mut buffer = Bounded {
        bytes: Vec::new(),
        limit: options.limits.max_candidate_bytes,
        exceeded: false,
    };
    let encoded = (|| {
        let mut encoder = png::Encoder::new(&mut buffer, c.width, c.height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(colors);
        if alpha.iter().any(|a| *a != 255) {
            encoder.set_trns(alpha);
        }
        let mut writer = encoder.write_header()?;
        writer.write_image_data(indices)?;
        writer.finish()
    })();
    guard.check()?;
    encoded.map_err(|_| {
        Error(if buffer.exceeded {
            "candidate_budget"
        } else {
            "encode"
        })
    })?;
    Ok(buffer.bytes)
}
