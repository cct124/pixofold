use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn root() -> PathBuf {
    std::env::temp_dir().join("native-selected.png")
}

#[test]
fn output_directory_is_reusable_but_session_bound_and_does_not_occupy_input_slot() {
    use pixofold_core::model::OutputDirectory;
    let imports = Arc::new(NativeImports::default());
    let session = DecimalU64(1);
    let dir = tempfile::tempdir().unwrap();
    let selected = imports
        .reserve(session)
        .unwrap()
        .complete_output(Some(OutputDirectory::open(dir.path()).unwrap()))
        .unwrap()
        .unwrap();
    let retained = imports.output(session, selected.directory_id).unwrap();
    assert!(imports.output(session, selected.directory_id).is_ok());
    assert!(
        imports
            .output(DecimalU64(2), selected.directory_id)
            .is_err()
    );
    assert!(imports.consume(session, selected.directory_id, Ok).is_err());
    assert!(
        imports
            .reserve(session)
            .unwrap()
            .complete_output(None)
            .unwrap()
            .is_none()
    );
    assert!(
        imports.output(session, selected.directory_id).is_ok(),
        "取消保留原草稿"
    );
    let input = imports
        .reserve(session)
        .unwrap()
        .complete(Some(vec![root()]))
        .unwrap()
        .unwrap();
    assert!(imports.output(session, input.grant_id).is_err());
    imports
        .consume(session, input.grant_id, |_| Ok(()))
        .unwrap();
    imports
        .release_output(DecimalU64(2), selected.directory_id)
        .unwrap();
    assert!(imports.output(session, selected.directory_id).is_ok());
    let replacement = imports
        .reserve(session)
        .unwrap()
        .complete_output(Some(OutputDirectory::open(dir.path()).unwrap()))
        .unwrap()
        .unwrap();
    imports
        .release_output(session, selected.directory_id)
        .unwrap();
    assert!(imports.output(session, replacement.directory_id).is_ok());
    assert!(imports.output(session, selected.directory_id).is_err());
    imports.revoke();
    assert!(imports.output(session, replacement.directory_id).is_err());
    assert_eq!(
        retained.path(),
        dir.path().canonicalize().unwrap(),
        "任务克隆不随草稿撤销失效"
    );
}

#[test]
fn output_selection_revocation_close_and_late_disconnect_preserve_dialog_exclusivity() {
    use pixofold_core::model::OutputDirectory;
    let imports = Arc::new(NativeImports::default());
    let dir = tempfile::tempdir().unwrap();
    let first = DecimalU64(1);
    let next = DecimalU64(2);
    let dialog = imports.reserve(first).unwrap();
    imports.revoke();
    assert!(imports.reserve(next).is_err());
    assert!(imports.begin_drag(next).is_err());
    assert!(
        dialog
            .complete_output(Some(OutputDirectory::open(dir.path()).unwrap()))
            .is_err()
    );
    let selected = imports
        .reserve(next)
        .unwrap()
        .complete_output(Some(OutputDirectory::open(dir.path()).unwrap()))
        .unwrap()
        .unwrap();
    imports.revoke_output_session(first);
    assert!(imports.output(next, selected.directory_id).is_ok());
    let pending = imports.reserve(next).unwrap();
    imports.close();
    assert!(
        pending
            .complete_output(Some(OutputDirectory::open(dir.path()).unwrap()))
            .is_err()
    );
    assert!(imports.output(next, selected.directory_id).is_err());
}

#[test]
fn native_drag_requires_one_matching_enter_and_is_bounded_until_consumed_or_released() {
    let imports = Arc::new(NativeImports::default());
    let session = DecimalU64(7);
    assert!(imports.finish_drag(session, &[root()]).is_err());
    imports.begin_drag(session).unwrap();
    assert!(imports.begin_drag(session).is_err());
    assert!(imports.reserve(session).is_err());
    assert!(imports.finish_drag(DecimalU64(8), &[root()]).is_err());
    let offer = imports.finish_drag(session, &[root(), root()]).unwrap();
    assert_eq!(offer.grant.unwrap().root_count, 2); // 去重仍由真实扫描器负责。
    assert!(imports.finish_drag(session, &[root()]).is_err());
    assert!(imports.begin_drag(session).is_err());
    imports.leave_drag(); // 已投递Drop之后的Leave不偷走授权。
    assert_eq!(
        imports.consume(session, offer.offer_id, Ok).unwrap(),
        vec![root(), root()]
    );
    assert!(imports.consume(session, offer.offer_id, Ok).is_err());
    imports.begin_drag(session).unwrap();
    let next = imports.finish_drag(session, &[root()]).unwrap();
    imports.release_drop(session, offer.offer_id).unwrap();
    imports.release_drop(DecimalU64(8), next.offer_id).unwrap();
    assert!(imports.begin_drag(session).is_err());
    imports.release_drop(session, next.offer_id).unwrap();
    imports.release_drop(session, next.offer_id).unwrap();
    assert!(imports.begin_drag(session).is_ok());
}

#[test]
fn native_drag_rejects_invalid_roots_without_retaining_paths_or_authorizing_import() {
    let imports = NativeImports::default();
    for roots in [
        vec![],
        vec![PathBuf::from("relative.png")],
        vec![root(); MAX_NATIVE_IMPORT_ROOTS + 1],
        vec![std::env::temp_dir().join("x".repeat(MAX_PATH_UNITS + 1))],
    ] {
        imports.begin_drag(DecimalU64(1)).unwrap();
        let offer = imports.finish_drag(DecimalU64(1), &roots).unwrap();
        assert!(offer.grant.is_none());
        assert!(imports.consume(DecimalU64(1), offer.offer_id, Ok).is_err());
        assert!(imports.begin_drag(DecimalU64(1)).is_err());
        imports.release_drop(DecimalU64(1), offer.offer_id).unwrap();
    }
}

#[test]
fn native_drag_revoke_leave_session_replacement_and_dialog_ownership_remain_separate() {
    let imports = Arc::new(NativeImports::default());
    imports.begin_drag(DecimalU64(1)).unwrap();
    imports.leave_drag();
    assert!(imports.finish_drag(DecimalU64(1), &[root()]).is_err());
    imports.begin_drag(DecimalU64(1)).unwrap();
    imports.revoke();
    assert!(imports.finish_drag(DecimalU64(1), &[root()]).is_err());
    imports.begin_drag(DecimalU64(2)).unwrap();
    let old = imports.finish_drag(DecimalU64(2), &[root()]).unwrap();
    imports.begin_drag(DecimalU64(3)).unwrap();
    assert!(imports.consume(DecimalU64(2), old.offer_id, Ok).is_err());
    imports.leave_drag();
    let dialog = imports.reserve(DecimalU64(3)).unwrap();
    assert!(imports.begin_drag(DecimalU64(4)).is_err());
    imports.revoke();
    imports.leave_drag();
    assert!(imports.begin_drag(DecimalU64(4)).is_err());
    assert!(dialog.complete(Some(vec![root()])).is_err());
    imports.begin_drag(DecimalU64(4)).unwrap();
    imports.close();
    assert!(imports.finish_drag(DecimalU64(4), &[root()]).is_err());
    assert!(imports.begin_drag(DecimalU64(5)).is_err());
}

#[test]
fn page_revocation_drops_grants_but_keeps_physical_dialog_occupied() {
    let imports = Arc::new(NativeImports::default());
    let issued = grant(&imports, 1);
    imports.revoke();
    assert!(matches!(
        imports.consume(DecimalU64(1), issued.grant_id, |_| Ok(())),
        Err(MutationError::StaleGrant)
    ));
    let pending = imports.reserve(DecimalU64(2)).unwrap();
    imports.revoke();
    imports.revoke();
    assert!(matches!(
        imports.reserve(DecimalU64(3)),
        Err(MutationError::SelectionBusy)
    ));
    assert!(matches!(
        pending.complete(Some(vec![root()])),
        Err(MutationError::StaleGrant)
    ));
    // 晚到结果拒绝，permit的Drop仍负责释放真实占位；新页面才能重新选择。
    let new = grant(&imports, 3);
    assert_eq!(
        imports.consume(DecimalU64(3), new.grant_id, Ok).unwrap(),
        vec![root()]
    );
}
fn grant(imports: &Arc<NativeImports>, session: u64) -> NativeImportGrant {
    imports
        .reserve(DecimalU64(session))
        .unwrap()
        .complete(Some(vec![root()]))
        .unwrap()
        .unwrap()
}

#[test]
fn single_dialog_cancel_empty_and_unwind_release_the_slot() {
    let imports = Arc::new(NativeImports::default());
    let permit = imports.reserve(DecimalU64(1)).unwrap();
    assert!(matches!(
        imports.reserve(DecimalU64(2)),
        Err(MutationError::SelectionBusy)
    ));
    assert_eq!(permit.complete(None).unwrap(), None);
    assert_eq!(
        imports
            .reserve(DecimalU64(2))
            .unwrap()
            .complete(Some(vec![]))
            .unwrap(),
        None
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _permit = imports.reserve(DecimalU64(1)).unwrap();
            panic!("injected native failure");
        }))
        .is_err()
    );
    assert!(imports.reserve(DecimalU64(1)).is_ok());
}

#[test]
fn grants_are_single_use_session_bound_and_replaced_without_exposing_paths() {
    let imports = Arc::new(NativeImports::default());
    let first = grant(&imports, 1);
    let latest = grant(&imports, 2);
    assert!(matches!(
        imports.consume(DecimalU64(1), first.grant_id, |_| Ok(())),
        Err(MutationError::StaleGrant)
    ));
    assert!(matches!(
        imports.consume(DecimalU64(1), latest.grant_id, |_| Ok(())),
        Err(MutationError::StaleGrant)
    ));
    // 接纳拒绝不会偷走授权。
    assert!(matches!(
        imports.consume::<()>(DecimalU64(2), latest.grant_id, |_| Err(
            MutationError::SelectionBusy
        )),
        Err(MutationError::SelectionBusy)
    ));
    assert_eq!(
        imports.consume(DecimalU64(2), latest.grant_id, Ok).unwrap(),
        vec![root()]
    );
    assert!(matches!(
        imports.consume(DecimalU64(2), latest.grant_id, |_| Ok(())),
        Err(MutationError::StaleGrant)
    ));
    let wire = serde_json::to_value(latest).unwrap();
    assert_eq!(wire.as_object().unwrap().len(), 2);
    assert_eq!(wire["rootCount"], 1);
    assert!(!wire.to_string().contains("native-selected"));
}

#[test]
fn expiration_limits_absolute_paths_and_close_fail_without_side_effects() {
    let imports = Arc::new(NativeImports::default());
    let expired = grant(&imports, 1);
    assert!(matches!(
        imports.consume_at(
            DecimalU64(1),
            expired.grant_id,
            Instant::now() + GRANT_TTL,
            |_| -> Result<(), MutationError> { panic!("expired paths must not be used") }
        ),
        Err(MutationError::StaleGrant)
    ));
    for roots in [
        vec![PathBuf::from("relative.png")],
        vec![root(); 1001],
        vec![std::env::temp_dir().join("a".repeat(MAX_PATH_UNITS + 1))],
    ] {
        assert!(matches!(
            imports
                .reserve(DecimalU64(1))
                .unwrap()
                .complete(Some(roots)),
            Err(MutationError::InvalidSelection)
        ));
    }
    let pending = imports.reserve(DecimalU64(1)).unwrap();
    imports.close();
    assert!(matches!(
        pending.complete(Some(vec![root()])),
        Err(MutationError::Closed)
    ));
    assert!(matches!(
        imports.reserve(DecimalU64(2)),
        Err(MutationError::Closed)
    ));
    assert!(matches!(
        imports.consume(DecimalU64(1), expired.grant_id, |_| Ok(())),
        Err(MutationError::Closed)
    ));
}

#[test]
fn exhausted_ids_and_poison_do_not_reopen_or_reuse_authority() {
    let imports = Arc::new(NativeImports::default());
    let previous = grant(&imports, 1);
    imports.state.lock().unwrap().next_id = u64::MAX;
    assert!(matches!(
        imports.reserve(DecimalU64(1)),
        Err(MutationError::IdExhausted)
    ));
    imports
        .consume(DecimalU64(1), previous.grant_id, |_| Ok(()))
        .unwrap();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _guard = imports.state.lock().unwrap();
            panic!("injected lock poison");
        }))
        .is_err()
    );
    assert!(matches!(
        imports.reserve(DecimalU64(1)),
        Err(MutationError::ServiceFault)
    ));
    imports.close();
}
