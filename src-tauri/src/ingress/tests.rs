use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn root() -> PathBuf {
    std::env::temp_dir().join("native-selected.png")
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
