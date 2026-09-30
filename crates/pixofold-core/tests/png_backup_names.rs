//! 原名备份的真实输出与扫描契约；不碰用户原图，随机冲突由tempfile独占创建处理。

#[path = "support/native_names.rs"]
mod native_names;

use pixofold_core::{
    batch::{BatchConfig, BatchParameters, BatchService, JobState},
    import::{ImportOutput, ScanOptions, scan},
    model::{CancellationToken, PngRequest, ProcessingOutcome},
    pipeline::optimize_png,
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/png")
            .join(name),
    )
    .unwrap()
}

fn assert_name(source: &Path, backup: &Path) {
    assert_eq!(backup.parent(), source.canonicalize().unwrap().parent());
    let mut prefix = source.file_stem().unwrap().to_os_string();
    prefix.push("-backup-");
    let token = backup
        .file_name()
        .unwrap()
        .as_encoded_bytes()
        .strip_prefix(prefix.as_encoded_bytes())
        .unwrap()
        .strip_suffix(b".png")
        .unwrap();
    assert_eq!(token.len(), 6);
    assert!(token.iter().all(u8::is_ascii_alphanumeric));
}

fn overwrite(source: &Path) -> PathBuf {
    let report = optimize_png(
        &PngRequest::new(source),
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let ProcessingOutcome::Optimized {
        backup: Some(backup),
        ..
    } = report.outcome
    else {
        panic!("测试语料必须有收益并保留备份");
    };
    assert_name(source, &backup);
    backup
}

fn assert_scan_excludes_backups(directory: &Path, excluded: usize) {
    let found = scan(
        &[directory.to_owned()],
        ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(found.progress().excluded, excluded);
    assert_eq!(found.files().len(), 1);
}

#[test]
fn backup_uses_exact_stem_and_keeps_original_bytes_for_supported_names() {
    for name in [
        "风景.png",
        "photo.v2.PNG",
        "photo with spaces (1).png",
        "山水🌄.png",
        "image",
        "image.data",
        "png-content.jpg",
        ".hidden.png",
        ".hidden",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join(name);
        let original = fixture("rgb8.png");
        fs::write(&source, &original).unwrap();
        let backup = overwrite(&source);
        assert_eq!(fs::read(&backup).unwrap(), original);
        assert!(fs::metadata(&source).unwrap().len() < original.len() as u64);
        assert_scan_excludes_backups(directory.path(), 1);
    }
}

#[test]
fn repeated_backups_preserve_all_earlier_versions_and_legacy_files() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("风景.v2.png");
    let old = directory.path().join(".pixofold-backup-Ab12xY.png");
    let existing = directory.path().join("风景.v2-backup-Ab12xY.png");
    fs::write(&old, b"legacy backup").unwrap();
    fs::write(&existing, b"existing backup").unwrap();
    let mut names = HashSet::new();
    let mut saved = Vec::new();
    for sample in ["rgb8.png", "rgba8.png", "gray8.png"] {
        let original = fixture(sample);
        fs::write(&source, &original).unwrap();
        let backup = overwrite(&source);
        assert!(names.insert(backup.clone()));
        saved.push((backup, original));
        for (path, bytes) in &saved {
            assert_eq!(fs::read(path).unwrap(), *bytes);
        }
        assert_eq!(fs::read(&old).unwrap(), b"legacy backup");
        assert_eq!(fs::read(&existing).unwrap(), b"existing backup");
    }
    assert_scan_excludes_backups(directory.path(), 5);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 6);
}

#[test]
fn parallel_sources_with_the_same_stem_keep_distinct_backups() {
    let directory = tempfile::tempdir().unwrap();
    let sources = [
        directory.path().join("同名.png"),
        directory.path().join("同名.data"),
    ];
    let originals = [fixture("rgb8.png"), fixture("rgba8.png")];
    for (source, bytes) in sources.iter().zip(&originals) {
        fs::write(source, bytes).unwrap();
    }
    let found = scan(
        &sources,
        ScanOptions::default(),
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let plan = found
        .plan(&ImportOutput::Overwrite, BatchParameters::default())
        .unwrap();
    let service = BatchService::new(BatchConfig {
        workers: 2,
        ..Default::default()
    })
    .unwrap();
    let id = service.start(plan).unwrap();
    let done = service.wait(id, Duration::from_secs(30)).unwrap();
    assert_eq!(done.summary.succeeded, 2);
    let mut backups = HashSet::new();
    for (index, job) in done.jobs.iter().enumerate() {
        let JobState::Succeeded(report) = &job.state else {
            panic!("应成功");
        };
        let ProcessingOutcome::Optimized {
            backup: Some(backup),
            ..
        } = &report.outcome
        else {
            panic!("应备份");
        };
        assert_name(&sources[index], backup);
        assert!(backups.insert(backup));
        assert_eq!(fs::read(backup).unwrap(), originals[index]);
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 4);
}

#[test]
fn native_stem_is_preserved_and_its_backup_is_excluded() {
    let directory = tempfile::tempdir().unwrap();
    let original = fixture("rgb8.png");
    let source = native_names::write_native_name(directory.path(), ".png", &original);
    let backup = overwrite(&source);
    assert_eq!(fs::read(backup).unwrap(), original);
    assert_scan_excludes_backups(directory.path(), 1);
}

#[cfg(windows)]
#[test]
fn windows_case_changed_backup_names_remain_excluded() {
    let directory = tempfile::tempdir().unwrap();
    for name in [
        "photo.png",
        "photo-BACKUP-Ab12xY.PNG",
        ".PIXOFOLD-BACKUP-Ab12xY.PNG",
    ] {
        fs::write(directory.path().join(name), fixture("rgb8.png")).unwrap();
    }
    assert_scan_excludes_backups(directory.path(), 2);
}

#[cfg(windows)]
#[test]
fn overlong_backup_name_fails_without_replacing_source_or_leaving_candidates() {
    let directory = tempfile::tempdir().unwrap();
    // 输入叶名在NTFS限制内，但增加backup/随机码后超限；不能静默放弃备份继续覆盖。
    let source = directory.path().join(format!("{}.png", "x".repeat(245)));
    let original = fixture("rgb8.png");
    fs::write(&source, &original).unwrap();
    let mut stages = Vec::new();
    let result = optimize_png(
        &PngRequest::new(&source),
        &CancellationToken::default(),
        |stage| stages.push(stage),
    );
    assert!(stages.contains(&pixofold_core::model::ProcessingStage::Validating));
    assert!(matches!(
        result,
        Err(pixofold_core::model::ProcessingError::Io {
            operation: "创建独占临时文件",
            ..
        })
    ));
    assert_eq!(fs::read(&source).unwrap(), original);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
