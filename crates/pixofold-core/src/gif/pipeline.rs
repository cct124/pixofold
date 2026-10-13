//! 单文件GIF候选选择与安全输出；无损/有损编码顺序执行，共用准备期限和既有提交层。
use super::*;

struct Preparation<'a> {
    limits: GifLimits,
    deadline: Instant,
    cancel: &'a CancellationToken,
}
impl Preparation<'_> {
    fn limits(&self) -> Result<GifLimits, GifError> {
        self.cancel.check()?;
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining < Duration::from_millis(1) {
            return Err(GifError::Timeout);
        }
        Ok(GifLimits {
            process_timeout: self.limits.process_timeout.min(remaining),
            validation_timeout: self.limits.validation_timeout.min(remaining),
            ..self.limits
        })
    }
}

struct Selected {
    bytes: Vec<u8>,
    processing: GifProcessing,
    quality: Option<GifComparison>,
}

fn select(
    source: &[u8],
    before: &GifPlayback,
    mode: GifMode,
    preparation: &Preparation<'_>,
    mut encode: impl FnMut(Option<u16>, GifLimits) -> Result<Vec<u8>, GifError>,
) -> Result<Selected, GifError> {
    let lossless = encode(None, preparation.limits()?)?;
    let GifMode::Lossy { quality } = mode else {
        return Ok(Selected {
            bytes: lossless,
            processing: GifProcessing::Lossless,
            quality: None,
        });
    };
    let parameters = GifQualityMapping::new(quality);
    if parameters.native_lossiness == 0 {
        return Ok(Selected {
            bytes: lossless,
            processing: GifProcessing::Lossless,
            quality: None,
        });
    }
    // 有损选择需要已验证的无损基准；纯无损/q100在落盘后统一复验，避免重复全动画解码。
    let after = validate_gif(&lossless, preparation.limits()?, preparation.cancel)?;
    if !before.equivalent(&after) {
        return Err(GifError::ValidationFailed);
    }
    let lossy = encode(Some(parameters.native_lossiness), preparation.limits()?)?;
    let comparison = comparison::compare_with_resident(
        source,
        &lossy,
        preparation.limits()?,
        preparation.cancel,
        lossless.capacity() as u64,
    )?;
    let baseline = source.len().min(lossless.len()) as u64;
    let processing = quality::decide(&comparison, baseline, lossy.len() as u64, parameters)?;
    let bytes = match processing {
        GifProcessing::Lossy { .. } => lossy,
        GifProcessing::Lossless if lossy.len() < lossless.len() => lossy,
        _ => lossless,
    };
    Ok(Selected {
        bytes,
        processing,
        quality: Some(comparison),
    })
}

pub(super) fn run(
    request: &GifRequest,
    engine: &GifEngine,
    cancel: &CancellationToken,
    mut on_stage: impl FnMut(ProcessingStage),
) -> Result<GifReport, GifError> {
    let started = Instant::now();
    request.limits.validate()?;
    let preparation = Preparation {
        limits: request.limits,
        deadline: started + request.limits.preparation_timeout,
        cancel,
    };
    preparation.limits()?;
    on_stage(ProcessingStage::Reading);
    let mut source = Source::read(&request.source, preparation.limits()?.resources)?;
    if !source
        .path
        .extension()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("gif"))
    {
        return Err(GifError::UnsupportedGif);
    }
    source.use_gif_backup_suffix();
    let image = inspect_structure(&source.bytes, preparation.limits()?, cancel)?;
    let before = validate_gif(&source.bytes, preparation.limits()?, cancel)?;
    let destination = Destination::plan(&source, &request.output)?;
    on_stage(ProcessingStage::Optimizing);
    let selected = select(
        &source.bytes,
        &before,
        request.mode,
        &preparation,
        |strength, limits| match strength {
            None => engine.optimize(&source.bytes, limits, cancel),
            Some(_) => engine.run(&source.bytes, limits, cancel, strength),
        },
    )?;
    preparation.limits()?;
    let mut temp = destination.stage()?;
    let validated = (|| {
        output::write_candidate(&mut temp, &selected.bytes, &source)?;
        on_stage(ProcessingStage::Validating);
        let stored = output::read_candidate(&temp, preparation.limits()?.resources)?;
        if stored != selected.bytes {
            return Err(GifError::ValidationFailed);
        }
        if matches!(selected.processing, GifProcessing::Lossy { .. }) {
            let comparison = comparison::compare_with_resident(
                &source.bytes,
                &stored,
                preparation.limits()?,
                cancel,
                selected.bytes.capacity() as u64,
            )?;
            if !meets_quality(&comparison) || comparison.lossless_equivalent {
                return Err(GifError::ValidationFailed);
            }
        } else if !before.equivalent(&validate_gif(&stored, preparation.limits()?, cancel)?) {
            return Err(GifError::ValidationFailed);
        }
        source.verify_unchanged(preparation.limits()?.resources)?;
        preparation.limits()?;
        Ok(stored)
    })();
    drop(selected.bytes);
    let stored = match validated {
        Ok(stored) => stored,
        Err(original) => {
            let temporary = temp.path().to_owned();
            return Err(match temp.close() {
                Ok(()) => original,
                Err(source) => GifError::Cleanup {
                    original: Some(Box::new(original)),
                    source,
                    temporary,
                },
            });
        }
    };
    let input_bytes = ByteCount(source.bytes.len() as u64);
    let (outcome, output_bytes) = if stored.len() as u64 >= input_bytes.0 {
        output::discard_no_gain(temp)?;
        (ProcessingOutcome::NoGain, input_bytes)
    } else {
        let size = ByteCount(stored.len() as u64);
        // 从此进入已有输出事务：备份与不可逆提交遵循共享取消契约，不在提交后改报超时。
        (
            output::commit(
                temp,
                &stored,
                &destination,
                source,
                request.limits.resources,
                cancel,
                &mut on_stage,
            )?,
            size,
        )
    };
    Ok(GifReport {
        image,
        input_bytes,
        output_bytes,
        elapsed: started.elapsed(),
        outcome,
        requested_mode: request.mode,
        processing: selected.processing,
        quality: selected.quality,
    })
}

#[cfg(test)]
mod tests;
