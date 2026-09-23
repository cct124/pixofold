use pixofold_core::app_info;
use serde_json::json;

#[test]
fn app_info_reports_compression_without_claiming_all_planned_formats() {
    let payload = serde_json::to_value(app_info()).expect("应用信息可序列化");
    assert_eq!(
        payload,
        json!({
            "name": "PixoFold",
            "version": env!("CARGO_PKG_VERSION"),
            "plannedFormats": ["png", "jpeg", "gif", "apng"],
            "compressionAvailable": true
        })
    );
}
