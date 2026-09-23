//! 应用唯一原生选择槽。路径只由Rust原生入口写入，不对WebView序列化。
//! 最多一个物理对话框或一份授权，授权单次消费、绑定订阅、5分钟惰性失效。
//! 锁内仅做有界内存操作和TaskControl接纳；不得持锁打开对话框/扫描/等待线程。

use crate::ipc::{DecimalU64, MAX_NATIVE_IMPORT_ROOTS, MutationError, NativeImportGrant};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const GRANT_TTL: Duration = Duration::from_secs(5 * 60);
// OsStr长度按本机编码计量；同时限制根数，避免持有无界原生选择结果。
const MAX_PATH_UNITS: usize = 1024 * 1024;
enum Slot {
    Dialog {
        id: u64,
        session: u64,
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
    next_id: u64,
    closed: bool,
}
pub(crate) struct NativeImports {
    state: Mutex<State>,
}
impl Default for NativeImports {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                slot: None,
                next_id: 1,
                closed: false,
            }),
        }
    }
}
impl NativeImports {
    /// 只保留一个物理对话框；即便旧WebView已重载，也必须等旧选择真正返回。
    pub(crate) fn reserve(
        self: &Arc<Self>,
        session: DecimalU64,
    ) -> Result<SelectionPermit, MutationError> {
        let mut state = self.state.lock().map_err(|_| MutationError::ServiceFault)?;
        if state.closed {
            return Err(MutationError::Closed);
        }
        if matches!(state.slot, Some(Slot::Dialog { .. })) {
            return Err(MutationError::SelectionBusy);
        }
        let id = state.next_id;
        state.next_id = id.checked_add(1).ok_or(MutationError::IdExhausted)?;
        state.slot = Some(Slot::Dialog {
            id,
            session: session.0,
        });
        Ok(SelectionPermit {
            owner: self.clone(),
            id,
            session: session.0,
        })
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

    pub(crate) fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
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
        if !matches!(state.slot, Some(Slot::Dialog { id, session }) if id == self.id && session == self.session)
        {
            return Err(MutationError::StaleGrant);
        }
        // 先取走占位，任何无效原生结果都不会保留路径或卡住下一次选择。
        state.slot = None;
        let Some(roots) = roots.filter(|roots| !roots.is_empty()) else {
            return Ok(None);
        };
        if roots.len() > MAX_NATIVE_IMPORT_ROOTS
            || roots.iter().any(|path| !path.is_absolute())
            || roots
                .iter()
                .try_fold(0usize, |sum, path| sum.checked_add(path.as_os_str().len()))
                .is_none_or(|sum| sum > MAX_PATH_UNITS)
        {
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
