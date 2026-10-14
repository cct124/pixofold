use super::*;
use crate::model::{OutputPolicy, ProcessingError};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/apng")
            .join(name),
    )
    .unwrap()
}
fn setup() -> (tempfile::TempDir, ApngRequest) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("private.APNG");
    fs::write(&source, fixture("default-in.png")).unwrap();
    (dir, ApngRequest::new(source))
}
fn names(dir: &Path) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[test]
fn substituted_valid_candidates_are_independently_rejected_before_output() {
    for name in [
        "changed-pixel.apng",
        "changed-delay.apng",
        "changed-loop.apng",
        "changed-offset.apng",
        "changed-dispose.apng",
        "changed-blend.apng",
        "changed-metadata.apng",
    ] {
        let (dir, request) = setup();
        let original = fs::read(&request.source).unwrap();
        let result = execute(
            &request,
            &CancellationToken::default(),
            |_| {},
            |_, _, _| Ok(fixture(name)),
            Instant::now(),
        );
        assert!(
            matches!(result, Err(ApngError::ValidationFailed)),
            "{name}: {result:?}"
        );
        assert_eq!(fs::read(&request.source).unwrap(), original);
        assert_eq!(names(dir.path()), [request.source]);
    }
}
#[test]
fn exact_candidate_size_and_accumulated_decode_limits_apply_before_commit() {
    let (dir, mut request) = setup();
    let original = fs::read(&request.source).unwrap();
    request.limits.validation.max_candidate_bytes = 10;
    let result = execute(
        &request,
        &CancellationToken::default(),
        |_| {},
        |_, _, _| Ok(original.clone()),
        Instant::now(),
    );
    assert!(matches!(
        result,
        Err(ApngError::ResourceLimit("candidate_budget"))
    ));
    request.limits = ApngLimits::default();
    let measured = validate_apng(
        &original,
        &request.limits.validation,
        &CancellationToken::default(),
    )
    .unwrap();
    request.limits.validation.max_decoded_bytes = measured.decoded_bytes;
    let result = execute(
        &request,
        &CancellationToken::default(),
        |_| {},
        |_, _, _| Ok(original.clone()),
        Instant::now(),
    );
    assert!(matches!(
        result,
        Err(ApngError::ResourceLimit("decoded_budget"))
    ));
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(names(dir.path()), [request.source]);
}
#[test]
fn encoding_failure_and_cancel_during_encoding_keep_source_and_no_temporary() {
    let (dir, request) = setup();
    let original = fs::read(&request.source).unwrap();
    let result = execute(
        &request,
        &CancellationToken::default(),
        |_| {},
        |_, _, _| Err(ApngError::Encoding(oxipng::PngError::InvalidData)),
        Instant::now(),
    );
    assert!(matches!(result, Err(ApngError::Encoding(_))));
    let token = CancellationToken::default();
    let signal = token.clone();
    let copy = request.clone();
    let (entered, started) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let (finished, done) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = execute(
            &copy,
            &signal,
            |_| {},
            |bytes, _, _| {
                entered.send(()).unwrap();
                gate.recv().unwrap();
                Ok(bytes.to_vec())
            },
            Instant::now(),
        );
        finished.send(result).unwrap();
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    token.cancel();
    assert!(
        matches!(done.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "编码仍占资源，不能提前返回取消"
    );
    release.send(()).unwrap();
    assert!(matches!(
        done.recv_timeout(Duration::from_secs(5)).unwrap(),
        Err(ApngError::Cancelled)
    ));
    worker.join().unwrap();
    assert_eq!(fs::read(&request.source).unwrap(), original);
    assert_eq!(names(dir.path()), [request.source]);
}
#[test]
fn expired_preparation_never_reads_or_encodes() {
    let (_, request) = setup();
    let started = Instant::now() - Duration::from_secs(31);
    assert!(matches!(
        execute(
            &request,
            &CancellationToken::default(),
            |_| panic!("expired"),
            |_, _, _| panic!("expired"),
            started
        ),
        Err(ApngError::Timeout)
    ));
}
#[test]
fn preparation_deadline_is_checked_after_synchronous_encoding_returns() {
    let (dir, request) = setup();
    let started = Instant::now();
    let time = std::cell::Cell::new(started);
    let result = execute_timed(
        &request,
        &CancellationToken::default(),
        |_| {},
        |bytes, _, _| {
            time.set(started + request.limits.preparation_timeout);
            Ok(bytes.to_vec())
        },
        started,
        || time.get(),
    );
    assert!(matches!(result, Err(ApngError::Timeout)));
    assert_eq!(names(dir.path()), [request.source]);
}
#[test]
fn missing_source_preserves_io_error() {
    let (dir, mut request) = setup();
    request.output = OutputPolicy::Copy {
        destination: dir.path().join("copy.apng"),
    };
    // 来源路径失败仍保持格式无关I/O原因，不伪装成候选/解码成功。
    fs::remove_file(&request.source).unwrap();
    assert!(matches!(
        run(&request, &CancellationToken::default(), |_| {}),
        Err(ApngError::File(ProcessingError::Io { .. }))
    ));
}
