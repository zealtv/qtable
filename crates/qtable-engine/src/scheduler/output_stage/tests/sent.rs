//! `Sent` reports: one per clip event, its lateness taken just before the send call of the
//! datagram that carries it (bundle-send-timing).
use super::*;
use crate::scheduler::SchedulerEvent;
use std::net::SocketAddr;

fn diagnosed(p: &Project) -> (Rig, EventSink, mpsc::Receiver<SchedulerEvent>) {
    let mut rig = Rig::new(p);
    rig.stages.diagnostics = true;
    let (tx, rx) = mpsc::sync_channel(1000);
    (rig, EventSink::new(tx), rx)
}

fn hit(
    rig: &mut Rig,
    tx: &mut EventSink,
    p: &Project,
    value: f32,
    due: Instant,
) -> io::Result<usize> {
    rig.stages.event(
        &mut rig.sender,
        tx,
        &p.outputs[0],
        &event_packet(value),
        due,
        Instant::now(),
    )
}

/// The `Sent` reports waiting, as (due, lateness), and whether any error was reported.
fn reports(rx: &mpsc::Receiver<SchedulerEvent>) -> (Vec<(Instant, Duration)>, bool) {
    let mut sent = Vec::new();
    let mut error = false;
    for event in rx.try_iter() {
        match event {
            SchedulerEvent::Sent { due, lateness } => sent.push((due, lateness)),
            SchedulerEvent::Error(_) => error = true,
            _ => {}
        }
    }
    (sent, error)
}

#[test]
fn bundled_hits_report_sent_once_each_when_their_datagram_goes() {
    let p = with_stage(many(1, SendMode::Continuous), |stage| stage.bundle = true);
    let (mut rig, mut tx, rx) = diagnosed(&p);
    let due = Instant::now();
    hit(&mut rig, &mut tx, &p, 1.0, due).unwrap();
    hit(&mut rig, &mut tx, &p, 2.0, due).unwrap();
    let buffered = Instant::now();
    assert_eq!(reports(&rx), (vec![], false), "buffering isn't sending");
    std::thread::sleep(Duration::from_millis(2));
    rig.stages.flush_events(&p, &mut rig.sender, &mut tx, None);
    let after = Instant::now();
    assert_eq!(rig.read().len(), 1, "one bundle");
    let (sent, error) = reports(&rx);
    assert!(!error);
    assert_eq!(sent.len(), 2);
    for (reported, lateness) in sent {
        assert_eq!(reported, due);
        assert!(
            lateness >= buffered + Duration::from_millis(2) - due,
            "{lateness:?}"
        );
        assert!(lateness <= after - due);
    }
}

#[test]
fn a_bundle_split_across_datagrams_reports_each_hit_once() {
    let p = with_stage(many(1, SendMode::Continuous), |stage| stage.bundle = true);
    let (mut rig, mut tx, rx) = diagnosed(&p);
    let due = Instant::now();
    for value in 0..200 {
        hit(&mut rig, &mut tx, &p, value as f32, due).unwrap();
    }
    rig.stages.flush_events(&p, &mut rig.sender, &mut tx, None);
    let datagrams = rig.read();
    assert!(datagrams.len() > 1, "200 events need more than one bundle");
    let (sent, error) = reports(&rx);
    assert!(!error);
    assert_eq!(sent.len(), 200);
    assert!(sent.iter().all(|(reported, _)| *reported == due));
    // Each datagram's events share its own sample, taken later for each later datagram.
    let mut samples: Vec<_> = sent.iter().map(|(_, lateness)| *lateness).collect();
    assert!(samples.is_sorted());
    samples.dedup();
    assert_eq!(samples.len(), datagrams.len());
}

#[test]
fn a_failed_flush_reports_its_error_and_never_a_sent() {
    let p = with_stage(many(1, SendMode::Continuous), |stage| stage.bundle = true);
    let (mut rig, mut tx, rx) = diagnosed(&p);
    let good = rig.receiver.local_addr().unwrap();
    let due = Instant::now();
    hit(&mut rig, &mut tx, &p, 1.0, due).unwrap();
    hit(&mut rig, &mut tx, &p, 2.0, due).unwrap();
    // An IPv6 endpoint on the IPv4 socket: the flush's send call fails.
    let bad: SocketAddr = "[::1]:9".parse().unwrap();
    rig.sender.insert_resolved(&p.outputs[0], bad);
    rig.stages.flush_events(&p, &mut rig.sender, &mut tx, None);
    assert_eq!(reports(&rx), (vec![], true));
    // A later bundle reports only its own hit, once.
    rig.sender.insert_resolved(&p.outputs[0], good);
    let later = Instant::now();
    hit(&mut rig, &mut tx, &p, 3.0, later).unwrap();
    rig.stages.flush_events(&p, &mut rig.sender, &mut tx, None);
    let (sent, error) = reports(&rx);
    assert!(!error);
    assert_eq!(
        sent.iter().map(|(due, _)| *due).collect::<Vec<_>>(),
        [later]
    );
}

#[test]
fn immediate_hits_report_sent_once_sampled_before_their_send() {
    for bundle in [false, true] {
        let p = with_stage(many(1, SendMode::Continuous), |stage| stage.bundle = bundle);
        let (mut rig, mut tx, rx) = diagnosed(&p);
        if bundle {
            // An unresolved output can't hold a bundle: its hit is sent (and fails) at once.
            let mut unresolved = OscSender::new().unwrap();
            std::mem::swap(&mut rig.sender, &mut unresolved);
            assert!(hit(&mut rig, &mut tx, &p, 1.0, Instant::now()).is_err());
            assert_eq!(
                reports(&rx),
                (vec![], false),
                "the caller reports the error"
            );
            continue;
        }
        let due = Instant::now() - Duration::from_millis(1);
        let before = Instant::now();
        hit(&mut rig, &mut tx, &p, 1.0, due).unwrap();
        let after = Instant::now();
        let (sent, error) = reports(&rx);
        assert!(!error);
        let [(reported, lateness)] = sent.as_slice() else {
            panic!("expected one Sent, got {sent:?}");
        };
        assert_eq!(*reported, due);
        assert!(*lateness >= before - due && *lateness <= after - due);
    }
}

#[test]
fn an_inactive_output_still_reports_its_hits_as_before() {
    // Unchanged behaviour: a zero-byte send to an inactive output counts as handled.
    for bundle in [false, true] {
        let mut p = with_stage(many(1, SendMode::Continuous), |stage| stage.bundle = bundle);
        p.outputs[0].mixer.active = false;
        let (mut rig, mut tx, rx) = diagnosed(&p);
        let due = Instant::now();
        hit(&mut rig, &mut tx, &p, 1.0, due).unwrap();
        rig.stages.flush_events(&p, &mut rig.sender, &mut tx, None);
        assert!(rig.read().is_empty());
        let (sent, error) = reports(&rx);
        assert!(!error);
        assert_eq!(sent.len(), 1, "bundle {bundle}");
    }
}

#[test]
fn without_diagnostics_nothing_is_reported() {
    let p = with_stage(many(1, SendMode::Continuous), |stage| stage.bundle = true);
    let (mut rig, mut tx, rx) = diagnosed(&p);
    rig.stages.diagnostics = false;
    hit(&mut rig, &mut tx, &p, 1.0, Instant::now()).unwrap();
    rig.stages.flush_events(&p, &mut rig.sender, &mut tx, None);
    assert_eq!(rig.read().len(), 1);
    assert_eq!(reports(&rx), (vec![], false));
}
