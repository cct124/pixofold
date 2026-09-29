//! 批次目录只读查询/打开，与文件定位共享一个在途许可，不开放任意目录或程序参数。
use crate::{
    assets::{
        self, AssetError, BatchAssetRequest, OpenOutputDirectoryRequest, Operation,
        OutputDirectoriesRequest, OutputDirectoryPage, RevealResult,
    },
    lifecycle::DesktopTasks,
};

#[tauri::command]
pub(crate) async fn get_output_directories(
    tasks: tauri::State<'_, DesktopTasks>,
    request: OutputDirectoriesRequest,
) -> Result<OutputDirectoryPage, AssetError> {
    let (snapshot, permit) = tasks
        .subscriptions
        .with_ready(request.batch.subscription_id, || {
            let snapshot = tasks.control.snapshot();
            assets::validate_batch(&snapshot, &request.batch)?;
            Ok((snapshot, tasks.assets.reserve(Operation::Reveal)?))
        })
        .map_err(|_| AssetError::SessionUnavailable)??;
    let service = tasks.assets.clone();
    let subscriptions = tasks.subscriptions.clone();
    let control = tasks.control.clone();
    let started = std::time::Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let page = assets::directory_page(&snapshot, &request)?;
        subscriptions
            .with_ready(request.batch.subscription_id, || {
                assets::validate_batch(&control.snapshot(), &request.batch)?;
                service.ensure_current(&permit)?;
                Ok(page)
            })
            .map_err(|_| AssetError::SessionUnavailable)?
    })
    .await
    .unwrap_or(Err(AssetError::ServiceFault));
    record(request.batch, "list_output_directories", started, &result);
    result
}

#[tauri::command]
pub(crate) async fn open_output_directory(
    tasks: tauri::State<'_, DesktopTasks>,
    request: OpenOutputDirectoryRequest,
) -> Result<RevealResult, AssetError> {
    let (target, permit) = tasks
        .subscriptions
        .with_ready(request.batch.subscription_id, || {
            Ok((
                assets::resolve_directory(&tasks.control.snapshot(), &request)?,
                tasks.assets.reserve(Operation::Reveal)?,
            ))
        })
        .map_err(|_| AssetError::SessionUnavailable)??;
    let service = tasks.assets.clone();
    let subscriptions = tasks.subscriptions.clone();
    let control = tasks.control.clone();
    let started = std::time::Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        target.validate()?;
        subscriptions
            .with_ready(request.batch.subscription_id, || {
                if assets::resolve_directory(&control.snapshot(), &request)?.path != target.path {
                    return Err(AssetError::StaleTask);
                }
                service.ensure_current(&permit)
            })
            .map_err(|_| AssetError::SessionUnavailable)??;
        // 直接打开已验证目录，而不是reveal其父目录；使用OS原生路径、不做字符串/命令拼接。
        tauri_plugin_opener::open_path(&target.path, None::<&str>)
            .map_err(|_| AssetError::RevealFailed)?;
        Ok(RevealResult::Requested)
    })
    .await
    .unwrap_or(Err(AssetError::ServiceFault));
    record(request.batch, "open_output_directory", started, &result);
    result
}

fn record<T>(
    request: BatchAssetRequest,
    operation: &'static str,
    started: std::time::Instant,
    result: &Result<T, AssetError>,
) {
    tracing::info!(target: "pixofold", event = "asset_request_finished", operation,
        selection_id = request.selection_id.0, batch_id = request.batch_id.0,
        elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        result = if result.is_ok() { "ok" } else { "error" }, error_code = ?result.as_ref().err());
}
