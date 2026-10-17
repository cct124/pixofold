//! 使用真实IPC接纳、文件标记和压缩核心验证首次确认；不把mock窗口当作GUI验收。
use super::tests::{app_with_consent, connected, invoke, mutate, native_grant, phase, window};
use crate::{lifecycle::DesktopTasks, overwrite_consent::OverwriteConsent, tasks::TaskPhase};
use serde_json::{Value, json};
use std::{fs, path::Path, sync::Arc};
use tauri::Manager;

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}

fn acknowledge(
    main: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    session: crate::ipc::DecimalU64,
) -> Result<Value, Value> {
    invoke(
        main,
        "acknowledge_overwrite_warning",
        json!({"request": {
            "subscriptionId": session, "consent": "overwrite_without_backup"
        }}),
    )
}

#[test]
fn first_unsafe_import_scans_without_writing_and_only_current_acknowledged_main_can_confirm() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("app-data");
    let consent = Arc::new(OverwriteConsent::new(Some(data.clone())));
    let app = app_with_consent(super::super::app_context(), consent.clone());
    let main = window(&app, "main");
    let other = window(&app, "other");
    let session = connected(&app);
    let source = temp.path().join("source.png");
    let original = fixture("rgb8.png");
    fs::write(&source, &original).unwrap();
    let grant = native_grant(&app, session, vec![source.clone()]);
    let settings = json!({"mode":{"kind":"lossless"},"output":"overwrite_without_backup"});
    let accepted = mutate(
        &main,
        session,
        json!({"kind":"import","grantId":grant["grantId"],"settings":settings}),
    )
    .unwrap();
    assert_eq!(accepted["overwriteConfirmationRequired"], true);
    let tasks = app.state::<DesktopTasks>();
    phase(&tasks.control, TaskPhase::Ready);
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(!consent.acknowledged());
    let start = json!({"kind":"start","selectionId":accepted["selectionId"],"settings":settings});
    assert_eq!(
        mutate(&main, session, start.clone()).unwrap_err(),
        json!({"code":"overwrite_confirmation_required"})
    );
    assert!(acknowledge(&other, session).is_err());
    assert!(acknowledge(&main, crate::ipc::DecimalU64(session.0 + 1)).is_err());
    assert!(invoke(&main, "acknowledge_overwrite_warning", json!({"request":{"subscriptionId":session,"consent":"overwrite_without_backup","path":"ignored"}})).is_err());
    assert!(!consent.acknowledged());
    assert_eq!(acknowledge(&main, session).unwrap(), Value::Null);
    assert!(OverwriteConsent::new(Some(data)).acknowledged());
    assert_eq!(tasks.control.snapshot().phase, TaskPhase::Ready);
    mutate(&main, session, start).unwrap();
    let finished = phase(&tasks.control, TaskPhase::Finished);
    assert_eq!(finished.batch.unwrap().summary.succeeded, 1);
    assert_ne!(fs::read(&source).unwrap(), original);
    mutate(
        &main,
        session,
        json!({"kind":"clear","selectionId":accepted["selectionId"]}),
    )
    .unwrap();
    phase(&tasks.control, TaskPhase::Idle);
    let grant = native_grant(&app, session, vec![source]);
    let again = mutate(
        &main,
        session,
        json!({"kind":"import","grantId":grant["grantId"],"settings":settings}),
    )
    .unwrap();
    assert!(again.get("overwriteConfirmationRequired").is_none());
    phase(&tasks.control, TaskPhase::Finished);
}

#[test]
fn backup_and_copy_do_not_require_consent_and_empty_scan_never_starts() {
    for output in ["overwrite", "copy_beside"] {
        let temp = tempfile::tempdir().unwrap();
        let consent = Arc::new(OverwriteConsent::new(Some(temp.path().join("data"))));
        let app = app_with_consent(super::super::app_context(), consent.clone());
        let main = window(&app, "main");
        let session = connected(&app);
        let source = temp.path().join("source.png");
        fs::write(&source, fixture("rgb8.png")).unwrap();
        let grant = native_grant(&app, session, vec![source]);
        let accepted = mutate(&main, session, json!({"kind":"import","grantId":grant["grantId"],"settings":{"mode":{"kind":"lossless"},"output":output}})).unwrap();
        assert!(accepted.get("overwriteConfirmationRequired").is_none());
        phase(&app.state::<DesktopTasks>().control, TaskPhase::Finished);
        assert!(!consent.acknowledged());
    }
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_consent(
        super::super::app_context(),
        Arc::new(OverwriteConsent::new(None)),
    );
    let main = window(&app, "main");
    let session = connected(&app);
    let source = temp.path().join("empty.png");
    fs::write(&source, b"invalid").unwrap();
    let grant = native_grant(&app, session, vec![source]);
    mutate(&main, session, json!({"kind":"import","grantId":grant["grantId"],"settings":{"mode":{"kind":"lossless"},"output":"overwrite_without_backup"}})).unwrap();
    let rejected = phase(&app.state::<DesktopTasks>().control, TaskPhase::Rejected);
    assert!(rejected.batch.is_none());
}

#[test]
fn save_failure_does_not_start_and_retry_rechecks_the_frozen_output_after_data_removal() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let consent = Arc::new(OverwriteConsent::new(Some(data.clone())));
    consent.acknowledge().unwrap();
    let app = app_with_consent(super::super::app_context(), consent.clone());
    let main = window(&app, "main");
    let session = connected(&app);
    let source = temp.path().join("bad.png");
    let original = fixture("bad-deflate.png");
    fs::write(&source, &original).unwrap();
    let grant = native_grant(&app, session, vec![source.clone()]);
    let accepted = mutate(&main, session, json!({"kind":"import","grantId":grant["grantId"],"settings":{"mode":{"kind":"lossless"},"output":"overwrite_without_backup"}})).unwrap();
    let control = &app.state::<DesktopTasks>().control;
    let first = phase(control, TaskPhase::Finished).batch.unwrap();
    assert_eq!(first.summary.failed, 1);
    let marker = fs::read_dir(&data).unwrap().next().unwrap().unwrap().path();
    fs::remove_file(&marker).unwrap();
    let retry = json!({"kind":"retry","selectionId":accepted["selectionId"],"expectedBatchRevision":first.revision.to_string(),"jobIds":[first.jobs[0].id.get()],"mode":{"kind":"lossless"}});
    assert_eq!(
        mutate(&main, session, retry.clone()).unwrap_err(),
        json!({"code":"overwrite_confirmation_required"})
    );
    // 同名目录阻止标记写入，不能因为用户点击确认就放行。
    fs::create_dir(&marker).unwrap();
    assert_eq!(
        acknowledge(&main, session).unwrap_err(),
        json!({"code":"overwrite_confirmation_storage"})
    );
    assert!(!consent.acknowledged());
    assert_eq!(
        control.snapshot().batch.as_ref().unwrap().revision,
        first.revision
    );
    assert_eq!(fs::read(source).unwrap(), original);
    fs::remove_dir(&marker).unwrap();
    acknowledge(&main, session).unwrap();
    mutate(&main, session, retry).unwrap();
    let final_batch = phase(control, TaskPhase::Finished).batch.unwrap();
    assert_eq!(final_batch.jobs[0].attempt, 2);
}

#[test]
fn credentials_confirmation_also_checks_its_selected_overwrite_policy() {
    let temp = tempfile::tempdir().unwrap();
    let consent = Arc::new(OverwriteConsent::new(Some(temp.path().join("data"))));
    let app = app_with_consent(super::super::app_context(), consent);
    let main = window(&app, "main");
    let session = connected(&app);
    let source = temp.path().join("credential.png");
    let original = fixture("content-credentials.png");
    fs::write(&source, &original).unwrap();
    let grant = native_grant(&app, session, vec![source.clone()]);
    let accepted = mutate(&main, session, json!({"kind":"import","grantId":grant["grantId"],"settings":{"mode":{"kind":"lossless"},"output":"overwrite"}})).unwrap();
    let control = &app.state::<DesktopTasks>().control;
    let first = phase(control, TaskPhase::Finished).batch.unwrap();
    let confirm = json!({"kind":"confirm_content_credentials","selectionId":accepted["selectionId"],"expectedBatchRevision":first.revision.to_string(),"jobIds":[first.jobs[0].id.get()],"mode":{"kind":"lossless"},"output":"overwrite_without_backup","consent":"remove_content_credentials"});
    assert_eq!(
        mutate(&main, session, confirm.clone()).unwrap_err(),
        json!({"code":"overwrite_confirmation_required"})
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    acknowledge(&main, session).unwrap();
    mutate(&main, session, confirm).unwrap();
    assert_eq!(
        phase(control, TaskPhase::Finished)
            .batch
            .unwrap()
            .summary
            .succeeded,
        1
    );
}
