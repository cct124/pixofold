use super::*;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/apng")
}
fn input(name: &str) -> Vec<u8> {
    fs::read(fixtures().join(name)).unwrap()
}
fn candidate(source: &[u8]) -> Candidate {
    quantize(
        source,
        Options {
            quality: 100,
            ..Options::default()
        },
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap()
}
fn mutate(bytes: &[u8], mut change: impl FnMut(&[u8], &mut Vec<u8>)) -> Vec<u8> {
    let mut out = structure::SIGNATURE.to_vec();
    for (k, d) in chunks(bytes) {
        let mut d = d.to_vec();
        change(k, &mut d);
        structure::chunk(&mut out, k.try_into().unwrap(), &d);
    }
    out
}
#[test]
fn existing_corpus_retains_structural_and_resource_rejections() {
    let manifest: Value = serde_json::from_slice(&input("manifest.json")).unwrap();
    for sample in manifest["samples"].as_array().unwrap() {
        let name = sample["file"].as_str().unwrap();
        let source = input(name);
        let limits =
            serde_json::from_value(sample.get("limits").cloned().unwrap_or(json!({}))).unwrap();
        let expected = if sample["expect"] != "ok" {
            sample["expect"].as_str().unwrap()
        } else if sample["structure"]["depth"] == 16 {
            "unsupported_depth"
        } else if sample["structure"]["interlace"] == 1 {
            "unsupported_interlace"
        } else {
            "ok"
        };
        let result = quantize(
            &source,
            Options {
                quality: 100,
                limits,
                ..Options::default()
            },
            &CancellationToken::default(),
            |_| {},
        );
        if expected != "ok" {
            assert_eq!(result.unwrap_err().0, expected, "{name}");
            continue;
        }
        let encoded = result.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(encoded.report.palette_entries <= 256);
        let actual = super::super::inspect(
            &encoded.bytes,
            &Limits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
        assert_eq!(actual.depth, 8);
        assert_eq!(actual.color, 3);
        assert_eq!(
            serde_json::to_value(actual.frames.iter().map(|f| &f.control).collect::<Vec<_>>())
                .unwrap(),
            sample["controls"],
            "{name}"
        );
        assert_eq!(
            actual.default_participates,
            sample["structure"]["defaultParticipates"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(
            actual.plays,
            u32::try_from(sample["structure"]["plays"].as_u64().unwrap()).unwrap()
        );
    }
}
#[test]
fn shared_palette_is_exact_for_small_color_sets_and_keeps_poster_and_two_plays() {
    for name in [
        "default-in.png",
        "default-out.apng",
        "previous.apng",
        "background.apng",
        "first-previous.apng",
        "first-over-transparent.apng",
        "half-over-half.apng",
        "palette.apng",
        "metadata.apng",
        "split-data.apng",
    ] {
        let source = input(name);
        let encoded = candidate(&source);
        let c = &encoded.report.comparison;
        assert!(encoded.report.quality_passed, "{name}: {c:?}");
        assert_eq!(c.black.rmse, 0.0, "{name}");
        assert_eq!(c.white.rmse, 0.0, "{name}");
        assert_eq!(c.checked_plays, 2);
        assert_eq!(
            chunks(&encoded.bytes)
                .filter(|(k, _)| *k == b"PLTE")
                .count(),
            1
        );
        // 生产严格无损仍禁止色型/隐藏RGB变化。
        assert!(
            super::super::compare(
                &source,
                &encoded.bytes,
                &Limits::default(),
                &CancellationToken::default()
            )
            .is_err()
        );
    }
    assert_eq!(
        candidate(&input("one-play.apng"))
            .report
            .comparison
            .checked_plays,
        1
    );
}
#[test]
fn changed_control_time_metadata_and_bad_indices_are_hard_failures() {
    let source = input("metadata.apng");
    let encoded = candidate(&source);
    for (kind, offset) in [
        (b"fcTL", 20),
        (b"fcTL", 24),
        (b"fcTL", 25),
        (b"acTL", 7),
        (b"tEXt", 10),
    ] {
        let changed = mutate(&encoded.bytes, |k, d| {
            if k == kind {
                d[offset] ^= 1;
            }
        });
        let error = compare(
            &source,
            &changed,
            Options::default(),
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert_eq!(
            error.0,
            if kind == b"tEXt" {
                "metadata_changed"
            } else {
                "animation_changed"
            }
        );
    }
    let missing_palette_entry = mutate(&encoded.bytes, |k, d| {
        if k == b"PLTE" {
            d.truncate(3);
        }
    });
    assert_eq!(
        compare(
            &source,
            &missing_palette_entry,
            Options::default(),
            &CancellationToken::default()
        )
        .unwrap_err()
        .0,
        "decode"
    );
    let truncated = &encoded.bytes[..encoded.bytes.len() - 1];
    assert!(
        compare(
            &source,
            truncated,
            Options::default(),
            &CancellationToken::default()
        )
        .is_err()
    );
}
#[test]
fn independent_poster_color_and_alpha_changes_are_observed() {
    let source = input("default-out.apng");
    let changed = candidate(&input("changed-poster.apng"));
    let c = compare(
        &source,
        &changed.bytes,
        Options::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(c.black.frames[0].rmse > 0.0);
    assert_eq!(c.black.frames[0].play, None);
    assert!(!meets_quality(&c, 100, 100));
    let source = input("palette.apng");
    let encoded = candidate(&source);
    let changed = mutate(&encoded.bytes, |k, d| {
        if k == b"tRNS" {
            for a in d {
                if *a == 0 {
                    *a = 1;
                }
            }
        }
    });
    let c = compare(
        &source,
        &changed,
        Options::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(c.alpha_class_changes > 0);
    assert!(!meets_quality(&c, 0, 100));
    assert!(quality_rejections(&c, 80, 100).contains(&QualityRejection::AlphaClassChanged));
}
#[test]
fn all_stages_cancel_and_cumulative_work_cannot_reset_between_passes() {
    let source = input("default-out.apng");
    for stage in [
        Stage::Histogram,
        Stage::Quantizing,
        Stage::Remapping,
        Stage::Comparing,
    ] {
        let token = CancellationToken::default();
        let result = quantize(&source, Options::default(), &token, |s| {
            if s == stage {
                token.cancel();
            }
        });
        assert_eq!(result.unwrap_err().0, "cancelled");
    }
    let measured = candidate(&source).report;
    let cases = [
        (
            Options {
                max_palette_searches: 0,
                ..Options::default()
            },
            "palette_search_budget",
        ),
        (
            Options {
                max_histogram_samples: 1,
                ..Options::default()
            },
            "histogram_budget",
        ),
        (
            Options {
                max_working_set_bytes: 1,
                ..Options::default()
            },
            "working_set_budget",
        ),
        (
            Options {
                quality: 101,
                ..Options::default()
            },
            "invalid_limits",
        ),
        (
            Options {
                limits: Limits {
                    deadline_ms: 0,
                    ..Limits::default()
                },
                ..Options::default()
            },
            "deadline",
        ),
        (
            Options {
                limits: Limits {
                    max_candidate_bytes: 1,
                    ..Limits::default()
                },
                ..Options::default()
            },
            "candidate_budget",
        ),
        (
            Options {
                limits: Limits {
                    max_total_pixels: measured.processed_pixels - 1,
                    ..Limits::default()
                },
                ..Options::default()
            },
            "total_pixel_budget",
        ),
        (
            Options {
                limits: Limits {
                    max_decoded_bytes: measured.decoded_bytes - 1,
                    ..Limits::default()
                },
                ..Options::default()
            },
            "decoded_budget",
        ),
    ];
    for (options, code) in cases {
        assert_eq!(
            quantize(&source, options, &CancellationToken::default(), |_| {})
                .unwrap_err()
                .0,
            code
        );
    }
}

fn animation(pixels: &[[u8; 4]], second: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, pixels.len() as u32, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(2, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        for frame in [pixels, second] {
            writer
                .write_image_data(&frame.iter().flatten().copied().collect::<Vec<_>>())
                .unwrap();
        }
        writer.finish().unwrap();
    }
    bytes
}

#[test]
fn grouped_palette_covers_unsampled_alpha_and_preserves_classes() {
    let mut first = vec![[20, 120, 200, 255]; 129];
    first[128] = [255, 0, 255, 1];
    let mut second = first.clone();
    second[0] = [255, 255, 0, 254];
    let source = animation(&first, &second);
    let encoded = quantize(
        &source,
        Options {
            quality: 80,
            max_histogram_samples: 2,
            ..Options::default()
        },
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(encoded.report.remapping_stats.unrepresentable_pixels, 0);
    assert_eq!(encoded.report.comparison.alpha_class_changes, 0);
    assert!(encoded.report.comparison.max_alpha_error <= 4.0);
    // q100最多256种alpha，各一色时可精确表示；不存在静默透明端点折叠。
    let pixels: Vec<_> = (0..=255).map(|a| [96, 128, 160, a]).collect();
    let source = animation(&pixels, &pixels);
    let constrained = quantize(
        &source,
        Options::default(),
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(constrained.report.comparison.alpha_class_changes, 0);
    assert!(constrained.report.comparison.max_alpha_error <= 4.0);
    assert_eq!(constrained.report.remapping_stats.unrepresentable_pixels, 0);
    let encoded = candidate(&animation(&pixels, &pixels));
    assert_eq!(encoded.report.palette_entries, 256);
    assert_eq!(encoded.report.comparison.max_alpha_error, 0.0);
    assert!(encoded.report.quality_passed);
}

#[test]
fn decoded_score_rejects_color_damage_after_alpha_repair() {
    let source = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/manual-images/apng/Animated_PNG_example_bouncing_beach_ball.png"),
    )
    .unwrap();
    let result = quantize(
        &source,
        Options {
            quality: 80,
            remapping: Remapping::AlphaConstrained,
            ..Options::default()
        },
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(result.report.comparison.alpha_class_changes, 0);
    assert!(result.report.comparison.max_alpha_error <= 8.0);
    assert!(result.report.minimum_remapping_quality >= 80);
    assert!(result.report.comparison.minimum_pixel_quality < 80);
    assert!(
        result
            .report
            .quality_rejections
            .contains(&QualityRejection::ActualPixelsBelowTarget)
    );
    assert!(result.report.remapping_stats.corrected_pixels > 0);
}

#[test]
fn palette_coverage_failure_is_quality_rejection_and_not_silent_fallback() {
    let source = animation(
        &[[255, 0, 0, 0], [0, 0, 255, 128]],
        &[[255, 0, 0, 255], [0, 0, 255, 128]],
    );
    let result = quantize(
        &source,
        Options {
            quality: 0,
            remapping: Remapping::AlphaConstrained,
            ..Options::default()
        },
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert!(result.report.remapping_stats.unrepresentable_pixels > 0);
    assert!(
        result
            .report
            .quality_rejections
            .contains(&QualityRejection::AlphaPaletteUnavailable)
    );
    assert!(!result.report.quality_passed);
}
