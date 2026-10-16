//! 真实有损报告与v14冻结模式/质量边界，篡改只发生在隔离测试快照。
use super::super::super::apng_test_sample as sample;
use super::*;
use pixofold_core::apng::{ApngMode, ApngProcessing};

#[test]
fn real_lossy_report_keeps_only_final_metrics_and_rejects_inconsistent_values() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("palette.apng");
    fs::write(&source, sample::animation()).unwrap();
    let runtime =
        TaskRuntime::with_engines(TaskConfig::default(), ImageEngines::default().add_apng())
            .unwrap();
    let control = runtime.control();
    control
        .import(
            vec![source],
            Some(TaskSettings {
                parameters: BatchParameters {
                    mode: PngMode::Lossy {
                        quality: QualityValue::new(80).unwrap(),
                    },
                    ..Default::default()
                },
                output: ImportOutput::Overwrite,
            }),
        )
        .unwrap();
    let mut snapshot = phase(&control, TaskPhase::Finished);
    let page_request = request(TaskCollection::Jobs);
    let value = wire(convert::snapshot(&snapshot, &page_request).unwrap());
    let details = &value["page"]["items"][0]["state"]["report"]["processing"]["details"];
    assert_eq!(details["kind"], "lossy");
    assert_eq!(details["mappingVersion"], 1);
    assert_eq!(details["policyVersion"], 2);
    assert!(details["quality"]["minimumPixelQuality"].as_u64().unwrap() >= 80);
    let saved = snapshot.batch.as_ref().unwrap().jobs[0].state.clone();
    for fault in 0..11 {
        let job = &mut Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0];
        job.state = saved.clone();
        let JobState::Succeeded(ImageReport::Apng(report)) = &mut job.state else {
            panic!("expected lossy success");
        };
        match fault {
            0 => report.requested_mode = ApngMode::Lossless,
            1 => {
                report.requested_mode = ApngMode::Lossy {
                    quality: QualityValue::new(100).unwrap(),
                }
            }
            2 => report.processing = ApngProcessing::Lossless,
            3..=5 => {
                let ApngProcessing::Lossy { parameters } = &mut report.processing else {
                    panic!("lossy");
                };
                match fault {
                    3 => parameters.version = 2,
                    4 => parameters.policy_version = 1,
                    _ => parameters.dithering = true,
                };
            }
            6 => report.quality = None,
            7 => report.quality.as_mut().unwrap().palette_entries = 257,
            8 => report.quality.as_mut().unwrap().minimum_pixel_quality = 79,
            9 => report.quality.as_mut().unwrap().max_alpha_error = f64::NAN,
            _ => report.outcome = pixofold_core::model::ProcessingOutcome::NoGain,
        }
        assert!(
            convert::snapshot(&snapshot, &page_request).is_err(),
            "fault {fault}"
        );
    }
    let job = &mut Arc::make_mut(snapshot.batch.as_mut().unwrap()).jobs[0];
    let JobState::Succeeded(report) = saved else {
        panic!("success");
    };
    job.state = JobState::NoGain(report);
    assert!(
        convert::snapshot(&snapshot, &page_request).is_err(),
        "终态不能与真实文件结果错配"
    );
}
