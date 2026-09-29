//! 真实任务/输出经生产IPC路由；原生选择结果用Rust注入，不冒充OS对话框验收。
use super::tests::{
    app, connected, invoke, mutate, native_grant, output_grant as choose, phase, window,
};
use crate::{lifecycle::DesktopTasks, tasks::TaskPhase};
use pixofold_core::batch::{JobErrorCode, JobState};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tauri::Manager;
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
            "中文 photos/a.png"
        } else {
            "a.png"
        });
        assert!(a_output.is_file());
        assert_eq!(fs::read_dir(&photos).unwrap().count(), 3);
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
                    "中文 photos/sub/credential.png"
                } else {
                    "credential.png"
                })
                .exists()
        );
        assert!(!out.join("credential.png").exists());
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
                "中文 photos/bad.png"
            } else {
                "bad.png"
            })
            .exists()
        );
        assert!(!other.join("bad.png").exists());
    }
}

#[test]
fn existing_targets_fail_per_image_in_import_confirmation_and_retry() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let session = connected(&app);
    let dir = tempfile::tempdir().unwrap();
    let names = ["a.png", "b.png", "c.png", "d.png"];
    let sources: Vec<_> = names.iter().map(|name| dir.path().join(name)).collect();
    for (i, source) in sources.iter().enumerate() {
        fs::write(
            source,
            fixture(if i < 2 {
                "rgb8.png"
            } else {
                "content-credentials.png"
            }),
        )
        .unwrap();
    }
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("a.png"), b"never overwrite").unwrap();
    let selected = choose(&app, session, &out);
    let import = native_grant(&app, session, sources.clone());
    let accepted = mutate(
        &main,
        session,
        json!({"kind":"import","grantId":import["grantId"],"settings":{"mode":{"kind":"lossless"},"output":target(&selected,false)}}),
    )
    .unwrap();
    let tasks = app.state::<DesktopTasks>();
    let first = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
    assert_eq!((first.summary.failed, first.summary.succeeded), (3, 1));
    assert!(
        matches!(&first.jobs[0].state, JobState::Failed(f) if f.code == JobErrorCode::TargetConflict)
    );
    assert_eq!(fs::read(out.join("a.png")).unwrap(), b"never overwrite");
    let b_output = fs::read(out.join("b.png")).unwrap();
    assert!(
        first.jobs[2..]
            .iter()
            .all(|job| job.content_credentials_source().is_some())
    );
    let confirm = |revision: u64, ids: Vec<usize>| json!({"kind":"confirm_content_credentials","selectionId":accepted["selectionId"],"expectedBatchRevision":revision.to_string(),"jobIds":ids,"mode":{"kind":"lossless"},"output":target(&selected,false),"consent":"remove_content_credentials"});
    // 弹窗打开后目标出现：仅c失败，d仍可按确认处理。
    fs::write(out.join("c.png"), b"late target").unwrap();
    mutate(
        &main,
        session,
        confirm(
            first.revision,
            vec![first.jobs[2].id.get(), first.jobs[3].id.get()],
        ),
    )
    .unwrap();
    let second = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
    assert_eq!((second.summary.failed, second.summary.succeeded), (2, 2));
    assert!(
        matches!(&second.jobs[2].state, JobState::Failed(f) if f.code == JobErrorCode::TargetConflict)
    );
    assert_eq!(fs::read(out.join("c.png")).unwrap(), b"late target");
    let d_output = fs::read(out.join("d.png")).unwrap();
    fs::remove_file(out.join("a.png")).unwrap();
    fs::remove_file(out.join("c.png")).unwrap();
    mutate(&main, session, json!({"kind":"retry","selectionId":accepted["selectionId"],"expectedBatchRevision":second.revision.to_string(),"jobIds":[second.jobs[0].id.get(),second.jobs[2].id.get()],"mode":{"kind":"lossless"}})).unwrap();
    let third = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
    assert_eq!((third.summary.failed, third.summary.succeeded), (1, 3));
    assert!(
        third.jobs[2].content_credentials_source().is_some(),
        "普通重试不继承移除许可"
    );
    mutate(
        &main,
        session,
        confirm(third.revision, vec![third.jobs[2].id.get()]),
    )
    .unwrap();
    let last = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
    assert_eq!(last.summary.succeeded, 4);
    assert_eq!((last.jobs[1].attempt, last.jobs[3].attempt), (1, 2));
    assert_eq!(fs::read(out.join("b.png")).unwrap(), b_output);
    assert_eq!(fs::read(out.join("d.png")).unwrap(), d_output);
    assert_eq!(fs::read_dir(out).unwrap().count(), 4);
    for (i, source) in sources.iter().enumerate() {
        assert_eq!(
            fs::read(source).unwrap(),
            fixture(if i < 2 {
                "rgb8.png"
            } else {
                "content-credentials.png"
            })
        );
    }
}

#[test]
fn original_folder_copies_fail_without_overwriting_even_after_credentials_consent() {
    for credentials in [false, true] {
        let app = app(super::super::app_context());
        let main = window(&app, "main");
        let session = connected(&app);
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.png");
        let original = fixture(if credentials {
            "content-credentials.png"
        } else {
            "rgb8.png"
        });
        fs::write(&source, &original).unwrap();
        let grant = native_grant(&app, session, vec![source.clone()]);
        let accepted = mutate(&main, session, json!({"kind":"import","grantId":grant["grantId"],"settings":{"mode":{"kind":"lossless"},"output":if credentials { "overwrite" } else { "copy_beside" }}})).unwrap();
        let tasks = app.state::<DesktopTasks>();
        let mut done = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
        if credentials {
            assert!(done.jobs[0].content_credentials_source().is_some());
            mutate(&main, session, json!({"kind":"confirm_content_credentials","selectionId":accepted["selectionId"],"expectedBatchRevision":done.revision.to_string(),"jobIds":[done.jobs[0].id.get()],"mode":{"kind":"lossless"},"output":"copy_beside","consent":"remove_content_credentials"})).unwrap();
            done = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
        }
        assert_eq!(done.summary.failed, 1);
        assert!(
            matches!(&done.jobs[0].state, JobState::Failed(f) if f.code == JobErrorCode::TargetConflict)
        );
        assert_eq!(fs::read(source).unwrap(), original);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
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
