use super::*;
use std::borrow::Cow;

fn gray(value: u8, looping: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder =
            gif::Encoder::new(&mut bytes, 1, 1, &[value, value, value, 255, 255, 255]).unwrap();
        if looping {
            encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        }
        encoder
            .write_frame(&gif::Frame {
                width: 1,
                height: 1,
                delay: 10,
                buffer: Cow::Borrowed(&[0]),
                ..Default::default()
            })
            .unwrap();
    }
    bytes
}

#[test]
fn selection_distinguishes_quality_gain_lossless_and_hard_failure() {
    let source = gray(10, false);
    let limits = GifLimits::default();
    let cancel = CancellationToken::default();
    let before = validate_gif(&source, limits, &cancel).unwrap();
    let preparation = Preparation {
        limits,
        cancel: &cancel,
        deadline: Instant::now() + limits.preparation_timeout,
    };
    let mode = GifMode::Lossy {
        quality: QualityValue::new(80).unwrap(),
    };
    for (candidate, reason) in [
        (gray(90, false), GifFallbackReason::QualityBelowTarget),
        (gray(11, false), GifFallbackReason::InsufficientGain),
    ] {
        let selected = select(&source, &before, mode, &preparation, |strength, _| {
            Ok(if strength.is_none() {
                source.clone()
            } else {
                candidate.clone()
            })
        })
        .unwrap();
        assert_eq!(selected.bytes, source);
        assert!(
            matches!(selected.processing, GifProcessing::LosslessFallback {reason:r,..} if r==reason)
        );
        assert!(!selected.quality.unwrap().lossless_equivalent);
    }
    let selected = select(&source, &before, mode, &preparation, |_, _| {
        Ok(source.clone())
    })
    .unwrap();
    assert_eq!(selected.processing, GifProcessing::Lossless);
    assert!(selected.quality.unwrap().lossless_equivalent);
    assert!(matches!(
        select(&source, &before, mode, &preparation, |strength, _| Ok(
            if strength.is_none() {
                source.clone()
            } else {
                gray(10, true)
            }
        )),
        Err(GifError::ValidationFailed)
    ));
    // 无损基准本身失效不能掩盖为回退成功。
    assert!(matches!(
        select(&source, &before, mode, &preparation, |_, _| Ok(gray(
            11, false
        ))),
        Err(GifError::ValidationFailed)
    ));
}

#[test]
fn highest_quality_never_calls_lossy_and_cancellation_stops_between_candidates() {
    let source = gray(10, false);
    let limits = GifLimits::default();
    let cancel = CancellationToken::default();
    let before = validate_gif(&source, limits, &cancel).unwrap();
    let preparation = Preparation {
        limits,
        cancel: &cancel,
        deadline: Instant::now() + limits.preparation_timeout,
    };
    let mut calls = 0;
    let selected = select(
        &source,
        &before,
        GifMode::Lossy {
            quality: QualityValue::new(100).unwrap(),
        },
        &preparation,
        |strength, _| {
            assert_eq!(strength, None);
            calls += 1;
            Ok(source.clone())
        },
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(selected.processing, GifProcessing::Lossless);
    assert!(selected.quality.is_none());
    calls = 0;
    assert!(matches!(
        select(
            &source,
            &before,
            GifMode::Lossy {
                quality: QualityValue::default()
            },
            &preparation,
            |_, _| {
                calls += 1;
                cancel.cancel();
                Ok(source.clone())
            }
        ),
        Err(GifError::Cancelled)
    ));
    assert_eq!(calls, 1);
}

#[test]
fn preparation_uses_one_absolute_deadline_and_tightens_stage_limits() {
    let limits = GifLimits::default();
    let cancel = CancellationToken::default();
    let preparation = Preparation {
        limits,
        cancel: &cancel,
        deadline: Instant::now() + Duration::from_secs(1),
    };
    let effective = preparation.limits().unwrap();
    assert!(effective.process_timeout <= Duration::from_secs(1));
    assert!(effective.validation_timeout <= Duration::from_secs(1));
    let expired = Preparation {
        deadline: Instant::now() - Duration::from_secs(1),
        ..preparation
    };
    assert!(matches!(expired.limits(), Err(GifError::Timeout)));
}
