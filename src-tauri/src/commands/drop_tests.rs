//! 从生产注册的窗口分发函数进入真实接纳/扫描/压缩；mock不覆盖OS事件投递。
use super::super::handle_window_event;
use super::tests::{app, invoke, mutate, phase, window};
use crate::{
    ipc::{DecimalU64, MutationError, TaskChangeAck},
    lifecycle::DesktopTasks,
    tasks::TaskPhase,
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::mpsc, time::Duration};
use tauri::{
    DragDropEvent, Manager, PhysicalPosition, WindowEvent, ipc::Channel, test::MockRuntime,
};

fn connect(app: &tauri::App<MockRuntime>, ready: bool) -> (DecimalU64, mpsc::Receiver<Value>) {
    let (send, recv) = mpsc::channel();
    let tasks = app.state::<DesktopTasks>();
    let ticket = tasks
        .subscriptions
        .subscribe(Channel::new(move |body| {
            send.send(body.deserialize()?)
                .map_err(|_| tauri::Error::Io(std::io::Error::other("test receiver closed")))
        }))
        .unwrap();
    if ready {
        tasks
            .subscriptions
            .acknowledge(TaskChangeAck {
                subscription_id: ticket.subscription_id,
                revision: ticket.revision,
            })
            .unwrap();
    }
    (ticket.subscription_id, recv)
}
fn dispatch(app: &tauri::App<MockRuntime>, label: &str, event: WindowEvent) {
    let window = app.get_webview_window(label).unwrap();
    handle_window_event(&window.as_ref().window(), &event);
}
fn enter(app: &tauri::App<MockRuntime>, label: &str) {
    dispatch(
        app,
        label,
        WindowEvent::DragDrop(DragDropEvent::Enter {
            paths: vec![],
            position: PhysicalPosition::new(240.0, 160.0),
        }),
    );
}
fn drop_files(app: &tauri::App<MockRuntime>, paths: Vec<PathBuf>) {
    dispatch(
        app,
        "main",
        WindowEvent::DragDrop(DragDropEvent::Drop {
            paths,
            position: PhysicalPosition::new(300.0, 220.0),
        }),
    );
}
fn receive(recv: &mpsc::Receiver<Value>) -> Value {
    recv.recv_timeout(Duration::from_secs(5)).unwrap()
}

#[test]
fn native_window_dispatch_ignores_unrelated_events_and_other_windows() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let _other = window(&app, "other");
    let (session, recv) = connect(&app, true);
    let tasks = app.state::<DesktopTasks>();
    let before = tasks.control.snapshot().revision;
    let over = || {
        WindowEvent::DragDrop(DragDropEvent::Over {
            position: PhysicalPosition::new(280.0, 200.0),
        })
    };
    // Over不能替代Enter取得授权；普通窗口事件也不能创建手势。
    dispatch(&app, "main", WindowEvent::Focused(true));
    dispatch(&app, "main", over());
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(recv.try_recv().is_err());

    enter(&app, "main");
    enter(&app, "other");
    dispatch(&app, "other", WindowEvent::DragDrop(DragDropEvent::Leave));
    dispatch(
        &app,
        "other",
        WindowEvent::DragDrop(DragDropEvent::Drop {
            paths: vec![std::env::temp_dir()],
            position: PhysicalPosition::new(300.0, 220.0),
        }),
    );
    dispatch(&app, "main", WindowEvent::Focused(false));
    dispatch(&app, "main", over());
    assert!(recv.try_recv().is_err());
    // 其他窗口不得消费或撤销main的手势，移动事件也不产生高频通知。
    drop_files(&app, vec![std::env::temp_dir()]);
    let offer = receive(&recv);
    assert_eq!(offer["offer"]["grant"]["rootCount"], 1);
    dispatch(&app, "main", WindowEvent::DragDrop(DragDropEvent::Leave));
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(recv.try_recv().is_err());
    assert_eq!(tasks.control.snapshot().revision, before);
    assert!(matches!(
        tasks.imports.begin_drag(session),
        Err(MutationError::SelectionBusy)
    ));
    invoke(
        &main,
        "release_native_drop",
        json!({"request":{"subscriptionId":session,"offerId":offer["offer"]["offerId"]}}),
    )
    .unwrap();
    assert!(tasks.imports.begin_drag(session).is_ok());
}

#[test]
fn native_drop_uses_one_pathless_grant_and_the_real_mixed_directory_pipeline() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let (session, recv) = connect(&app, true);
    let tasks = app.state::<DesktopTasks>();
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("中文子目录");
    std::fs::create_dir(&nested).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png");
    let mut sources = Vec::new();
    for file in ["rgb8.png", "rgba8.png", "content-credentials.png"] {
        let path = nested.join(file);
        std::fs::copy(fixture.join(file), &path).unwrap();
        sources.push((path.clone(), std::fs::read(path).unwrap()));
    }
    std::fs::write(nested.join("note.txt"), b"not an image").unwrap();
    let before = tasks.control.snapshot().revision;
    enter(&app, "main");
    drop_files(&app, vec![dir.path().into(), sources[0].0.clone()]);
    let offer = receive(&recv);
    assert_eq!(offer["kind"], "native_drop");
    assert_eq!(
        offer["subscriptionId"],
        serde_json::to_value(session).unwrap()
    );
    assert_eq!(offer["position"], json!({"x":300.0,"y":220.0}));
    assert_eq!(offer["offer"]["grant"]["rootCount"], 2);
    assert!(!offer.to_string().contains("rgb8"));
    assert!(
        !offer
            .to_string()
            .contains(&dir.path().to_string_lossy().to_string())
    );
    assert_eq!(tasks.control.snapshot().revision, before);
    // 未决票据与重复Drop不增加消息，真实消费以前无文件处理。
    enter(&app, "main");
    drop_files(&app, vec![dir.path().into()]);
    assert!(recv.try_recv().is_err());
    let import = json!({"kind":"import", "grantId":offer["offer"]["grant"]["grantId"], "settings":{"mode":{"kind":"lossless"},"output":"copy_beside"}});
    mutate(&main, session, import.clone()).unwrap();
    assert_eq!(
        mutate(&main, session, import).unwrap_err()["code"],
        "stale_grant"
    );
    let finished = phase(&tasks.control, TaskPhase::Finished);
    let batch = finished.batch.unwrap();
    assert_eq!(batch.summary.total, 3);
    assert_eq!(batch.summary.succeeded, 2);
    assert_eq!(batch.summary.failed, 1);
    assert_eq!(
        batch
            .jobs
            .iter()
            .filter(|job| job.content_credentials_source().is_some())
            .count(),
        1
    );
    assert!(finished.import.unwrap().progress().duplicates > 0);
    for (path, bytes) in sources {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(nested.join("rgb8_compressed.png").exists());
    assert!(nested.join("rgba8_compressed.png").exists());
    assert_eq!(std::fs::read_dir(&nested).unwrap().count(), 6);
    assert_eq!(
        invoke(
            &main,
            "release_native_drop",
            json!({"request":{"subscriptionId":session, "offerId":offer["offer"]["offerId"]}})
        )
        .unwrap(),
        Value::Null
    );
}

#[test]
fn native_drop_handshake_window_reload_dialog_and_busy_gates_prevent_admission() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let _other = window(&app, "other");
    let tasks = app.state::<DesktopTasks>();
    let (_, unready) = connect(&app, false);
    enter(&app, "main");
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(unready.try_recv().is_err());
    let (session, recv) = connect(&app, true);
    enter(&app, "other");
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(recv.try_recv().is_err());
    enter(&app, "main");
    dispatch(&app, "main", WindowEvent::DragDrop(DragDropEvent::Leave));
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(recv.try_recv().is_err());
    enter(&app, "main");
    let (replacement, next) = connect(&app, true);
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(next.try_recv().is_err());
    let dialog = tasks.imports.reserve(replacement).unwrap();
    // 旧会话手势不能阻塞新页面选择；不同于必须等真实关闭的物理对话框。
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(next.try_recv().is_err());
    drop(dialog);
    enter(&app, "main");
    tasks.page_load("main", tauri::webview::PageLoadEvent::Started);
    let (current, recv) = connect(&app, true);
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(recv.try_recv().is_err());
    let dialog = tasks.imports.reserve(current).unwrap();
    enter(&app, "main");
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(recv.try_recv().is_err());
    drop(dialog);
    assert_eq!(
        invoke(
            &main,
            "release_native_drop",
            json!({"request":{"subscriptionId":session,"offerId":"1"}})
        )
        .unwrap_err()["code"],
        "subscription"
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png/rgb8.png"),
        dir.path().join("input.png"),
    )
    .unwrap();
    enter(&app, "main");
    drop_files(&app, vec![dir.path().into()]);
    let offer = receive(&recv);
    mutate(
        &main,
        current,
        json!({"kind":"import","grantId":offer["offer"]["offerId"],"settings":null}),
    )
    .unwrap();
    phase(&tasks.control, TaskPhase::Ready);
    while recv.try_recv().is_ok() {} // 丢弃正常任务通知，以下只断言无新拖放授权。
    enter(&app, "main");
    drop_files(&app, vec![dir.path().into()]);
    assert!(
        recv.try_iter()
            .all(|message| message["kind"] != "native_drop")
    );
    assert_eq!(tasks.control.snapshot().phase, TaskPhase::Ready);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn invalid_native_drop_is_reported_without_paths_and_release_reopens_the_slot() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let (session, recv) = connect(&app, true);
    let tasks = app.state::<DesktopTasks>();
    enter(&app, "main");
    drop_files(&app, vec![PathBuf::from("relative-private.png")]);
    let offer = receive(&recv);
    assert!(offer["offer"]["grant"].is_null());
    assert!(!offer.to_string().contains("private"));
    assert!(
        mutate(
            &main,
            session,
            json!({"kind":"import","grantId":offer["offer"]["offerId"],"settings":null})
        )
        .is_err()
    );
    let release = json!({"request":{"subscriptionId":session,"offerId":offer["offer"]["offerId"]}});
    invoke(&main, "release_native_drop", release.clone()).unwrap();
    invoke(&main, "release_native_drop", release).unwrap();
    enter(&app, "main");
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(receive(&recv)["offer"]["grant"].is_object());
    assert_eq!(tasks.control.snapshot().phase, TaskPhase::Idle);
}

#[test]
fn failed_native_drop_delivery_releases_only_the_unconsumed_offer() {
    let app = app(super::super::app_context());
    let _main = window(&app, "main");
    let tasks = app.state::<DesktopTasks>();
    let ticket = tasks
        .subscriptions
        .subscribe(Channel::new(|_| {
            Err(tauri::Error::Io(std::io::Error::other(
                "injected delivery failure",
            )))
        }))
        .unwrap();
    tasks
        .subscriptions
        .acknowledge(TaskChangeAck {
            subscription_id: ticket.subscription_id,
            revision: ticket.revision,
        })
        .unwrap();
    enter(&app, "main");
    drop_files(&app, vec![std::env::temp_dir()]);
    assert!(tasks.imports.begin_drag(ticket.subscription_id).is_ok());
    tasks.imports.leave_drag();
    assert_eq!(tasks.control.snapshot().phase, TaskPhase::Idle);
}
