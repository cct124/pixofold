//! 首帧与支持矩阵核对确定性语料的独立预期画布；不使用验证器输出作为预期值。
use pixofold_core::{apng::*, model::CancellationToken};
use std::{fs, path::Path};
#[test]
fn first_animation_canvas_matches_corpus_including_separate_poster_and_rgba16() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/apng");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let mut checked = 0;
    for entry in manifest["samples"].as_array().unwrap() {
        if entry["expect"] != "ok" {
            continue;
        }
        let Some(canvas) = entry["canvases"]
            .as_array()
            .and_then(|c| c.first())
            .and_then(|v| v.as_array())
        else {
            continue;
        };
        let name = entry["file"].as_str().unwrap();
        let bytes = fs::read(root.join(name)).unwrap();
        let preview = decode_preview(&bytes, &Default::default(), &Default::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let expected: Vec<u8> = canvas
            .iter()
            .flat_map(|p| {
                p.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| ((v.as_u64().unwrap() + 128) / 257) as u8)
            })
            .collect();
        assert_eq!(preview.pixels, expected, "{name}");
        checked += 1;
    }
    assert!(checked >= 25);
}
#[test]
fn preview_rejects_metadata_resource_limits_corrupt_pixels_and_revocation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/apng");
    for name in [
        "metadata-caBX.apng",
        "metadata-iCCP.apng",
        "metadata-eXIf.apng",
        "low-depth.apng",
    ] {
        assert!(
            decode_preview(
                &fs::read(root.join(name)).unwrap(),
                &Default::default(),
                &Default::default()
            )
            .is_err(),
            "{name}"
        );
    }
    // 语料的坏deflate在后续帧；首帧预览不授予整图有效性证明。
    let later_bad = fs::read(root.join("bad-deflate.apng")).unwrap();
    assert!(decode_preview(&later_bad, &Default::default(), &Default::default()).is_ok());
    assert!(validate_apng(&later_bad, &Default::default(), &Default::default()).is_err());
    // 首帧的流头与Adler分别破坏，重算容器CRC，确保错误在真实像素解码边界检测。
    for corrupt_header in [true, false] {
        let mut damaged = fs::read(root.join("default-in.png")).unwrap();
        let mut at = 8;
        while &damaged[at + 4..at + 8] != b"IDAT" {
            at += u32::from_be_bytes(damaged[at..at + 4].try_into().unwrap()) as usize + 12;
        }
        let end = at + u32::from_be_bytes(damaged[at..at + 4].try_into().unwrap()) as usize + 8;
        damaged[if corrupt_header { at + 8 } else { end - 1 }] ^= 0xff;
        let crc = crc32fast::hash(&damaged[at + 4..end]);
        damaged[end..end + 4].copy_from_slice(&crc.to_be_bytes());
        assert!(matches!(
            decode_preview(&damaged, &Default::default(), &Default::default()),
            Err(ApngError::Validation(ApngValidationError("decode")))
        ));
    }
    let bytes = fs::read(root.join("first-over-transparent.apng")).unwrap();
    for limits in [
        ApngValidationLimits {
            max_validation_bytes: 1,
            ..Default::default()
        },
        ApngValidationLimits {
            max_canvas_pixels: 1,
            ..Default::default()
        },
        ApngValidationLimits {
            deadline_ms: 0,
            ..Default::default()
        },
    ] {
        assert!(decode_preview(&bytes, &limits, &Default::default()).is_err());
    }
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(matches!(
        decode_preview(&bytes, &Default::default(), &cancel),
        Err(ApngError::Cancelled)
    ));
}
