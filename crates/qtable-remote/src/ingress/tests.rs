use super::*;
use std::sync::{Arc, mpsc};
use std::thread;

fn midi(status: u8) -> Input {
    Input::Midi(vec![status, 60, 100].into_boxed_slice())
}

#[test]
fn continuous_flood_reserves_notes_and_counts_overflow_once() {
    let ingress = Ingress::default();
    for _ in 0..CONTINUOUS_CAPACITY + 31 {
        ingress.push(Endpoint::Keyboard, midi(0xb0));
    }
    for _ in 0..DISCRETE_CAPACITY + 7 {
        ingress.push(Endpoint::Keyboard, midi(0x90));
    }
    assert_eq!(
        ingress.overflow(),
        Overflow {
            continuous: 31,
            discrete: 7,
            oversized: 0
        }
    );
    let first = ingress.drain();
    assert_eq!(first.events.len(), DRAIN_LIMIT);
    assert_eq!(first.dropped.total(), 38);
    let mut events = first.events;
    while events.len() < CONTINUOUS_CAPACITY + DISCRETE_CAPACITY {
        let next = ingress.drain();
        assert_eq!(next.dropped.total(), 0);
        events.extend(next.events);
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| e.input.queue_class() == QueueClass::Discrete)
            .count(),
        DISCRETE_CAPACITY
    );
    assert!(
        events
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
}

#[test]
fn concurrent_ports_publish_one_sequence_and_monotonic_receive_times() {
    let ingress = Arc::new(Ingress::default());
    let threads: Vec<_> = (0..4)
        .map(|port| {
            let ingress = ingress.clone();
            thread::spawn(move || {
                for _ in 0..100 {
                    ingress.push_midi(
                        &Endpoint::Midi {
                            id: port.to_string(),
                            name: "Controller".into(),
                        },
                        &[0xb0, 1, 1],
                    );
                }
            })
        })
        .collect();
    for worker in threads {
        worker.join().unwrap();
    }
    let events = ingress.drain().events;
    assert_eq!(events.len(), 400);
    assert!(
        events
            .windows(2)
            .all(|p| p[0].sequence + 1 == p[1].sequence && p[0].received_at <= p[1].received_at)
    );
}

#[test]
fn keys_and_note_off_are_discrete_and_cc_bend_pressure_are_continuous() {
    for status in [0xa0, 0xbf, 0xd0, 0xef] {
        assert_eq!(midi(status).queue_class(), QueueClass::Continuous);
    }
    for status in [0x80, 0x90, 0xc0, 0xf8] {
        assert_eq!(midi(status).queue_class(), QueueClass::Discrete);
    }
    assert_eq!(
        Input::Key {
            key: "a".into(),
            pressed: false,
            repeat: false
        }
        .queue_class(),
        QueueClass::Discrete
    );
}

#[test]
fn oversized_sysex_is_counted_without_filling_reserved_queue() {
    let ingress = Ingress::default();
    ingress.push_midi(&Endpoint::Keyboard, &vec![0xf0; MIDI_MESSAGE_LIMIT + 1]);
    ingress.push(Endpoint::Keyboard, midi(0x80));
    let drain = ingress.drain();
    assert_eq!(drain.events.len(), 1);
    assert_eq!(drain.activity, 2);
    assert_eq!(drain.dropped.oversized, 1);
    assert_eq!(drain.events[0].sequence, 2);
}

#[test]
fn discrete_interrupts_timer_and_one_wake_waits_for_drain() {
    let ingress = Arc::new(Ingress::default());
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let worker = {
        let ingress = ingress.clone();
        let stop = stop.clone();
        thread::spawn(move || {
            while ingress.wait_for_wake(&stop) {
                tx.send(()).unwrap();
            }
        })
    };
    assert!(rx.recv_timeout(Duration::from_millis(25)).is_err());
    ingress.push(Endpoint::Keyboard, midi(0xb0));
    // Inspect the contract's deadline rather than asserting loaded wall-clock timing.
    assert!(ingress.state.lock().unwrap().continuous_due.is_some());
    ingress.push(Endpoint::Keyboard, midi(0x90));
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(25)).is_err());
    assert_eq!(ingress.drain().events.len(), 2);
    assert!(rx.recv_timeout(Duration::from_millis(25)).is_err());
    stop.store(true, Ordering::Release);
    ingress.notify_host();
    worker.join().unwrap();
}

#[test]
fn resetting_a_lost_wake_allows_a_new_subscription_to_drain_pending_input() {
    let ingress = Ingress::default();
    let stop = AtomicBool::new(false);
    ingress.push(Endpoint::Keyboard, midi(0x90));
    assert!(ingress.wait_for_wake(&stop));
    assert!(ingress.state.lock().unwrap().in_flight);
    ingress.reset_wake();
    assert!(!ingress.state.lock().unwrap().in_flight);
    assert!(ingress.wait_for_wake(&stop));
    assert_eq!(ingress.drain().events.len(), 1);
}

#[test]
fn continuous_overflow_interrupts_the_continuous_deadline() {
    let ingress = Ingress::default();
    for _ in 0..CONTINUOUS_CAPACITY {
        ingress.push(Endpoint::Keyboard, midi(0xb0));
    }
    {
        let mut state = ingress.state.lock().unwrap();
        assert!(!state.urgent);
        // Keep the timer in the future independently of loaded test runtime.
        state.continuous_due = Some(Instant::now() + Duration::from_secs(60));
    }
    ingress.push(Endpoint::Keyboard, midi(0xb0));
    assert!(ingress.state.lock().unwrap().urgent);
    assert!(ingress.wait_for_wake(&AtomicBool::new(false)));
    assert_eq!(ingress.drain().dropped.continuous, 1);
}

#[test]
fn project_reset_discards_more_than_one_drain_without_reusing_sequences() {
    let ingress = Ingress::default();
    for _ in 0..CONTINUOUS_CAPACITY {
        ingress.push_midi(&Endpoint::Keyboard, &[0xb0, 1, 127]);
    }
    ingress.push(
        Endpoint::Keyboard,
        Input::Key {
            key: "a".into(),
            pressed: true,
            repeat: false,
        },
    );
    ingress.discard_pending();
    assert!(ingress.drain().events.is_empty());
    ingress.push_midi(&Endpoint::Keyboard, &[0x90, 60, 127]);
    let drain = ingress.drain();
    assert_eq!(drain.events.len(), 1);
    assert_eq!(drain.events[0].sequence, CONTINUOUS_CAPACITY as u64 + 2);
}
