//! 使用Tauri mock运行真实命令路由与生成权限；不创建原生GUI。

use crate::{
    lifecycle::DesktopTasks,
    tasks::{TaskConfig, TaskRuntime},
};
use serde_json::{Value, json};
use tauri::{
    Manager,
    test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};

fn app(context: tauri::Context<MockRuntime>) -> tauri::App<MockRuntime> {
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    super::register(mock_builder().manage(DesktopTasks::new(runtime).unwrap()))
        .build(context)
        .unwrap()
}

fn window(app: &tauri::App<MockRuntime>, label: &str) -> tauri::WebviewWindow<MockRuntime> {
    tauri::WebviewWindowBuilder::new(app, label, Default::default())
        .build()
        .unwrap()
}

fn snapshot_request() -> Value {
    json!({"request": {"expectedRevision": null, "collection": "jobs", "offset": 0, "limit": 100}})
}

fn invoke(
    window: &tauri::WebviewWindow<MockRuntime>,
    command: &str,
    body: Value,
) -> Result<Value, Value> {
    // 来源跟随实际配置/平台，不把Windows打包协议当作所有平台的本地页面。
    invoke_at(window, command, body, window.url().unwrap())
}

fn invoke_at(
    window: &tauri::WebviewWindow<MockRuntime>,
    command: &str,
    body: Value,
    origin: tauri::Url,
) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: origin,
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|response| response.deserialize().unwrap())
}

fn assert_permission_denied(response: Result<Value, Value>, command: &str) {
    let error = response.unwrap_err();
    assert!(
        error.as_str().is_some_and(|message| {
            // Tauri在debug带权限诊断，release仅返回短错误；二者都不是业务失败。
            message.starts_with(&format!("{command} not allowed on window"))
                || message == format!("Command {command} not allowed by ACL")
        }),
        "应由ACL拒绝，而非反序列化或业务错误：{error}"
    );
}

fn assert_local_queries_and_permissions(app: &tauri::App<MockRuntime>) {
    let main = window(app, "main");
    let other = window(app, "other");
    let before = app.state::<DesktopTasks>().control.snapshot();
    assert_eq!(
        invoke(&main, "get_app_info", json!({})).unwrap(),
        serde_json::to_value(pixofold_core::app_info()).unwrap()
    );
    assert_eq!(
        invoke(&main, "get_task_snapshot", snapshot_request()).unwrap()["phase"],
        "idle"
    );
    let ticket = invoke(
        &main,
        "subscribe_task_changes",
        json!({"onChange":"__CHANNEL__:42"}),
    )
    .unwrap();
    assert_eq!(ticket["protocolVersion"], 1);
    let ack = json!({"request": {"subscriptionId": ticket["subscriptionId"], "revision": ticket["revision"]}});
    assert_eq!(
        invoke(&main, "acknowledge_task_changes", ack.clone()).unwrap(),
        Value::Null
    );
    assert_eq!(
        invoke(&main, "acknowledge_task_changes", ack).unwrap(),
        Value::Null
    );
    let invalid_ack = json!({"request": {"subscriptionId": ticket["subscriptionId"], "revision": "18446744073709551615"}});
    assert_eq!(
        invoke(&main, "acknowledge_task_changes", invalid_ack).unwrap_err(),
        json!({"code":"invalid_acknowledgement"})
    );
    let close = json!({"request": {"subscriptionId": ticket["subscriptionId"]}});
    assert_eq!(
        invoke(&main, "unsubscribe_task_changes", close.clone()).unwrap(),
        true
    );
    assert_eq!(
        invoke(&main, "unsubscribe_task_changes", close).unwrap(),
        false
    );
    for (command, body) in [
        ("get_app_info", json!({})),
        ("get_task_snapshot", snapshot_request()),
        (
            "subscribe_task_changes",
            json!({"onChange":"__CHANNEL__:43"}),
        ),
        (
            "acknowledge_task_changes",
            json!({"request":{"subscriptionId":"0", "revision":"0"}}),
        ),
        (
            "unsubscribe_task_changes",
            json!({"request":{"subscriptionId":"0"}}),
        ),
        (
            "select_native_import",
            json!({"request":{"subscriptionId":"0", "kind":"files"}}),
        ),
        (
            "apply_task_mutation",
            json!({"request":{"subscriptionId":"0", "operation":{"kind":"cancel", "selectionId":"1"}}}),
        ),
    ] {
        assert_permission_denied(invoke(&other, command, body.clone()), command);
        // 只构造IPC来源，不发起网络请求；远程页面/相似域名均不能继承main权限。
        for origin in [
            "https://example.invalid",
            "http://tauri.localhost.example.invalid",
            "tauri://example.invalid",
        ] {
            assert_permission_denied(
                invoke_at(&main, command, body.clone(), origin.parse().unwrap()),
                command,
            );
        }
    }
    let after = app.state::<DesktopTasks>().control.snapshot();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.phase, before.phase);
    assert!(after.selection.is_none());
    // 生产确实装配dialog插件，但前端无权绕过受控入口获取路径。
    for command in ["plugin:dialog|open", "plugin:dialog|save"] {
        assert!(invoke(&main, command, json!({"options":{}})).is_err());
    }
}

fn connected(app: &tauri::App<MockRuntime>) -> crate::ipc::DecimalU64 {
    let tasks = app.state::<DesktopTasks>();
    let ticket = tasks
        .subscriptions
        .subscribe(tauri::ipc::Channel::new(|_| Ok(())))
        .unwrap();
    // 测试观察器也完成真实查询/ACK握手，但不依赖不存在的mock JS Channel消费者。
    let request = serde_json::from_value(snapshot_request()["request"].clone()).unwrap();
    crate::ipc::query(&tasks.control, request).unwrap();
    tasks
        .subscriptions
        .acknowledge(crate::ipc::TaskChangeAck {
            subscription_id: ticket.subscription_id,
            revision: ticket.revision,
        })
        .unwrap();
    ticket.subscription_id
}
fn mutate(
    window: &tauri::WebviewWindow<MockRuntime>,
    session: crate::ipc::DecimalU64,
    operation: Value,
) -> Result<Value, Value> {
    invoke(
        window,
        "apply_task_mutation",
        json!({"request":{"subscriptionId":session, "operation":operation}}),
    )
}
fn phase(
    control: &crate::tasks::TaskControl,
    phase: crate::tasks::TaskPhase,
) -> crate::tasks::TaskSnapshot {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut view = control.snapshot();
    while view.phase != phase {
        view = control
            .wait_for_change(
                view.revision,
                deadline.saturating_duration_since(std::time::Instant::now()),
            )
            .unwrap();
    }
    view
}
fn native_grant(
    app: &tauri::App<MockRuntime>,
    session: crate::ipc::DecimalU64,
    paths: Vec<std::path::PathBuf>,
) -> Value {
    serde_json::to_value(
        app.state::<DesktopTasks>()
            .imports
            .reserve(session)
            .unwrap()
            .complete(Some(paths))
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn native_and_mutation_commands_require_acknowledged_current_subscription() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let tasks = app.state::<DesktopTasks>();
    let ticket = tasks
        .subscriptions
        .subscribe(tauri::ipc::Channel::new(|_| Ok(())))
        .unwrap();
    let before = tasks.control.snapshot().revision;
    for (command, body) in [
        (
            "select_native_import",
            json!({"request":{"subscriptionId":ticket.subscription_id, "kind":"files"}}),
        ),
        (
            "apply_task_mutation",
            json!({"request":{"subscriptionId":ticket.subscription_id, "operation":{"kind":"import", "grantId":"1", "settings":null}}}),
        ),
    ] {
        assert_eq!(
            invoke(&main, command, body).unwrap_err(),
            json!({"code":"subscription", "error":{"code":"invalid_acknowledgement"}})
        );
    }
    connected(&app);
    assert_eq!(
        mutate(
            &main,
            ticket.subscription_id,
            json!({"kind":"cancel", "selectionId":"1"})
        )
        .unwrap_err(),
        json!({"code":"subscription", "error":{"code":"stale_subscription"}})
    );
    assert_eq!(tasks.control.snapshot().revision, before);
}

#[test]
fn strict_mutation_contract_rejects_paths_quality_and_malformed_identifiers() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let session = connected(&app);
    let before = app.state::<DesktopTasks>().control.snapshot().revision;
    for operation in [
        json!({"kind":"import", "grantId":"1", "settings":null, "paths":["private.png"]}),
        json!({"kind":"cancel", "selectionId":"01"}),
        json!({"kind":"cancel", "selectionId":1}),
        json!({"kind":"start", "selectionId":"1", "settings":{"mode":{"kind":"lossy", "quality":101}, "output":"overwrite"}}),
        json!({"kind":"start", "selectionId":"1", "settings":{"mode":{"kind":"lossless", "quality":80}, "output":"overwrite"}}),
        json!({"kind":"start", "selectionId":"1", "settings":{"mode":{"kind":"lossless"}, "output":"copy_to", "directory":"private"}}),
        json!({"kind":"retry", "selectionId":"1", "expectedBatchRevision":"0", "jobIds":[0], "mode":{"kind":"lossless"}, "output":"private.png"}),
    ] {
        assert!(mutate(&main, session, operation).unwrap_err().is_string());
    }
    assert!(
        invoke(
            &main,
            "select_native_import",
            json!({"request":{"subscriptionId":session, "kind":"files", "path":"private"}})
        )
        .unwrap_err()
        .is_string()
    );
    assert_eq!(
        app.state::<DesktopTasks>().control.snapshot().revision,
        before
    );
}

#[test]
fn native_slot_is_bounded_even_after_reconnection_and_stale_result_cannot_be_granted() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let session = connected(&app);
    let tasks = app.state::<DesktopTasks>();
    let pending = tasks.imports.reserve(session).unwrap();
    let next = connected(&app);
    assert_eq!(
        invoke(
            &main,
            "select_native_import",
            json!({"request":{"subscriptionId":next, "kind":"folder"}})
        )
        .unwrap_err(),
        json!({"code":"selection_busy"})
    );
    // 与真实原生回调同样在当前订阅锁下完成，不允许旧响应越过新会话。
    assert!(
        tasks
            .subscriptions
            .with_ready(session, || pending
                .complete(Some(vec![std::env::temp_dir()])))
            .is_err()
    );
    assert!(tasks.imports.reserve(next).is_ok());
}

#[test]
fn authorized_scan_start_cancel_clear_and_reimport_share_one_task_owner() {
    use crate::tasks::TaskPhase;
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let session = connected(&app);
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("image.png");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png/rgb8.png"),
        &source,
    )
    .unwrap();
    let original = std::fs::read(&source).unwrap();
    let tasks = app.state::<DesktopTasks>();
    let grant = native_grant(&app, session, vec![source.clone()]);
    let import = json!({"kind":"import", "grantId":grant["grantId"], "settings":null});
    let accepted = mutate(&main, session, import.clone()).unwrap();
    let id = &accepted["selectionId"];
    assert_eq!(
        mutate(&main, session, import).unwrap_err(),
        json!({"code":"stale_grant"})
    );
    phase(&tasks.control, TaskPhase::Ready);
    let second = native_grant(&app, session, vec![source.clone()]);
    assert_eq!(
        mutate(
            &main,
            session,
            json!({"kind":"import", "grantId":second["grantId"], "settings":null})
        )
        .unwrap_err(),
        json!({"code":"task", "error":{"code":"busy"}})
    );
    assert_eq!(
        invoke(
            &main,
            "select_native_import",
            json!({"request":{"subscriptionId":session, "kind":"files"}})
        )
        .unwrap_err(),
        json!({"code":"task", "error":{"code":"busy"}})
    );
    mutate(&main, session, json!({"kind":"cancel", "selectionId":id})).unwrap();
    assert_eq!(tasks.control.snapshot().phase, TaskPhase::Cancelled);
    mutate(&main, session, json!({"kind":"clear", "selectionId":id})).unwrap();
    phase(&tasks.control, TaskPhase::Idle);
    let accepted = mutate(
        &main,
        session,
        json!({"kind":"import", "grantId":second["grantId"], "settings":null}),
    )
    .unwrap();
    phase(&tasks.control, TaskPhase::Ready);
    assert_eq!(
        mutate(&main, session, json!({"kind":"cancel", "selectionId":id})).unwrap_err(),
        json!({"code":"task", "error":{"code":"stale_selection"}})
    );
    let start = json!({"kind":"start", "selectionId":accepted["selectionId"], "settings":{"mode":{"kind":"lossless"}, "output":"copy_beside"}});
    mutate(&main, session, start.clone()).unwrap();
    assert_eq!(
        mutate(&main, session, start).unwrap_err(),
        json!({"code":"task", "error":{"code":"not_ready"}})
    );
    let finished = phase(&tasks.control, TaskPhase::Finished);
    assert_eq!(finished.batch.unwrap().summary.failed, 0);
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert!(dir.path().join("image_compressed.png").exists());
}

#[test]
fn mutation_retry_preserves_success_backups_and_checks_batch_revision_and_row_set() {
    use crate::tasks::TaskPhase;
    use pixofold_core::{batch::JobState, model::ProcessingOutcome};
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let session = connected(&app);
    let dir = tempfile::tempdir().unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png");
    let a = dir.path().join("a.png");
    let b = dir.path().join("b.png");
    std::fs::copy(fixture.join("rgb8.png"), &a).unwrap();
    std::fs::copy(fixture.join("bad-deflate.png"), &b).unwrap();
    let original = std::fs::read(&a).unwrap();
    let invalid = std::fs::read(&b).unwrap();
    let grant = native_grant(&app, session, vec![a.clone(), b.clone()]);
    let accepted = mutate(&main, session, json!({"kind":"import", "grantId":grant["grantId"], "settings":{"mode":{"kind":"lossless"}, "output":"overwrite"}})).unwrap();
    let tasks = app.state::<DesktopTasks>();
    let first = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
    assert_eq!((first.summary.succeeded, first.summary.failed), (1, 1));
    assert_eq!(std::fs::read(&b).unwrap(), invalid);
    let JobState::Succeeded(report) = &first.jobs[0].state else {
        panic!("a should succeed")
    };
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = &report.outcome
    else {
        panic!("overwrite must preserve backup")
    };
    assert_eq!(std::fs::read(backup).unwrap(), original);
    let optimized = std::fs::read(&a).unwrap();
    let failed_id = first.jobs[1].id.get();
    let succeeded_id = first.jobs[0].id.get();
    let retry = json!({"kind":"retry", "selectionId":accepted["selectionId"], "expectedBatchRevision":first.revision.to_string(), "jobIds":[failed_id], "mode":{"kind":"lossless"}});
    let revision = tasks.control.snapshot().revision;
    for ids in [
        json!([]),
        json!([0]),
        json!([succeeded_id]),
        json!([failed_id, failed_id]),
        json!([99]),
        json!(vec![failed_id; 1001]),
    ] {
        let mut invalid = retry.clone();
        invalid["jobIds"] = ids;
        assert_eq!(
            mutate(&main, session, invalid).unwrap_err(),
            json!({"code":"invalid_retry"})
        );
    }
    let mut stale = retry.clone();
    stale["expectedBatchRevision"] = json!((first.revision - 1).to_string());
    assert_eq!(
        mutate(&main, session, stale).unwrap_err(),
        json!({"code":"task", "error":{"code":"stale_batch"}})
    );
    assert_eq!(tasks.control.snapshot().revision, revision);
    std::fs::copy(fixture.join("rgba8.png"), &b).unwrap();
    mutate(&main, session, retry.clone()).unwrap();
    let next = phase(&tasks.control, TaskPhase::Finished).batch.unwrap();
    assert_eq!(next.summary.succeeded, 2);
    assert_eq!((next.jobs[0].attempt, next.jobs[1].attempt), (1, 2));
    assert_eq!(std::fs::read(a).unwrap(), optimized);
    assert_eq!(std::fs::read(backup).unwrap(), original);
    assert_eq!(
        mutate(&main, session, retry).unwrap_err(),
        json!({"code":"task", "error":{"code":"stale_batch"}})
    );
    mutate(
        &main,
        session,
        json!({"kind":"clear", "selectionId":accepted["selectionId"]}),
    )
    .unwrap();
    phase(&tasks.control, TaskPhase::Idle);
    assert_eq!(std::fs::read(backup).unwrap(), original);
}

#[test]
fn corrected_settings_reuse_frozen_scan_and_old_session_cannot_consume_new_grant() {
    use crate::tasks::TaskPhase;
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let first = connected(&app);
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("image.png");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png/rgb8.png"),
        &source,
    )
    .unwrap();
    let original = std::fs::read(&source).unwrap();
    std::fs::write(dir.path().join("image_compressed.png"), b"existing target").unwrap();
    let old_grant = native_grant(&app, first, vec![source.clone()]);
    let current = connected(&app);
    assert_eq!(
        mutate(
            &main,
            current,
            json!({"kind":"import", "grantId":old_grant["grantId"], "settings":null})
        )
        .unwrap_err(),
        json!({"code":"stale_grant"})
    );
    let grant = native_grant(&app, current, vec![source.clone()]);
    let accepted = mutate(&main, current, json!({"kind":"import", "grantId":grant["grantId"], "settings":{"mode":{"kind":"lossless"}, "output":"copy_beside"}})).unwrap();
    let tasks = app.state::<DesktopTasks>();
    let ready = phase(&tasks.control, TaskPhase::Ready);
    assert!(ready.error.is_some());
    assert!(ready.batch.is_none());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    mutate(&main, current, json!({"kind":"start", "selectionId":accepted["selectionId"], "settings":{"mode":{"kind":"lossless"}, "output":"overwrite"}})).unwrap();
    let finished = phase(&tasks.control, TaskPhase::Finished);
    assert!(std::sync::Arc::ptr_eq(
        ready.import.as_ref().unwrap(),
        finished.import.as_ref().unwrap()
    ));
    assert_eq!(finished.batch.unwrap().summary.succeeded, 1);
    assert_eq!(
        std::fs::read(dir.path().join("image_compressed.png")).unwrap(),
        b"existing target"
    );
}

#[test]
fn queries_use_configured_app_origin_and_main_only_permissions() {
    let app = app(super::super::app_context());
    // 默认测试构建覆盖实际devUrl；custom-protocol构建仍由Tauri选择本地协议。
    if tauri::is_dev() {
        let probe = window(&app, "origin-probe");
        assert_eq!(Some(probe.url().unwrap()), app.config().build.dev_url);
    }
    assert_local_queries_and_permissions(&app);
}

#[test]
fn queries_use_platform_packaged_origin_and_main_only_permissions() {
    let mut context = super::super::app_context();
    // 只移除测试上下文的开发地址以覆盖打包协议，不修改真实capability/权限。
    context.config_mut().build.dev_url = None;
    let app = app(context);
    let probe = window(&app, "origin-probe");
    let url = probe.url().unwrap();
    if cfg!(windows) || cfg!(target_os = "android") {
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("tauri.localhost"));
    } else {
        assert_eq!(url.scheme(), "tauri");
        assert_eq!(url.host_str(), Some("localhost"));
    }
    assert_local_queries_and_permissions(&app);
}

#[test]
fn snapshot_command_rejects_invalid_requests_without_mutating_tasks() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let request = snapshot_request();
    let before = app.state::<DesktopTasks>().control.snapshot();
    let mut invalid = request.clone();
    invalid["request"]["limit"] = json!(101);
    assert_eq!(
        invoke(&main, "get_task_snapshot", invalid).unwrap_err(),
        json!({"code": "invalid_page"})
    );
    let mut invalid = request;
    invalid["request"]["path"] = json!("arbitrary.png");
    let error = invoke(&main, "get_task_snapshot", invalid).unwrap_err();
    assert!(
        error
            .as_str()
            .is_some_and(|message| message.contains("unknown field `path`")),
        "额外路径应在反序列化边界被拒绝：{error}"
    );
    let after = app.state::<DesktopTasks>().control.snapshot();
    assert_eq!(after.revision, before.revision);
    assert!(after.selection.is_none());
}

#[test]
fn snapshot_command_observes_the_existing_application_owner() {
    let app = app(super::super::app_context());
    let main = window(&app, "main");
    let before = invoke(&main, "get_task_snapshot", snapshot_request()).unwrap();
    assert_eq!(before["phase"], "idle");
    app.state::<DesktopTasks>().control.request_close();
    // 协调线程可已收尾，也可仍在关闭；两种情况都必须读取原所有者而非新建空闲服务。
    let after = invoke(&main, "get_task_snapshot", snapshot_request()).unwrap();
    assert!(matches!(
        after["phase"].as_str(),
        Some("closing" | "closed")
    ));
    assert_ne!(after["revision"], before["revision"]);
}
