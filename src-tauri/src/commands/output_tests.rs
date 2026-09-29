//! 真实任务/输出经生产IPC路由；原生选择结果用Rust注入，不冒充OS对话框验收。
use super::tests::{app, connected, invoke, mutate, native_grant, phase, window};
use crate::{ipc::DecimalU64, lifecycle::DesktopTasks, tasks::TaskPhase};
use pixofold_core::model::OutputDirectory;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tauri::{Manager, test::MockRuntime};

fn choose(app: &tauri::App<MockRuntime>, session: DecimalU64, path: &Path) -> Value {
    let directory = OutputDirectory::open(path).unwrap();
    let tasks = app.state::<DesktopTasks>();
    serde_json::to_value(
        tasks
            .imports
            .reserve(session)
            .unwrap()
            .complete_output(Some(directory))
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
fn target(grant: &Value, preserve: bool) -> Value {
    json!({"copy_to":{"directoryId":grant["directoryId"],"preserveStructure":preserve}})
}
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}

#[test]
fn chosen_directory_credentials_and_ordinary_retry_use_their_frozen_targets() {
    for preserve in [false, true] {
        let app = app(super::super::app_context());
        let main = window(&app, "main");
        let session = connected(&app);
        let dir = tempfile::tempdir().unwrap();
        let photos = dir.path().join("中文 photos");
        fs::create_dir_all(photos.join("sub")).unwrap();
        let a = photos.join("a.png");
        let bad = photos.join("bad.png");
        let credential = photos.join("sub/credential.png");
        fs::write(&a, fixture("rgb8.png")).unwrap();
        fs::write(&bad, fixture("bad-deflate.png")).unwrap();
        let original = fixture("content-credentials.png");
        fs::write(&credential, &original).unwrap();
        let out = dir.path().join("out");
        let other = dir.path().join("other");
        fs::create_dir(&out).unwrap();
        fs::create_dir(&other).unwrap();
        let selected = choose(&app, session, &out);
        assert!(selected.get("path").is_none());
        assert_eq!(selected["name"]["text"], "out");
        let grant = native_grant(&app, session, vec![photos.clone()]);
        let accepted = mutate(&main, session, json!({"kind":"import", "grantId":grant["grantId"], "settings":{"mode":{"kind":"lossless"}, "output":target(&selected,preserve)}})).unwrap();
        let tasks = app.state::<DesktopTasks>();
        let first = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
        assert_eq!(first.summary.succeeded, 1);
        assert_eq!(first.summary.failed, 2);
        let a_output = out.join(if preserve {
            "中文 photos/a_compressed.png"
        } else {
            "a_compressed.png"
        });
        assert!(a_output.is_file());
        assert!(!photos.join("a_compressed.png").exists());
        let id = first
            .jobs
            .iter()
            .find(|j| j.request.source.file_name().unwrap() == "credential.png")
            .unwrap()
            .id
            .get();
        let bad_id = first
            .jobs
            .iter()
            .find(|j| j.request.source.file_name().unwrap() == "bad.png")
            .unwrap()
            .id
            .get();
        let selected_other = choose(&app, session, &other);
        let confirm = json!({"kind":"confirm_content_credentials","selectionId":accepted["selectionId"],"expectedBatchRevision":first.revision.to_string(),"jobIds":[id],"mode":{"kind":"lossless"},"output":target(&selected_other,preserve),"consent":"remove_content_credentials"});
        mutate(&main, session, confirm.clone()).unwrap();
        let second = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
        assert_eq!(second.summary.succeeded, 2);
        assert!(mutate(&main, session, confirm).is_err());
        assert!(
            other
                .join(if preserve {
                    "中文 photos/sub/credential_compressed.png"
                } else {
                    "credential_compressed.png"
                })
                .exists()
        );
        assert!(!out.join("credential_compressed.png").exists());
        assert_eq!(fs::read(&credential).unwrap(), original);
        assert_eq!(fs::read(&a).unwrap(), fixture("rgb8.png"));
        invoke(&main,"release_output_directory",json!({"request":{"subscriptionId":session,"directoryId":selected_other["directoryId"]}})).unwrap();
        fs::write(&bad, fixture("rgb8.png")).unwrap();
        // 新会话无输出草稿授权，但已有失败行的目标仍然有效。
        let session = connected(&app);
        mutate(&main, session, json!({"kind":"retry","selectionId":accepted["selectionId"],"expectedBatchRevision":second.revision.to_string(),"jobIds":[bad_id],"mode":{"kind":"lossless"}})).unwrap();
        let last = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
        assert_eq!(last.summary.succeeded, 3);
        assert!(
            out.join(if preserve {
                "中文 photos/bad_compressed.png"
            } else {
                "bad_compressed.png"
            })
            .exists()
        );
        assert!(!other.join("bad_compressed.png").exists());
    }
}

#[test]
fn ready_can_correct_directory_conflicts_without_rescan_or_consuming_another_grant() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let session = connected(&app);
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.png");
    fs::write(&source, fixture("rgb8.png")).unwrap();
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("source_compressed.png"), b"never overwrite").unwrap();
    let import = native_grant(&app, session, vec![source.clone()]);
    let accepted = mutate(
        &main,
        session,
        json!({"kind":"import","grantId":import["grantId"],"settings":null}),
    )
    .unwrap();
    let tasks = app.state::<DesktopTasks>();
    let scan = phase(&tasks.control, TaskPhase::Ready).import.unwrap();
    let selected = choose(&app, session, &out);
    let start = |grant: &Value| json!({"kind":"start","selectionId":accepted["selectionId"],"settings":{"mode":{"kind":"lossless"},"output":target(grant,false)}});
    mutate(&main, session, start(&selected)).unwrap();
    let failed = phase(&tasks.control, TaskPhase::Ready);
    assert!(failed.error.is_some());
    assert!(std::sync::Arc::ptr_eq(
        &scan,
        failed.import.as_ref().unwrap()
    ));
    assert_eq!(
        fs::read(out.join("source_compressed.png")).unwrap(),
        b"never overwrite"
    );
    let other = dir.path().join("other");
    fs::create_dir(&other).unwrap();
    let selected_other = choose(&app, session, &other);
    assert_eq!(
        mutate(&main, session, start(&selected)).unwrap_err(),
        json!({"code":"stale_output_directory"})
    );
    mutate(&main, session, start(&selected_other)).unwrap();
    assert_eq!(
        phase(&tasks.control, TaskPhase::Finished)
            .batch
            .unwrap()
            .summary
            .succeeded,
        1
    );
    assert!(other.join("source_compressed.png").exists());
    assert_eq!(fs::read(source).unwrap(), fixture("rgb8.png"));
}

#[test]
fn directory_ipc_requires_session_and_rejects_paths_and_wrong_purpose_ids() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let other = window(&app, "other");
    let session = connected(&app);
    let dir = tempfile::tempdir().unwrap();
    let selected = choose(&app, session, dir.path());
    for command in ["select_output_directory", "release_output_directory"] {
        let request = json!({"subscriptionId":session,"directoryId":selected["directoryId"],"path":"arbitrary"});
        assert!(
            invoke(&main, command, json!({"request":request}))
                .unwrap_err()
                .is_string()
        );
        assert!(
            invoke(
                &other,
                command,
                json!({"request":{"subscriptionId":session}})
            )
            .unwrap_err()
            .is_string()
        );
    }
    assert_eq!(
        mutate(
            &main,
            session,
            json!({"kind":"import","grantId":selected["directoryId"],"settings":null})
        )
        .unwrap_err(),
        json!({"code":"stale_grant"})
    );
    let grant = native_grant(&app, session, vec![dir.path().to_owned()]);
    for output in [
        json!({"copy_to":{"directoryId":grant["grantId"],"preserveStructure":false}}),
        json!({"copy_to":{"directoryId":selected["directoryId"],"preserveStructure":false,"path":"arbitrary"}}),
    ] {
        assert!(mutate(&main,session,json!({"kind":"import","grantId":grant["grantId"],"settings":{"mode":{"kind":"lossless"},"output":output}})).is_err());
    }
    let next = connected(&app);
    assert_eq!(
        invoke(
            &main,
            "select_output_directory",
            json!({"request":{"subscriptionId":session}})
        )
        .unwrap_err(),
        json!({"code":"subscription","error":{"code":"stale_subscription"}})
    );
    let grant = native_grant(&app, next, vec![dir.path().to_owned()]);
    assert_eq!(mutate(&main,next,json!({"kind":"import","grantId":grant["grantId"],"settings":{"mode":{"kind":"lossless"},"output":target(&selected,false)}})).unwrap_err(),json!({"code":"stale_output_directory"}));
    assert_eq!(
        app.state::<DesktopTasks>().control.snapshot().phase,
        TaskPhase::Idle
    );
}
