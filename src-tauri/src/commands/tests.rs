//! 使用Tauri mock运行真实命令路由与生成权限；不创建原生GUI。

use crate::{
    lifecycle::DesktopTasks,
    tasks::{TaskConfig, TaskRuntime},
};
use serde_json::{Value, json};
use tauri::{
    Manager,
    test::{INVOKE_KEY, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};

fn invoke(
    window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    body: Value,
) -> Result<Value, Value> {
    invoke_at(window, body, "http://tauri.localhost")
}

fn invoke_at(
    window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    body: Value,
    origin: &str,
) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: "get_task_snapshot".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: origin.parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|response| response.deserialize().unwrap())
}

#[test]
fn snapshot_command_uses_app_owner_and_main_only_permission() {
    let runtime = TaskRuntime::new(TaskConfig::default()).unwrap();
    let app = mock_builder()
        .manage(DesktopTasks::new(runtime))
        .invoke_handler(tauri::generate_handler![
            super::get_app_info,
            super::get_task_snapshot
        ])
        .build(tauri::generate_context!())
        .unwrap();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let other = tauri::WebviewWindowBuilder::new(&app, "other", Default::default())
        .build()
        .unwrap();
    let request = json!({"request": {"expectedRevision": null, "collection": "jobs", "offset": 0, "limit": 100}});
    assert_eq!(invoke(&main, request.clone()).unwrap()["phase"], "idle");
    assert!(invoke(&other, request.clone()).is_err());
    // 只更换IPC请求来源，无网络访问；远程页面不能沿用main窗口的本地权限。
    assert!(invoke_at(&main, request.clone(), "https://example.invalid").is_err());
    let mut invalid = request.clone();
    invalid["request"]["limit"] = json!(101);
    assert_eq!(
        invoke(&main, invalid).unwrap_err(),
        json!({"code": "invalid_page"})
    );
    let mut invalid = request;
    invalid["request"]["path"] = json!("arbitrary.png");
    assert!(invoke(&main, invalid).is_err());
    assert!(
        app.state::<DesktopTasks>()
            .control
            .snapshot()
            .selection
            .is_none()
    );
}
