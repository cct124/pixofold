//! 应用唯一原生输入槽。路径只由Rust原生选择/拖放写入，不对WebView序列化。
//! 最多一个物理对话框、拖放手势或待决授权；授权单次消费、绑定订阅、5分钟惰性失效。
//! 锁内仅做有界内存操作和TaskControl接纳；不得持锁打开对话框/扫描/等待线程。

use crate::ipc::{
    DecimalU64, MAX_NATIVE_IMPORT_ROOTS, MutationError, NativeDropOffer, NativeImportGrant,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const GRANT_TTL: Duration = Duration::from_secs(5 * 60);
// OsStr长度按本机编码计量；同时限制根数，避免持有无界原生选择结果。
const MAX_PATH_UNITS: usize = 1024 * 1024;
enum Slot {
    Drag {
        id: u64,
        session: u64,
    },
    RejectedDrop {
        id: u64,
        session: u64,
    },
    Dialog {
        id: u64,
        session: u64,
        revoked: bool,
    },
    Grant {
        id: u64,
        session: u64,
        roots: Vec<PathBuf>,
        expires: Instant,
    },
}
struct State {
    slot: Option<Slot>,
    output: Option<OutputGrant>,
    next_id: u64,
    closed: bool,
}
struct OutputGrant {
    id: u64,
    session: u64,
    directory: pixofold_core::model::OutputDirectory,
}
pub(crate) struct NativeImports {
    state: Mutex<State>,
}
impl Default for NativeImports {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                slot: None,
                output: None,
                next_id: 1,
                closed: false,
            }),
        }
    }
}
impl NativeImports {
    /// 只撤销指定旧会话的目录；断开响应迟到时不能撤销新会话的选择。
    pub(crate) fn revoke_output_session(&self, session: DecimalU64) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state
            .output
            .as_ref()
            .is_some_and(|o| o.session == session.0)
        {
            state.output = None;
        }
    }
    /// 只克隆当前会话目录，不在授权/订阅锁内执行I/O；实际写入前核心复查身份。
    pub(crate) fn output(
        &self,
        session: DecimalU64,
        id: DecimalU64,
    ) -> Result<pixofold_core::model::OutputDirectory, MutationError> {
        let state = self.state.lock().map_err(|_| MutationError::ServiceFault)?;
        if state.closed {
            return Err(MutationError::Closed);
        }
        state
            .output
            .as_ref()
            .filter(|o| o.session == session.0 && o.id == id.0)
            .map(|o| o.directory.clone())
            .ok_or(MutationError::StaleOutputDirectory)
    }

    /// 幂等释放精确标识；迟到清除不影响后来选中的目录。已接纳任务持有自己的克隆。
    pub(crate) fn release_output(
        &self,
        session: DecimalU64,
        id: DecimalU64,
    ) -> Result<(), MutationError> {
        let mut state = self.state.lock().map_err(|_| MutationError::ServiceFault)?;
        if matches!(&state.output, Some(o) if o.session == session.0 && o.id == id.0) {
            state.output = None;
        }
        Ok(())
    }
    /// 只保留一个物理对话框；即便旧WebView已重载，也必须等旧选择真正返回。
    pub(crate) fn reserve(
        self: &Arc<Self>,
        session: DecimalU64,
    ) -> Result<SelectionPermit, MutationError> {
        let mut state = self.state.lock().map_err(|_| MutationError::ServiceFault)?;
        if state.closed {
            return Err(MutationError::Closed);
        }
        // 旧会话的手势不是仍打开的物理对话框；新页面选择可撤销它，晚到Drop无授权。
        if matches!(&state.slot, Some(Slot::Drag { session: owner, .. }) if *owner != session.0) {
            state.slot = None;
        }
        if matches!(state.slot, Some(Slot::Dialog { .. } | Slot::Drag { .. })) {
            return Err(MutationError::SelectionBusy);
        }
        let id = state.next_id;
        state.next_id = id.checked_add(1).ok_or(MutationError::IdExhausted)?;
        state.slot = Some(Slot::Dialog {
            id,
            session: session.0,
            revoked: false,
        });
        Ok(SelectionPermit {
            owner: self.clone(),
            id,
            session: session.0,
        })
    }

    /// Enter只占位，不保存路径；上一票据未处理时忽略新拖放，不积压Channel消息。
    pub(crate) fn begin_drag(&self, session: DecimalU64) -> Result<(), MutationError> {
        let mut state = self.state.lock().map_err(|_| MutationError::ServiceFault)?;
        if state.closed {
            return Err(MutationError::Closed);
        }
        // 旧页面票据不能阻塞新会话；物理对话框例外，仍须等其真正返回。
        if matches!(&state.slot, Some(Slot::Grant { session: owner, .. } | Slot::RejectedDrop { session: owner, .. } | Slot::Drag { session: owner, .. }) if *owner != session.0)
        {
            state.slot = None;
        }
        if state.slot.is_some() {
            return Err(MutationError::SelectionBusy);
        }
        let id = state.next_id;
        state.next_id = id.checked_add(1).ok_or(MutationError::IdExhausted)?;
        state.slot = Some(Slot::Drag {
            id,
            session: session.0,
        });
        Ok(())
    }

    /// Drop只能结束同会话Enter，重复Drop没有授权；无效输入也占待决槽直到页面释放。
    pub(crate) fn finish_drag(
        &self,
        session: DecimalU64,
        roots: &[PathBuf],
    ) -> Result<NativeDropOffer, MutationError> {
        let mut state = self.state.lock().map_err(|_| MutationError::ServiceFault)?;
        if state.closed {
            return Err(MutationError::Closed);
        }
        let Some(Slot::Drag { id, session: owner }) = state.slot else {
            return Err(MutationError::StaleGrant);
        };
        if owner != session.0 {
            return Err(MutationError::StaleGrant);
        }
        let grant = if valid_roots(roots) {
            state.slot = Some(Slot::Grant {
                id,
                session: owner,
                roots: roots.to_vec(),
                expires: Instant::now() + GRANT_TTL,
            });
            Some(NativeImportGrant {
                grant_id: DecimalU64(id),
                root_count: roots.len() as u32,
            })
        } else {
            state.slot = Some(Slot::RejectedDrop { id, session: owner });
            None
        };
        Ok(NativeDropOffer {
            offer_id: DecimalU64(id),
            grant,
        })
    }

    /// Leave只撤销未完成手势，不回收已投递票据或影响原生选择框。
    pub(crate) fn leave_drag(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(state.slot, Some(Slot::Drag { .. })) {
            state.slot = None;
        }
    }

    /// 拖放处理完成/区域外/弹窗阻挡时释放匹配票据；不影响后续票据或对话框。
    pub(crate) fn release_drop(
        &self,
        session: DecimalU64,
        id: DecimalU64,
    ) -> Result<(), MutationError> {
        let mut state = self.state.lock().map_err(|_| MutationError::ServiceFault)?;
        if matches!(&state.slot, Some(Slot::Grant { id: granted, session: owner, .. } | Slot::RejectedDrop { id: granted, session: owner }) if *granted == id.0 && *owner == session.0)
        {
            state.slot = None;
        }
        Ok(())
    }

    /// 接纳成功才消费；忙/参数拒绝保留同一授权供显式重试，不执行文件I/O。
    pub(crate) fn consume<T>(
        &self,
        session: DecimalU64,
        id: DecimalU64,
        accept: impl FnOnce(Vec<PathBuf>) -> Result<T, MutationError>,
    ) -> Result<T, MutationError> {
        self.consume_at(session, id, Instant::now(), accept)
    }
    fn consume_at<T>(
        &self,
        session: DecimalU64,
        id: DecimalU64,
        now: Instant,
        accept: impl FnOnce(Vec<PathBuf>) -> Result<T, MutationError>,
    ) -> Result<T, MutationError> {
        let mut state = self.state.lock().map_err(|_| MutationError::ServiceFault)?;
        if state.closed {
            return Err(MutationError::Closed);
        }
        if matches!(&state.slot, Some(Slot::Grant { expires, .. }) if now >= *expires) {
            state.slot = None;
        }
        let Some(Slot::Grant {
            id: granted,
            session: owner,
            roots,
            ..
        }) = &state.slot
        else {
            return Err(MutationError::StaleGrant);
        };
        if *granted != id.0 || *owner != session.0 {
            return Err(MutationError::StaleGrant);
        }
        let result = accept(roots.clone())?;
        state.slot = None;
        Ok(result)
    }

    /// 页面重载撤销授权；物理对话框仍占槽，直到permit完成或释放。
    pub(crate) fn revoke(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.output = None;
        if let Some(Slot::Dialog { revoked, .. }) = &mut state.slot {
            *revoked = true;
        } else {
            state.slot = None;
        }
    }

    pub(crate) fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.output = None;
        state.closed = true;
        state.slot = None;
    }
}

/// RAII释放取消、原生失败和panic留下的占位；已发放的授权不由permit的Drop回收。
pub(crate) struct SelectionPermit {
    owner: Arc<NativeImports>,
    id: u64,
    session: u64,
}
impl SelectionPermit {
    /// 目录已在锁外验证。与输入选择共享物理槽，但不占用单次导入授权。取消保留旧草稿。
    pub(crate) fn complete_output(
        self,
        directory: Option<pixofold_core::model::OutputDirectory>,
    ) -> Result<Option<crate::ipc::NativeOutputDirectory>, MutationError> {
        let mut state = self
            .owner
            .state
            .lock()
            .map_err(|_| MutationError::ServiceFault)?;
        if state.closed {
            return Err(MutationError::Closed);
        }
        if !matches!(state.slot, Some(Slot::Dialog { id, session, revoked: false }) if id == self.id && session == self.session)
        {
            return Err(MutationError::StaleGrant);
        }
        state.slot = None;
        let Some(directory) = directory else {
            return Ok(None);
        };
        let response = crate::ipc::NativeOutputDirectory {
            directory_id: DecimalU64(self.id),
            name: crate::ipc::display_name(directory.path()),
        };
        state.output = Some(OutputGrant {
            id: self.id,
            session: self.session,
            directory,
        });
        Ok(Some(response))
    }
    pub(crate) fn complete(
        self,
        roots: Option<Vec<PathBuf>>,
    ) -> Result<Option<NativeImportGrant>, MutationError> {
        let mut state = self
            .owner
            .state
            .lock()
            .map_err(|_| MutationError::ServiceFault)?;
        if state.closed {
            return Err(MutationError::Closed);
        }
        if !matches!(state.slot, Some(Slot::Dialog { id, session, revoked: false }) if id == self.id && session == self.session)
        {
            return Err(MutationError::StaleGrant);
        }
        // 先取走占位，任何无效原生结果都不会保留路径或卡住下一次选择。
        state.slot = None;
        let Some(roots) = roots.filter(|roots| !roots.is_empty()) else {
            return Ok(None);
        };
        if !valid_roots(&roots) {
            return Err(MutationError::InvalidSelection);
        }
        let grant = NativeImportGrant {
            grant_id: DecimalU64(self.id),
            root_count: roots.len() as u32,
        };
        state.slot = Some(Slot::Grant {
            id: self.id,
            session: self.session,
            roots,
            expires: Instant::now() + GRANT_TTL,
        });
        Ok(Some(grant))
    }
}
impl Drop for SelectionPermit {
    fn drop(&mut self) {
        let mut state = self.owner.state.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(state.slot, Some(Slot::Dialog { id, .. }) if id == self.id) {
            state.slot = None;
        }
    }
}

#[cfg(test)]
mod tests;

fn valid_roots(roots: &[PathBuf]) -> bool {
    !roots.is_empty()
        && roots.len() <= MAX_NATIVE_IMPORT_ROOTS
        && roots.iter().all(|path| path.is_absolute())
        && roots
            .iter()
            .try_fold(0usize, |sum, path| sum.checked_add(path.as_os_str().len()))
            .is_some_and(|sum| sum <= MAX_PATH_UNITS)
}
