//! 展示命令先绑定当前已ACK会话和权威终态行；I/O与OS调用在有界后台执行。

use crate::{
    assets::{
        self, AssetError, JobAssetRequest, Operation, RevealRequest, RevealResult, ThumbnailDto,
    },
    lifecycle::DesktopTasks,
};

#[tauri::command]
pub(crate) async fn get_task_thumbnail(
    tasks: tauri::State<'_, DesktopTasks>,
    request: JobAssetRequest,
) -> Result<ThumbnailDto, AssetError> {
    let (path, permit) = tasks
        .subscriptions
        .with_ready(request.subscription_id, || {
            Ok((
                assets::resolve(&tasks.control.snapshot(), &request, None)?,
                tasks.assets.reserve(Operation::Thumbnail)?,
            ))
        })
        .map_err(|_| AssetError::SessionUnavailable)??;
    let service = tasks.assets.clone();
    let subscriptions = tasks.subscriptions.clone();
    let control = tasks.control.clone();
    let started = std::time::Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let image = service.prepare(&permit, request, path.clone())?;
        subscriptions
            .with_ready(request.subscription_id, || {
                if assets::resolve(&control.snapshot(), &request, None)? != path {
                    return Err(AssetError::StaleTask);
                }
                service.finish(&permit, image)
            })
            .map_err(|_| AssetError::SessionUnavailable)?
    })
    .await
    .unwrap_or(Err(AssetError::ServiceFault));
    record(request, "thumbnail", started, &result);
    result
}

#[tauri::command]
pub(crate) async fn reveal_task_file(
    tasks: tauri::State<'_, DesktopTasks>,
    request: RevealRequest,
) -> Result<RevealResult, AssetError> {
    let (path, permit) = tasks
        .subscriptions
        .with_ready(request.job.subscription_id, || {
            Ok((
                assets::resolve(
                    &tasks.control.snapshot(),
                    &request.job,
                    Some(request.target),
                )?,
                tasks.assets.reserve(Operation::Reveal)?,
            ))
        })
        .map_err(|_| AssetError::SessionUnavailable)??;
    let subscriptions = tasks.subscriptions.clone();
    let control = tasks.control.clone();
    let started = std::time::Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        assets::validate_file(&path)?;
        // 当前opener的macOS实现会进行有损UTF-8转换；不能定位到另一个名称。
        #[cfg(target_os = "macos")]
        if path.to_str().is_none() {
            return Err(AssetError::UnsafePath);
        }
        subscriptions
            .with_ready(request.job.subscription_id, || {
                if assets::resolve(&control.snapshot(), &request.job, Some(request.target))? != path
                {
                    return Err(AssetError::StaleTask);
                }
                Ok(())
            })
            .map_err(|_| AssetError::SessionUnavailable)??;
        // 仅调用Rust定位API，不安装opener插件、不暴露其通用open_url/open_path命令。
        // 系统可能只打开目录；Requested不是“已选中”的验证结果。
        tauri_plugin_opener::reveal_item_in_dir(path).map_err(|_| AssetError::RevealFailed)?;
        Ok(RevealResult::Requested)
    })
    .await
    .unwrap_or(Err(AssetError::ServiceFault));
    record(request.job, "reveal", started, &result);
    result
}

fn record<T>(
    request: JobAssetRequest,
    operation: &'static str,
    started: std::time::Instant,
    result: &Result<T, AssetError>,
) {
    // 只记录数字身份与无路径枚举；不记录请求Debug、像素、文件名、OS错误或Blob。
    tracing::info!(target: "pixofold", event = "asset_request_finished", operation,
        selection_id = request.selection_id.0, job_id = request.job_id, attempt = request.attempt,
        elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        result = if result.is_ok() { "ok" } else { "error" }, error_code = ?result.as_ref().err());
}
