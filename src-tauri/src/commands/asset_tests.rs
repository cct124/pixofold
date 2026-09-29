//! 生产展示IPC的会话、行身份、路径隔离与读取回归；不启动系统文件管理器。
use super::tests::{app, connected, invoke, mutate, native_grant, phase, window};
use crate::{lifecycle::DesktopTasks, tasks::TaskPhase};
use serde_json::json;
use std::{fs, path::Path};
use tauri::Manager;

#[test]
fn thumbnail_command_reads_only_the_authorized_current_job_and_never_serializes_paths() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let session = connected(&app);
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("private中文, name.png");
    let input =
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png/rgb8.png"))
            .unwrap();
    fs::write(&source, &input).unwrap();
    let grant = native_grant(&app, session, vec![source.clone()]);
    let accepted = mutate(&main, session, json!({"kind":"import","grantId":grant["grantId"],"settings":{"mode":{"kind":"lossless"},"output":"overwrite"}})).unwrap();
    let tasks = app.state::<DesktopTasks>();
    let finished = phase(&tasks.control, TaskPhase::Finished);
    let job = &finished.batch.as_ref().unwrap().jobs[0];
    let request = json!({"subscriptionId":session.0.to_string(),"selectionId":accepted["selectionId"],"jobId":job.id.get(),"attempt":job.attempt,"expectedState":"succeeded"});
    let bytes_before = fs::read(&source).unwrap();
    let image = invoke(&main, "get_task_thumbnail", json!({"request":request})).unwrap();
    assert!(image["width"].as_u64().unwrap() <= 128);
    assert!(image["png"].as_array().unwrap().len() <= 65_536);
    assert!(!image.to_string().contains("private"));
    assert!(image.get("path").is_none());
    assert_eq!(fs::read(&source).unwrap(), bytes_before);
    for (field, value) in [("path", json!(source)), ("limit", json!(9999))] {
        let mut wrong = request.clone();
        wrong[field] = value;
        assert!(invoke(&main, "get_task_thumbnail", json!({"request":wrong})).is_err());
    }
    let mut wrong = request.clone();
    wrong["attempt"] = json!(2);
    assert_eq!(
        invoke(&main, "get_task_thumbnail", json!({"request":wrong})).unwrap_err(),
        json!({"code":"stale_task"})
    );
    // 文件被外部移走时，即便缓存已命中也不返回旧像素；定位不能误报成功或打开其他目标。
    fs::remove_file(&source).unwrap();
    assert_eq!(
        invoke(&main, "get_task_thumbnail", json!({"request":request})).unwrap_err(),
        json!({"code":"file_missing"})
    );
    assert_eq!(
        invoke(
            &main,
            "reveal_task_file",
            json!({"request":{"job":request,"target":"result"}})
        )
        .unwrap_err(),
        json!({"code":"file_missing"})
    );
    mutate(
        &main,
        session,
        json!({"kind":"clear","selectionId":accepted["selectionId"]}),
    )
    .unwrap();
    phase(&tasks.control, TaskPhase::Idle);
    assert_eq!(
        invoke(&main, "get_task_thumbnail", json!({"request":request})).unwrap_err(),
        json!({"code":"stale_task"})
    );
    tasks.page_load("main", tauri::webview::PageLoadEvent::Started);
    assert_eq!(
        invoke(&main, "get_task_thumbnail", json!({"request":request})).unwrap_err(),
        json!({"code":"session_unavailable"})
    );
}

#[test]
fn asset_commands_require_acknowledgement_and_reject_unknown_targets_and_paths() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let ticket = invoke(
        &main,
        "subscribe_task_changes",
        json!({"onChange":"__CHANNEL__:77"}),
    )
    .unwrap();
    let job = json!({"subscriptionId":ticket["subscriptionId"],"selectionId":"1","jobId":1,"attempt":1,"expectedState":"succeeded"});
    assert_eq!(
        invoke(&main, "get_task_thumbnail", json!({"request":job})).unwrap_err(),
        json!({"code":"session_unavailable"})
    );
    assert_eq!(
        invoke(
            &main,
            "reveal_task_file",
            json!({"request":{"job":job,"target":"result"}})
        )
        .unwrap_err(),
        json!({"code":"session_unavailable"})
    );
    assert!(
        invoke(
            &main,
            "reveal_task_file",
            json!({"request":{"job":job,"target":"temporary"}})
        )
        .is_err()
    );
    assert!(
        invoke(
            &main,
            "reveal_task_file",
            json!({"request":{"job":job,"target":"result","path":"private-path"}})
        )
        .is_err()
    );
}
