//! 使用真实TaskControl和内存Channel验证握手/背压；同步屏障代替固定休眠。

use super::*;
use crate::{
    ipc::TaskSubscriptionRequest,
    tasks::{TaskConfig, TaskRuntime, TaskSnapshot},
};
use serde_json::{Value, json};
use std::{sync::mpsc, time::Instant};

const TIMEOUT: Duration = Duration::from_secs(10);

fn channel() -> (Channel<TaskChangeNotice>, mpsc::Receiver<Value>) {
    let (sender, receiver) = mpsc::channel();
    (
        Channel::new(move |body| {
            sender
                .send(body.deserialize::<Value>()?)
                .map_err(|error| tauri::Error::Io(io::Error::other(error.to_string())))
        }),
        receiver,
    )
}
fn ack(control: &SubscriptionControl, ticket: TaskChangeNotice) -> Result<(), SubscriptionError> {
    control.acknowledge(TaskChangeAck {
        subscription_id: ticket.subscription_id,
        revision: ticket.revision,
    })
}
fn received(receiver: &mpsc::Receiver<Value>) -> TaskChangeNotice {
    let value = receiver.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
    assert!(value.to_string().len() < 160);
    TaskChangeNotice {
        protocol_version: value["protocolVersion"].as_u64().unwrap() as u32,
        subscription_id: DecimalU64(value["subscriptionId"].as_str().unwrap().parse().unwrap()),
        revision: DecimalU64(value["revision"].as_str().unwrap().parse().unwrap()),
    }
}
fn phase(control: &TaskControl, expected: TaskPhase) -> TaskSnapshot {
    let deadline = Instant::now() + TIMEOUT;
    let mut snapshot = control.snapshot();
    while snapshot.phase != expected {
        snapshot = control
            .wait_for_change(
                snapshot.revision,
                deadline.saturating_duration_since(Instant::now()),
            )
            .unwrap();
    }
    snapshot
}
fn changes(control: &TaskControl) -> u64 {
    for _ in 0..5 {
        control.import(vec![], None).unwrap();
        phase(control, TaskPhase::Rejected);
    }
    control.snapshot().revision
}

#[test]
fn handshake_and_slow_consumer_coalesce_to_one_latest_notice() {
    let tasks = TaskRuntime::new(TaskConfig::default()).unwrap();
    let mut runtime = SubscriptionRuntime::new(tasks.control()).unwrap();
    let control = runtime.control();
    let (channel, messages) = channel();
    let initial = control.subscribe(channel).unwrap();
    assert_eq!(initial.protocol_version, TASK_PROTOCOL_VERSION);
    let latest = changes(&tasks.control());
    assert_eq!(messages.try_recv(), Err(mpsc::TryRecvError::Empty));
    assert_eq!(
        ack(&control, notice(initial.subscription_id.0, latest + 1)),
        Err(SubscriptionError::InvalidAcknowledgement)
    );
    ack(&control, initial).unwrap();
    let first = received(&messages);
    assert_eq!(first, notice(initial.subscription_id.0, latest));
    let next = changes(&tasks.control());
    // 重复确认上一票据不能解锁尚未确认的first，变化也不会排队。
    ack(&control, initial).unwrap();
    assert_eq!(messages.try_recv(), Err(mpsc::TryRecvError::Empty));
    ack(&control, first).unwrap();
    assert_eq!(received(&messages), notice(initial.subscription_id.0, next));
    assert_eq!(messages.try_recv(), Err(mpsc::TryRecvError::Empty));
    runtime.shutdown().unwrap();
    assert_eq!(tasks.control().snapshot().phase, TaskPhase::Rejected);
}

#[test]
fn replacement_old_controls_and_terminal_reconnect_do_not_restart_tasks() {
    let mut tasks = TaskRuntime::new(TaskConfig::default()).unwrap();
    let mut runtime = SubscriptionRuntime::new(tasks.control()).unwrap();
    let control = runtime.control();
    let (old_channel, old_messages) = channel();
    let old = control.subscribe(old_channel).unwrap();
    tasks.shutdown().unwrap();
    let terminal = tasks.control().snapshot().revision;
    ack(&control, old).unwrap();
    assert_eq!(received(&old_messages).revision, DecimalU64(terminal));
    let (new_channel, new_messages) = channel();
    let new = control.subscribe(new_channel).unwrap();
    assert!(new.subscription_id.0 > old.subscription_id.0);
    assert_eq!(new.revision, DecimalU64(terminal));
    assert!(!control.unsubscribe(old.subscription_id).unwrap());
    assert_eq!(
        ack(&control, old),
        Err(SubscriptionError::StaleSubscription)
    );
    ack(&control, new).unwrap();
    // 已Closed但没有新revision时没有重复通知，shutdown仍能唤醒休眠线程。
    runtime.shutdown().unwrap();
    assert!(new_messages.try_recv().is_err());
    assert_eq!(tasks.control().snapshot().phase, TaskPhase::Closed);
}

#[test]
fn unsubscribe_and_drop_are_idempotent_and_preserve_tasks() {
    let tasks = TaskRuntime::new(TaskConfig::default()).unwrap();
    let runtime = SubscriptionRuntime::new(tasks.control()).unwrap();
    let control = runtime.control();
    let (channel, _messages) = channel();
    let ticket = control.subscribe(channel).unwrap();
    ack(&control, ticket).unwrap();
    assert!(control.unsubscribe(ticket.subscription_id).unwrap());
    assert!(!control.unsubscribe(ticket.subscription_id).unwrap());
    drop(runtime);
    assert_eq!(tasks.control().snapshot().phase, TaskPhase::Idle);
    assert_eq!(
        control.subscribe(Channel::new(|_| Ok(()))),
        Err(SubscriptionError::Closed)
    );
    assert_eq!(ack(&control, ticket), Err(SubscriptionError::Closed));
}

#[test]
fn replacement_during_failed_send_is_not_removed_by_the_old_sender() {
    let mut tasks = TaskRuntime::new(TaskConfig::default()).unwrap();
    let mut runtime = SubscriptionRuntime::new(tasks.control()).unwrap();
    let control = runtime.control();
    let (entered, sending) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Mutex::new(gate);
    let old = control
        .subscribe(Channel::new(move |_| {
            entered.send(()).unwrap();
            gate.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
            Err(tauri::Error::Io(io::Error::other("transport closed")))
        }))
        .unwrap();
    changes(&tasks.control());
    ack(&control, old).unwrap();
    sending.recv_timeout(TIMEOUT).unwrap();
    // 发送卡住时仍能替换，证明状态锁不跨Channel::send。
    let (new_channel, new_messages) = channel();
    let new = control.subscribe(new_channel).unwrap();
    tasks.shutdown().unwrap();
    release.send(()).unwrap();
    ack(&control, new).unwrap();
    assert_eq!(
        received(&new_messages),
        notice(new.subscription_id.0, tasks.control().snapshot().revision)
    );
    runtime.shutdown().unwrap();
}

#[test]
fn failed_send_releases_the_session_but_not_task_ownership() {
    let tasks = TaskRuntime::new(TaskConfig::default()).unwrap();
    let mut runtime = SubscriptionRuntime::new(tasks.control()).unwrap();
    let control = runtime.control();
    // 最后一个闭包捕获值释放时通知测试，不用延迟猜测发送失败的收尾时机。
    struct OnDrop(mpsc::Sender<()>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let (dropped, done) = mpsc::channel();
    let guard = OnDrop(dropped);
    let initial = control
        .subscribe(Channel::new(move |_| {
            let _ = &guard;
            Err(tauri::Error::Io(io::Error::other("closed")))
        }))
        .unwrap();
    changes(&tasks.control());
    ack(&control, initial).unwrap();
    done.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(
        ack(&control, initial),
        Err(SubscriptionError::StaleSubscription)
    );
    assert_eq!(tasks.control().snapshot().phase, TaskPhase::Rejected);
    let (channel, _messages) = channel();
    assert!(control.subscribe(channel).is_ok());
    runtime.shutdown().unwrap();
}

#[test]
fn sender_panic_fails_closed_and_is_reported_by_join() {
    let tasks = TaskRuntime::new(TaskConfig::default()).unwrap();
    let mut runtime = SubscriptionRuntime::new(tasks.control()).unwrap();
    let control = runtime.control();
    let (started, observed) = mpsc::channel();
    let ticket = control
        .subscribe(Channel::new(move |_| {
            started.send(()).unwrap();
            panic!("injected sender panic")
        }))
        .unwrap();
    changes(&tasks.control());
    ack(&control, ticket).unwrap();
    observed.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(runtime.shutdown(), Err(SubscriptionError::ServiceFault));
    assert_eq!(
        control.subscribe(Channel::new(|_| Ok(()))),
        Err(SubscriptionError::ServiceFault)
    );
    assert_eq!(tasks.control().snapshot().phase, TaskPhase::Rejected);
}

#[test]
fn subscription_ids_never_wrap_and_wire_requests_remain_strict() {
    let tasks = TaskRuntime::new(TaskConfig::default()).unwrap();
    let runtime = SubscriptionRuntime::new(tasks.control()).unwrap();
    let control = runtime.control();
    control.shared.lock().unwrap().next_id = u64::MAX - 1;
    let (channel, _messages) = channel();
    let ticket = control.subscribe(channel).unwrap();
    assert_eq!(ticket.subscription_id, DecimalU64(u64::MAX - 1));
    assert_eq!(
        control.subscribe(Channel::new(|_| Ok(()))),
        Err(SubscriptionError::IdExhausted)
    );
    ack(&control, ticket).unwrap(); // 耗尽不破坏原会话。
    let wire = serde_json::to_value(ticket).unwrap();
    assert_eq!(wire["subscriptionId"], "18446744073709551614");
    assert!(
        serde_json::from_value::<TaskChangeAck>(
            json!({"subscriptionId":"1", "revision":"0", "path":"secret"})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<TaskChangeAck>(json!({"subscriptionId":1, "revision":"0"}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<TaskSubscriptionRequest>(json!({"subscriptionId":"01"})).is_err()
    );
    assert!(
        serde_json::from_value::<TaskSubscriptionRequest>(
            json!({"subscriptionId":"1", "revision":"0"})
        )
        .is_err()
    );
}

#[test]
fn poisoned_subscription_lock_is_not_silently_recovered_as_success() {
    let tasks = TaskRuntime::new(TaskConfig::default()).unwrap();
    let mut runtime = SubscriptionRuntime::new(tasks.control()).unwrap();
    let control = runtime.control();
    let shared = control.shared.clone();
    let _ = std::thread::spawn(move || {
        let _guard = shared.state.lock().unwrap();
        panic!("injected lock poison");
    })
    .join();
    assert_eq!(
        control.subscribe(Channel::new(|_| Ok(()))),
        Err(SubscriptionError::ServiceFault)
    );
    assert_eq!(runtime.shutdown(), Err(SubscriptionError::ServiceFault));
}

#[test]
fn real_png_terminal_snapshot_survives_subscription_replacement_without_rerunning() {
    use crate::{ipc, tasks::TaskSettings};
    use pixofold_core::import::ImportOutput;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("private-source.png");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/png/rgb8.png");
    std::fs::copy(fixture, &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let tasks = TaskRuntime::new(TaskConfig::default()).unwrap();
    let mut runtime = SubscriptionRuntime::new(tasks.control()).unwrap();
    let control = runtime.control();
    let (channel, messages) = channel();
    let first = control.subscribe(channel).unwrap();
    tasks
        .control()
        .import(
            vec![source.clone()],
            Some(TaskSettings {
                output: ImportOutput::CopyBeside,
                ..TaskSettings::default()
            }),
        )
        .unwrap();
    let finished = phase(&tasks.control(), TaskPhase::Finished);
    ack(&control, first).unwrap();
    let terminal = received(&messages);
    assert_eq!(terminal.revision, DecimalU64(finished.revision));
    let request = || ipc::TaskPageRequest {
        expected_revision: Some(terminal.revision),
        collection: serde_json::from_value(json!("jobs")).unwrap(),
        offset: 0,
        limit: 1,
    };
    let before = serde_json::to_value(ipc::query(&tasks.control(), request()).unwrap()).unwrap();
    assert_eq!(before["phase"], "finished");
    let second = control.subscribe(Channel::new(|_| Ok(()))).unwrap();
    assert_eq!(second.revision, terminal.revision);
    assert!(!control.unsubscribe(first.subscription_id).unwrap());
    ack(&control, second).unwrap();
    let after = serde_json::to_value(ipc::query(&tasks.control(), request()).unwrap()).unwrap();
    assert_eq!(after, before);
    assert_eq!(std::fs::read(source).unwrap(), original);
    runtime.shutdown().unwrap();
}
