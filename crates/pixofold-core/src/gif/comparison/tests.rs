use super::super::validate_gif;
use super::*;
use crate::model::ByteCount;
use sha2::{Digest, Sha256};
use std::{borrow::Cow, fs, path::Path, time::Duration};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/gif")
            .join(name),
    )
    .unwrap()
}
fn comparison(a: &str, b: &str) -> Comparison {
    compare(
        &fixture(a),
        &fixture(b),
        GifLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap()
}

#[test]
fn streaming_compositor_matches_all_independently_expected_corpus_canvases() {
    // 生产验证器已由人工画布manifest核对，这里逐轮/逐背景比较新流式实现的同一摘要。
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/gif");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let cancel = CancellationToken::default();
    let limits = GifLimits::default();
    for sample in manifest["samples"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["expect"] == "ok")
    {
        let bytes = fixture(sample["file"].as_str().unwrap());
        let playback = validate_gif(&bytes, limits, &cancel).unwrap();
        let work = Work {
            cancel: &cancel,
            deadline: Instant::now() + Duration::from_secs(10),
        };
        let structure = structure::parse(&bytes, limits.validation_limits(), &work).unwrap();
        for logical in [false, true] {
            let mut stream =
                render::Frames::new(&bytes, &structure, limits.validation_limits(), logical)
                    .unwrap();
            let mut cycles = Vec::new();
            for _ in 0..if structure.loop_count.is_some() { 2 } else { 1 } {
                let mut events = Vec::new();
                for frame in &structure.frames {
                    stream.next(&work).unwrap();
                    events.push(playback::Event {
                        delay_cs: u64::from(frame.control.delay),
                        sha256: format!("{:x}", Sha256::digest(&stream.canvas)),
                    });
                }
                cycles.push(playback::normalize(&events).unwrap());
            }
            assert_eq!(
                cycles,
                if logical {
                    &playback.logical
                } else {
                    &playback.transparent
                }
                .clone(),
                "{}",
                sample["file"]
            );
        }
    }
}

#[test]
fn temporal_contract_allows_equivalent_frame_merge_and_rejects_timing_loop_metadata_alpha_changes()
{
    let merged = comparison("duplicate-visible.gif", "duplicate-merged.gif");
    assert!(merged.hard_constraints && merged.lossless_equivalent);
    assert_eq!(merged.transparent.unwrap().rmse, 0.0);
    for (before, after, reason) in [
        ("no-loop.gif", "changed-delay.gif", "timeline"),
        ("no-loop.gif", "finite-loop1.gif", "loop"),
        (
            "unequal-delay-comment.gif",
            "changed-comment.gif",
            "metadata",
        ),
    ] {
        let report = comparison(before, after);
        assert_eq!(report.rejection, Some(reason));
        assert!(!report.hard_constraints);
    }
    let mut target = fixture("offset-keep.gif");
    // 改动透明标记不破坏GIF结构，但会让合成alpha覆盖发生变化。
    let gce = target
        .windows(3)
        .position(|v| v == [0x21, 0xf9, 4])
        .unwrap();
    target[gce + 3] |= 1;
    target[gce + 6] = 1; // 首帧为红色索引1；将真实可见像素改为透明。
    let report = compare(
        &fixture("offset-keep.gif"),
        &target,
        GifLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(report.rejection, Some("transparency"));
}

#[test]
fn exact_motion_has_zero_temporal_error_and_altered_pixels_are_measured_not_claimed_lossless() {
    let exact = comparison("no-loop.gif", "no-loop.gif");
    let metric = exact.transparent.unwrap();
    assert_eq!(metric.temporal_error_rmse, 0.0);
    assert_eq!(metric.max_channel_error, 0);
    let altered = comparison("no-loop.gif", "changed-pixel.gif");
    assert!(altered.hard_constraints && !altered.lossless_equivalent);
    assert!(altered.transparent.unwrap().rmse > 0.0);
    let cancel = CancellationToken::default();
    let work = Work {
        cancel: &cancel,
        deadline: Instant::now() + Duration::from_secs(1),
    };
    let mut stats = Accumulator::default();
    stats
        .add(&[10, 10, 10, 255], &[12, 12, 12, 255], 1, &work)
        .unwrap();
    stats
        .add(&[20, 20, 20, 255], &[22, 22, 22, 255], 3, &work)
        .unwrap();
    let metrics = stats.finish();
    assert_eq!(metrics.rmse, 2.0);
    assert_eq!(metrics.temporal_error_rmse, 0.0); // 运动+固定误差没有额外闪烁。
}

#[test]
fn same_total_with_shifted_frame_transition_is_rejected() {
    let source = fixture("no-loop.gif");
    let mut target = source.clone();
    let offsets: Vec<_> = target
        .windows(3)
        .enumerate()
        .filter_map(|(i, v)| (v == [0x21, 0xf9, 4]).then_some(i))
        .collect();
    assert_eq!(offsets.len(), 2);
    target[offsets[0] + 4] += 1;
    target[offsets[1] + 4] -= 1;
    assert_eq!(
        compare(
            &source,
            &target,
            GifLimits::default(),
            &CancellationToken::default()
        )
        .unwrap()
        .rejection,
        Some("timeline")
    );
}

#[test]
fn limits_cancellation_and_expired_deadline_fail_before_large_work() {
    let source = fixture("no-loop.gif");
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(compare(&source, &source, GifLimits::default(), &cancel).is_err());
    let limits = GifLimits {
        resources: crate::model::ResourceLimits {
            max_decoded_bytes: ByteCount(1),
            ..GifLimits::default().resources
        },
        ..GifLimits::default()
    };
    assert!(compare(&source, &source, limits, &CancellationToken::default()).is_err());
    let pair_limits = GifLimits {
        resources: crate::model::ResourceLimits {
            max_decoded_bytes: ByteCount(1536 * 1024),
            ..GifLimits::default().resources
        },
        ..GifLimits::default()
    };
    assert!(validate_gif(&source, pair_limits, &CancellationToken::default()).is_ok());
    assert!(matches!(
        compare(&source, &source, pair_limits, &CancellationToken::default()),
        Err(GifError::ResourceLimit("GIF有损比较同时存活缓冲"))
    ));
    let token = CancellationToken::default();
    let work = Work {
        cancel: &token,
        deadline: Instant::now() - Duration::from_millis(1),
    };
    assert!(
        Accumulator::default()
            .add(&[0, 0, 0, 255], &[0, 0, 0, 255], 1, &work)
            .is_err()
    );
    assert!(
        compare(
            &source[..source.len() - 1],
            &source,
            GifLimits::default(),
            &token
        )
        .is_err()
    );
}

fn gray_animation(frames: &[(u8, u16)], looping: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut bytes, 1, 1, &[0, 0, 0, 255, 255, 255]).unwrap();
        if looping {
            encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        }
        for &(gray, delay) in frames {
            encoder
                .write_frame(&gif::Frame {
                    width: 1,
                    height: 1,
                    delay,
                    dispose: gif::DisposalMethod::Keep,
                    palette: Some(vec![gray, gray, gray, 255, 255, 255]),
                    buffer: Cow::Borrowed(&[0]),
                    ..Default::default()
                })
                .unwrap();
        }
    }
    bytes
}

#[test]
fn equivalent_splits_preserve_quality_and_unequal_duration_weighting() {
    let source = gray_animation(&[(10, 5), (20, 15)], false);
    let merged = gray_animation(&[(12, 5), (24, 15)], false);
    let split = gray_animation(&[(12, 2), (12, 3), (24, 6), (24, 9)], false);
    let measure = |target: &[u8]| {
        compare(
            &source,
            target,
            GifLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap()
    };
    let a = measure(&merged);
    let b = measure(&split);
    assert!(a.hard_constraints && b.hard_constraints);
    let (a, b) = (a.transparent.unwrap(), b.transparent.unwrap());
    assert_eq!(a.rmse, 13f64.sqrt());
    assert_eq!(a.rmse, b.rmse);
    assert_eq!(a.temporal_error_rmse, 2.0);
    assert_eq!(a.temporal_error_rmse, b.temporal_error_rmse);
    assert_eq!(a.worst_temporal_rmse, b.worst_temporal_rmse);
    assert_eq!(a.worst_interval_rmse, b.worst_interval_rmse);
    assert_eq!(a.changed_pixel_fraction, b.changed_pixel_fraction);
}

#[test]
fn brief_error_and_loop_seam_keep_worst_error_visible() {
    let source = gray_animation(&[(10, 100)], false);
    let target = gray_animation(&[(10, 99), (90, 1)], false);
    let report = compare(
        &source,
        &target,
        GifLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(report.hard_constraints);
    let m = report.transparent.unwrap();
    assert_eq!(m.rmse, 8.0);
    assert_eq!(m.worst_interval_rmse, 80.0);
    assert_eq!(m.max_channel_error, 80);
    assert_eq!(m.worst_temporal_rmse, 80.0);
    let source = gray_animation(&[(100, 2), (100, 2), (100, 2)], true);
    let target = gray_animation(&[(100, 2), (102, 2), (104, 2)], true);
    let report = compare(
        &source,
        &target,
        GifLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    // 单轮最大跳变2；第二轮第一帧误差4→0，必须观测到接缝跳变4。
    assert_eq!(report.transparent.unwrap().worst_temporal_rmse, 4.0);
}

#[test]
fn final_disposal_changes_second_cycle_alpha_on_three_pixel_canvas() {
    let make = |last: gif::DisposalMethod| {
        let mut bytes = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut bytes, 3, 1, &[0, 0, 0, 255, 0, 0]).unwrap();
            encoder.set_repeat(gif::Repeat::Infinite).unwrap();
            for (pixels, dispose) in [
                ([1, 0, 0], gif::DisposalMethod::Background),
                ([0, 0, 1], last),
            ] {
                encoder
                    .write_frame(&gif::Frame {
                        width: 3,
                        height: 1,
                        delay: 10,
                        transparent: Some(0),
                        dispose,
                        buffer: Cow::Borrowed(&pixels),
                        ..Default::default()
                    })
                    .unwrap();
            }
        }
        bytes
    };
    let source = make(gif::DisposalMethod::Background);
    let target = make(gif::DisposalMethod::Keep);
    let cancel = CancellationToken::default();
    let work = Work {
        cancel: &cancel,
        deadline: Instant::now() + Duration::from_secs(10),
    };
    for (bytes, expected) in [(&source, [255, 0, 0]), (&target, [255, 0, 255])] {
        let parsed =
            structure::parse(bytes, GifLimits::default().validation_limits(), &work).unwrap();
        let mut frames = render::Frames::new(
            bytes,
            &parsed,
            GifLimits::default().validation_limits(),
            false,
        )
        .unwrap();
        frames.next(&work).unwrap();
        assert_eq!(frames.canvas, [255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0]);
        frames.next(&work).unwrap();
        assert_eq!(frames.canvas, [0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 255]);
        frames.next(&work).unwrap();
        assert_eq!(
            [frames.canvas[3], frames.canvas[7], frames.canvas[11]],
            expected
        );
    }
    assert_eq!(
        compare(&source, &target, GifLimits::default(), &cancel)
            .unwrap()
            .rejection,
        Some("transparency")
    );
    assert!(
        !validate_gif(&source, GifLimits::default(), &cancel)
            .unwrap()
            .equivalent(&validate_gif(&target, GifLimits::default(), &cancel).unwrap())
    );
}
