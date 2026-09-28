//! 主WebviewWindow的原生拖放桥，经WindowEvent分发；只在Drop传递有界票据/终点坐标。
//! 路径留在原生输入槽；未来若新增子WebView，须独立评估事件来源和区域归属。
//! 不注册通用事件权限，不依赖DOM File.path，不在事件线程扫描或写图片。

use super::mutations::ensure_can_import;
use crate::{
    ipc::{
        MutationError, NativeDropNotice, NativeDropPosition, SubscriptionError,
        TASK_PROTOCOL_VERSION, TaskStreamMessage,
    },
    lifecycle::DesktopTasks,
};
use tauri::DragDropEvent;

pub(crate) fn handle_native_drop(tasks: &DesktopTasks, label: &str, event: &DragDropEvent) {
    if label != "main" {
        return;
    }
    if matches!(event, DragDropEvent::Leave) {
        tasks.imports.leave_drag();
        return;
    }
    let result = tasks.subscriptions.with_current_ready(|session| {
        if ensure_can_import(&tasks.control).is_err() {
            tasks.imports.leave_drag();
            return Ok(None);
        }
        match event {
            DragDropEvent::Enter { .. } => {
                // 忙/待决票据/物理对话框占槽时拒绝新手势，不排队自动处理。
                tasks.imports.begin_drag(session).map(|_| None)
            }
            DragDropEvent::Drop { paths, position } => {
                if !position.x.is_finite() || !position.y.is_finite() {
                    tasks.imports.leave_drag();
                    return Ok(None);
                }
                tasks.imports.finish_drag(session, paths).map(|offer| {
                    Some((
                        offer,
                        NativeDropPosition {
                            x: position.x,
                            y: position.y,
                        },
                    ))
                })
            }
            // 不发送高频移动事件，避免阻塞页面时积压；最终以Drop终点命中真实区域。
            _ => Ok(None),
        }
    });
    if matches!(
        result,
        Err(SubscriptionError::ServiceFault | SubscriptionError::IdExhausted)
    ) {
        eprintln!("PixoFold 原生拖放：订阅服务异常，未接纳输入");
    }
    if let Ok((_, _, Err(error))) = &result {
        report_input_fault(error);
    }
    if let Ok((session, channel, Ok(Some((offer, position))))) = result {
        let message = TaskStreamMessage::NativeDrop(NativeDropNotice::NativeDrop {
            protocol_version: TASK_PROTOCOL_VERSION,
            subscription_id: session,
            offer,
            position,
        });
        // 发送及Channel释放在订阅/输入锁外。旧票据失败不能撤销新页面或新票据。
        if channel.send(message).is_err() {
            eprintln!("PixoFold 原生拖放：通知发送失败，回收本次授权");
            if let Err(error) = tasks.imports.release_drop(session, offer.offer_id) {
                report_input_fault(&error);
            }
        }
    }
}

fn report_input_fault(error: &MutationError) {
    if matches!(
        error,
        MutationError::ServiceFault | MutationError::IdExhausted
    ) {
        // 不记录路径/图片内容；正常忙、退出及重复手势属于预期拒绝，不刷错误日志。
        eprintln!("PixoFold 原生拖放：输入授权服务异常，未接纳输入");
    }
}
