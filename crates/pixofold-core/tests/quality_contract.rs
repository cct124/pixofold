use pixofold_core::{
    model::{PngMode, PngRequest, QualityValue},
    quality::png_quality,
};

#[test]
fn all_integer_quality_values_map_without_clamping_or_native_minimum_leakage() {
    for value in 0..=100 {
        let quality = QualityValue::new(value).unwrap();
        let mapping = png_quality(quality);
        assert_eq!(mapping.version, 1);
        assert_eq!(mapping.quality.get(), value as u8);
        assert_eq!(mapping.minimum, 0);
        assert_eq!(mapping.target, value as u8);
        assert_eq!(serde_json::to_string(&quality).unwrap(), value.to_string());
        assert_eq!(
            serde_json::from_str::<QualityValue>(&value.to_string()).unwrap(),
            quality
        );
    }
    for value in [i32::MIN, -1, 101, i32::MAX] {
        assert!(QualityValue::new(value).is_err());
    }
    assert_eq!(QualityValue::default().get(), 80);
}

#[test]
fn serialized_mode_has_exactly_one_quality_source_and_rejects_invalid_boundary_values() {
    let mode = PngMode::Lossy {
        quality: QualityValue::default(),
    };
    let payload = serde_json::json!({"kind":"lossy", "quality":80});
    assert_eq!(serde_json::to_value(mode).unwrap(), payload);
    assert_eq!(serde_json::from_value::<PngMode>(payload).unwrap(), mode);
    assert_eq!(
        serde_json::to_value(PngMode::Lossless).unwrap(),
        serde_json::json!({"kind":"lossless"})
    );
    assert_eq!(
        serde_json::from_str::<PngMode>(r#"{"kind":"lossless"}"#).unwrap(),
        PngMode::Lossless
    );
    for json in [
        r#"{"kind":"lossless","quality":80}"#,
        r#"{"kind":"lossy"}"#,
        r#"{"kind":"lossy","quality":-1}"#,
        r#"{"kind":"lossy","quality":101}"#,
        r#"{"kind":"lossy","quality":80.5}"#,
        r#"{"kind":"lossy","quality":80.0}"#,
        r#"{"kind":"lossy","quality":"80"}"#,
        r#"{"kind":"lossy","quality":null}"#,
        r#"{"kind":"lossy","quality":80,"target":1}"#,
        r#"{"kind":"lossy","quality":80,"quality":40}"#,
        r#"{"kind":"unknown"}"#,
    ] {
        assert!(serde_json::from_str::<PngMode>(json).is_err(), "{json}");
    }
}

#[test]
fn existing_request_constructor_remains_explicitly_lossless() {
    let request = PngRequest::new("sample.png");
    assert_eq!(request.mode, PngMode::Lossless);
    assert_eq!(PngMode::default(), PngMode::Lossless);
}
