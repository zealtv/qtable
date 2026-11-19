use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

fn available(id: &str, name: &str) -> Available<()> {
    Available {
        handle: (),
        id: id.into(),
        name: name.into(),
    }
}

#[test]
fn saved_remote_reconnects_new_endpoint_and_drops_absent_or_disabled_connections() {
    let remote = BTreeMap::from([("Keys".into(), true)]);
    let mut connections = HashMap::new();
    let first = reconcile(
        vec![available("old", "Keys"), available("other", "New device")],
        &remote,
        &mut connections,
        |port| Ok(port.id.clone()),
    );
    assert!(first.ports[0].connected);
    assert!(!first.ports[1].remote);
    assert!(!first.ports[1].connected);
    // Unchanged scans keep the live connection rather than reopening it.
    reconcile(
        vec![available("old", "Keys")],
        &remote,
        &mut connections,
        |_| panic!("already connected"),
    );
    let absent = reconcile(
        Vec::<Available<()>>::new(),
        &remote,
        &mut connections,
        |_| panic!("no available port"),
    );
    assert!(absent.ports.is_empty());
    assert!(connections.is_empty());
    let returned = reconcile(
        vec![available("new", "Keys")],
        &remote,
        &mut connections,
        |port| Ok(port.id.clone()),
    );
    assert_eq!(returned.ports[0].id, "new");
    assert!(returned.ports[0].remote && returned.ports[0].connected);
    assert_eq!(connections["new"], "new");
    let disabled = reconcile(
        vec![available("new", "Keys")],
        &BTreeMap::from([("Keys".into(), false)]),
        &mut connections,
        |_| panic!("Remote Off"),
    );
    assert!(!disabled.ports[0].connected);
    assert!(connections.is_empty());
}

#[test]
fn failed_connections_retry_and_same_name_endpoints_stay_distinct() {
    let remote = BTreeMap::from([("Keys".into(), true)]);
    let mut connections = HashMap::new();
    let failed = reconcile(
        vec![available("one", "Keys")],
        &remote,
        &mut connections,
        |_| Err::<(), _>("not ready".into()),
    );
    assert!(failed.ports[0].remote);
    assert!(!failed.ports[0].connected);
    assert_eq!(
        failed.ports[0].error.as_deref(),
        Some("MIDI input Keys: not ready")
    );
    let retry = reconcile(
        vec![available("two", "Keys"), available("one", "Keys")],
        &remote,
        &mut connections,
        |_| Ok(()),
    );
    assert!(retry.ports.iter().all(|p| p.connected && p.error.is_none()));
    assert_eq!(retry.ports[0].id, "one");
    assert_eq!(retry.ports[1].id, "two");
    assert_eq!(connections.len(), 2);
}

#[test]
fn only_changed_snapshots_wake_an_idle_host_without_midi_input() {
    let ingress = Arc::new(Ingress::default());
    let stop = Arc::new(AtomicBool::new(false));
    let snapshot = Mutex::new(PortSnapshot::default());
    let next = PortSnapshot {
        error: Some("test error".into()),
        ..Default::default()
    };
    publish(&snapshot, next.clone(), &ingress);
    assert!(ingress.wait_for_wake(&stop));
    assert!(ingress.drain().events.is_empty());

    let (tx, rx) = mpsc::channel();
    let worker = {
        let ingress = ingress.clone();
        let stop = stop.clone();
        std::thread::spawn(move || tx.send(ingress.wait_for_wake(&stop)).unwrap())
    };
    publish(&snapshot, next, &ingress);
    let unchanged = rx.recv_timeout(Duration::from_millis(50));
    // Recover the worker even if the negative assertion fails.
    publish(&snapshot, PortSnapshot::default(), &ingress);
    let changed = if unchanged.is_err() {
        rx.recv_timeout(Duration::from_secs(2))
    } else {
        unchanged
    };
    stop.store(true, Ordering::Release);
    ingress.notify_host();
    worker.join().unwrap();
    assert!(matches!(unchanged, Err(mpsc::RecvTimeoutError::Timeout)));
    assert!(changed.unwrap());
    assert!(ingress.drain().events.is_empty());
}

#[test]
#[ignore = "requires CoreMIDI/ALSA; measures discovery wall time without sleeping"]
fn discovery_cost() {
    let input = MidiInput::new("qtable discovery cost").unwrap();
    let remote = BTreeMap::new();
    let mut connections = HashMap::new();
    let ingress = Arc::new(Ingress::default());
    let snapshot = Mutex::new(PortSnapshot::default());
    let mut samples = Vec::new();
    let mut ports = 0;
    for _ in 0..1000 {
        let start = Instant::now();
        #[cfg(target_os = "macos")]
        notifications::pump();
        let next = refresh(&input, &remote, &mut connections, &ingress);
        ports = next.ports.len();
        publish(&snapshot, next, &ingress);
        samples.push(start.elapsed().as_secs_f64() * 1_000_000.0);
    }
    samples.sort_by(f64::total_cmp);
    eprintln!(
        "discovery: n={} ports={ports}, mean={:.1} us median={:.1} us p95={:.1} us max={:.1} us",
        samples.len(),
        samples.iter().sum::<f64>() / samples.len() as f64,
        samples[500],
        samples[950],
        samples[999]
    );
}
