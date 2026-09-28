use super::*;

fn store(path: PathBuf) -> Store {
    Store::new(
        Arc::new(Directory::create(path).unwrap()),
        b"{\"schema\":\"pixofold.log.v1\",\"event\":\"session_started\"}\n".to_vec(),
    )
}
fn logs(path: &Path) -> Vec<PathBuf> {
    fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_str().is_some_and(owned_name))
        .collect()
}

#[test]
fn rotation_caps_actual_bytes_and_total_files_preserving_valid_json_lines() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let mut store = store(path.clone());
    store.max_bytes = 180;
    for _ in 0..80 {
        store
            .append(b"{\"event\":\"example\",\"count\":123456789}\n")
            .unwrap();
    }
    store.close().unwrap();
    let logs = logs(&path);
    assert_eq!(logs.len(), 10);
    for file in logs {
        let bytes = fs::read(file).unwrap();
        assert!(bytes.len() <= 180);
        let lines = String::from_utf8(bytes).unwrap();
        for line in lines.lines() {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
    }
}

#[test]
fn active_instances_are_never_deleted_and_all_busy_slots_refuse_an_eleventh_file() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let mut instances: Vec<_> = (0..10).map(|_| store(path.clone())).collect();
    for writer in &mut instances {
        writer.append(b"{\"event\":\"active\"}\n").unwrap();
    }
    let active = logs(&path);
    let mut eleventh = store(path.clone());
    assert_eq!(
        eleventh.append(b"{}\n").unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(logs(&path).len(), 10);
    assert!(active.iter().all(|p| p.exists()));
    instances[0].close().unwrap();
    eleventh.append(b"{}\n").unwrap();
    assert_eq!(logs(&path).len(), 10);
    for writer in instances.iter_mut().skip(1) {
        writer.append(b"{}\n").unwrap();
    }
}

#[test]
fn unknown_files_and_fake_headers_are_never_pruned_and_oversized_events_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let mut writer = store(path.clone());
    fs::write(path.join("notes.jsonl"), b"private data").unwrap();
    let fake = path.join("run-0000000000000-AAAAAAAAAAAA.jsonl");
    fs::write(&fake, b"not our file").unwrap();
    writer.max_files = 1;
    assert_eq!(
        writer.append(b"{}\n").unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(fs::read(fake).unwrap(), b"not our file");
    writer.max_bytes = 100;
    assert_eq!(
        writer.append(&[b'x'; 101]).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(path.join("notes.jsonl")).unwrap(), b"private data");
}

#[test]
fn existing_regular_file_is_not_replaced_with_a_directory() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    fs::write(&path, b"do not replace").unwrap();
    assert!(Directory::create(path.clone()).is_err());
    assert_eq!(fs::read(path).unwrap(), b"do not replace");
}

#[test]
fn active_jsonl_can_be_read_while_its_separate_lease_still_prevents_pruning() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let mut writer = store(path.clone());
    writer.append(b"{\"event\":\"readable\"}\n").unwrap();
    let file = logs(&path).pop().unwrap();
    assert!(fs::read_to_string(&file).unwrap().contains("readable"));
    let mut other = store(path);
    other.max_files = 1;
    assert_eq!(
        other.append(b"{}\n").unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    writer.append(b"{\"event\":\"still_writing\"}\n").unwrap();
    assert!(fs::read_to_string(&file).unwrap().contains("still_writing"));
}

#[cfg(unix)]
#[test]
fn shared_permissions_and_symlinks_are_not_silently_trusted() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(Directory::create(target.clone()).is_err());
    let link = temp.path().join("logs");
    symlink(target, &link).unwrap();
    assert!(Directory::create(link).is_err());
}

#[test]
fn child_lock_holder() {
    let Some(path) = std::env::var_os("PIXOFOLD_LOG_TEST_DIRECTORY") else {
        return;
    };
    let mut writer = store(PathBuf::from(path));
    writer.append(b"{}\n").unwrap();
    println!("PIXOFOLD_LOG_LOCKED");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    writer.close().unwrap();
}

#[test]
fn another_process_holds_its_active_file_until_exit() {
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("logs");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "desktop::diagnostics::storage::tests::child_lock_holder",
            "--nocapture",
        ])
        .env("PIXOFOLD_LOG_TEST_DIRECTORY", &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, ready) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("PIXOFOLD_LOG_LOCKED") {
                let _ = sender.send(());
            }
        }
    });
    let started = ready
        .recv_timeout(std::time::Duration::from_secs(15))
        .is_ok();
    if !started {
        // 仅终止本测试创建的子进程，绝不操作用户软件。
        let _ = child.kill();
        let _ = child.wait();
        let _ = reader.join();
        panic!("child did not acquire its log lock");
    }
    let mut writer = store(path.clone());
    writer.max_files = 1;
    let result = writer.append(b"{}\n");
    // 先释放自己启动的子进程，再断言，失败也不留下持锁进程。
    child.stdin.take().unwrap().write_all(b"exit\n").unwrap();
    assert!(child.wait().unwrap().success());
    reader.join().unwrap();
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
    writer.append(b"{}\n").unwrap();
    assert_eq!(logs(&path).len(), 1);
}
