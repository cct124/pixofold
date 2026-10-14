use super::*;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/apng")
}

#[test]
fn deterministic_corpus_matches_manual_structure_pixels_and_time() {
    let root = fixtures();
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for sample in manifest["samples"].as_array().unwrap() {
        let name = sample["file"].as_str().unwrap();
        let input = fs::read(root.join(name)).unwrap();
        assert_eq!(hash(&input), sample["sha256"], "{name}");
        let limits =
            serde_json::from_value(sample.get("limits").cloned().unwrap_or(json!({}))).unwrap();
        let result = inspect(&input, &limits, &CancellationToken::default());
        if sample["expect"] != "ok" {
            assert!(result.is_err(), "{name}: unexpectedly accepted");
            assert_eq!(result.unwrap_err().0, sample["expect"], "{name}");
            continue;
        }
        let summary = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let view = serde_json::to_value(&summary).unwrap();
        assert_eq!(
            view["defaultRawSha256"], sample["defaultRawSha256"],
            "{name}/poster"
        );
        for (key, expected) in sample["structure"].as_object().unwrap() {
            assert_eq!(view[key], *expected, "{name}/{key}");
        }
        assert_eq!(
            summary.frames.len(),
            sample["controls"].as_array().unwrap().len(),
            "{name}"
        );
        for (i, frame) in summary.frames.iter().enumerate() {
            assert_eq!(
                frame.raw_sha256, sample["frameRawSha256"][i],
                "{name}/{i}/raw"
            );
            assert_eq!(
                serde_json::to_value(&frame.control).unwrap(),
                sample["controls"][i],
                "{name}/{i}/control"
            );
            assert_eq!(
                serde_json::to_value(&frame.canvas).unwrap(),
                sample["canvases"][i],
                "{name}/{i}/canvas"
            );
            assert_eq!(
                frame.effective_delay,
                [
                    frame.control.delay_num,
                    if frame.control.delay_den == 0 {
                        100
                    } else {
                        frame.control.delay_den
                    }
                ],
                "{name}/{i}/time"
            );
        }
        assert_eq!(
            summary.final_canvas_sha256,
            summary.frames.last().unwrap().canvas_sha256
        );
        assert_eq!(summary.loop_start, "transparent_black_each_play");
        assert_eq!(
            summary.second_play_canvases.len(),
            if summary.plays == 1 {
                0
            } else {
                summary.frames.len()
            }
        );
        for (frame, second) in summary.frames.iter().zip(&summary.second_play_canvases) {
            assert_eq!(&frame.canvas_sha256, second, "{name}/second play");
        }
    }
    for pair in manifest["comparisons"].as_array().unwrap() {
        let a = fs::read(root.join(pair["a"].as_str().unwrap())).unwrap();
        let b = fs::read(root.join(pair["b"].as_str().unwrap())).unwrap();
        let result = compare(&a, &b, &Limits::default(), &CancellationToken::default());
        assert_eq!(
            result.is_ok(),
            pair["equivalent"].as_bool().unwrap(),
            "{pair}: {result:?}"
        );
        if let Err(error) = result {
            assert_eq!(error.0, "not_equivalent");
        }
    }
}

#[test]
fn candidate_budget_cancellation_and_sixteen_bit_work_are_enforced() {
    let input = fs::read(fixtures().join("default-in.png")).unwrap();
    let measured = inspect(&input, &Limits::default(), &CancellationToken::default()).unwrap();
    let combined = Limits {
        max_decoded_bytes: measured.decoded_bytes,
        ..Limits::default()
    };
    assert_eq!(
        compare(&input, &input, &combined, &CancellationToken::default())
            .unwrap_err()
            .0,
        "decoded_budget"
    );
    let combined = Limits {
        max_total_pixels: measured.decoded_pixels,
        ..Limits::default()
    };
    assert_eq!(
        compare(&input, &input, &combined, &CancellationToken::default())
            .unwrap_err()
            .0,
        "total_pixel_budget"
    );
    let invalid = Limits {
        max_canvas_pixels: u64::MAX,
        ..Limits::default()
    };
    assert_eq!(
        inspect(&input, &invalid, &CancellationToken::default())
            .unwrap_err()
            .0,
        "invalid_limits"
    );
    let limits = Limits {
        max_candidate_bytes: 1,
        ..Limits::default()
    };
    assert_eq!(
        compare(&input, &input, &limits, &CancellationToken::default())
            .unwrap_err()
            .0,
        "candidate_budget"
    );
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        inspect(&input, &Limits::default(), &token).unwrap_err().0,
        "cancelled"
    );
    let wide = fs::read(fixtures().join("rgba16.apng")).unwrap();
    let narrow = fs::read(fixtures().join("gray-alpha8.apng")).unwrap();
    let limits = Limits {
        max_decoded_bytes: 40,
        ..Limits::default()
    };
    assert!(inspect(&narrow, &limits, &CancellationToken::default()).is_ok());
    assert_eq!(
        inspect(&wide, &limits, &CancellationToken::default())
            .unwrap_err()
            .0,
        "decoded_budget"
    );
}

#[test]
fn existing_static_png_gate_still_rejects_apng() {
    // 生产支持不因开发验证器存在而扩张；静态回归语料保持原有拒绝路径。
    let input = fs::read(fixtures().join("default-in.png")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("input.png");
    fs::write(&file, input).unwrap();
    let request = pixofold_core::model::PngRequest::new(file);
    let result =
        pixofold_core::pipeline::optimize_png(&request, &CancellationToken::default(), |_| {});
    assert!(
        matches!(
            result,
            Err(pixofold_core::model::ProcessingError::UnsupportedAnimation)
        ),
        "{result:?}"
    );
}
