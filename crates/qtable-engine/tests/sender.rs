use std::time::Duration;

use qtable_engine::model::{BeatTime, DeviceId, OscAddress, OscArgument, Track, TrackId};
use qtable_engine::sender::{OscSender, event_packet, resolve_address};
use qtable_test_support::TestReceiver;
use rosc::OscType;

#[test]
fn sends_resolved_events_with_typed_arguments_and_updated_destination() {
    let first = TestReceiver::bind().unwrap();
    let second = TestReceiver::bind().unwrap();
    let mut destination =
        Track::new_output(TrackId(201), "generic", "127.0.0.1", first.addr().port());
    let play = OscAddress::Relative("/play".into());
    let mut track = Track::new(TrackId(1), "track", 0);
    let event = track.hit(
        BeatTime::ZERO,
        play.clone(),
        vec![
            OscArgument::Int(-7),
            OscArgument::Float(1.25),
            OscArgument::String("0 / arbitrary receiver 🦀".into()),
            OscArgument::Bool(true),
            OscArgument::Bool(false),
        ],
        || DeviceId(1),
    );
    let packet = event_packet(&event, &resolve_address("/custom", &play));
    let mut sender = OscSender::new().unwrap();
    assert_eq!(sender.prepare(&destination).unwrap(), first.addr());
    // Repeated sends exercise the cached endpoint; changing the port must reroute.
    for _ in 0..2 {
        assert!(sender.send(&destination, &packet).unwrap() > 0);
    }
    destination.destination_mut().unwrap().port = second.addr().port();
    // Without a cached endpoint, send fails instead of resolving.
    let error = sender.send(&destination, &packet).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AddrNotAvailable);
    sender.prepare(&destination).unwrap();
    sender.send(&destination, &packet).unwrap();
    for (receiver, count) in [(&first, 2), (&second, 1)] {
        for (_, message) in receiver
            .wait_for_messages(count, Duration::from_secs(2))
            .unwrap()
        {
            assert_eq!(message.addr, "/custom/play");
            assert_eq!(
                message.args,
                vec![
                    OscType::Int(-7),
                    OscType::Float(1.25),
                    OscType::String("0 / arbitrary receiver 🦀".into()),
                    OscType::Bool(true),
                    OscType::Bool(false),
                ]
            );
        }
    }
}
