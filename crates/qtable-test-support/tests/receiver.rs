use std::io;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

use qtable_test_support::TestReceiver;
use rosc::{OscBundle, OscMessage, OscPacket, OscType};

fn message(index: i32) -> OscMessage {
    OscMessage {
        addr: format!("/test/{index}"),
        args: vec![
            OscType::Int(index),
            OscType::Float(0.5),
            OscType::String("hello".into()),
            OscType::Bool(true),
        ],
    }
}

fn bundle(content: Vec<OscPacket>) -> OscPacket {
    OscPacket::Bundle(OscBundle {
        timetag: (0, 1).into(),
        content,
    })
}

#[test]
fn captures_content_order_and_monotonic_arrival_times() {
    let receiver = TestReceiver::bind().unwrap();
    assert!(receiver.addr().ip().is_loopback());
    assert_ne!(receiver.addr().port(), 0);
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    let before = Instant::now();
    for index in 0..4 {
        let bytes = rosc::encoder::encode(&OscPacket::Message(message(index))).unwrap();
        sender.send_to(&bytes, receiver.addr()).unwrap();
    }
    let received = receiver
        .wait_for_messages(4, Duration::from_secs(2))
        .unwrap();
    let after = Instant::now();
    for (index, (arrival, actual)) in received.iter().enumerate() {
        assert_eq!(*actual, message(index as i32));
        assert!(*arrival >= before && *arrival <= after);
    }
    assert!(received.windows(2).all(|pair| pair[0].0 <= pair[1].0));
}

#[test]
fn flattens_nested_bundles_and_leaves_excess_messages_queued() {
    let receiver = TestReceiver::bind().unwrap();
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    let packet = bundle(vec![
        OscPacket::Message(message(0)),
        bundle(vec![OscPacket::Message(message(1))]),
        OscPacket::Message(message(2)),
    ]);
    sender
        .send_to(&rosc::encoder::encode(&packet).unwrap(), receiver.addr())
        .unwrap();
    let mut received = receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    received.extend(
        receiver
            .wait_for_messages(1, Duration::from_secs(2))
            .unwrap(),
    );
    assert_eq!(
        received
            .iter()
            .map(|item| item.1.clone())
            .collect::<Vec<_>>(),
        (0..3).map(message).collect::<Vec<_>>()
    );
    assert!(received.iter().all(|item| item.0 == received[0].0));
}

#[test]
fn timeout_and_empty_wait_are_explicit() {
    let receiver = TestReceiver::bind().unwrap();
    assert!(
        receiver
            .wait_for_messages(0, Duration::ZERO)
            .unwrap()
            .is_empty()
    );
    let error = receiver
        .wait_for_messages(1, Duration::from_millis(30))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("0 of 1"));
}

#[test]
fn malformed_datagram_is_reported_and_receiver_continues() {
    let receiver = TestReceiver::bind().unwrap();
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    sender.send_to(b"bad OSC", receiver.addr()).unwrap();
    assert_eq!(
        receiver
            .wait_for_messages(1, Duration::from_secs(2))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    sender
        .send_to(
            &rosc::encoder::encode(&OscPacket::Message(message(0))).unwrap(),
            receiver.addr(),
        )
        .unwrap();
    assert_eq!(
        receiver
            .wait_for_messages(1, Duration::from_secs(2))
            .unwrap()[0]
            .1,
        message(0)
    );
}

#[test]
fn wildcard_bind_supports_broadcast_test_setup_and_drop_releases_port() {
    let receiver = TestReceiver::bind_addr("0.0.0.0:0").unwrap();
    let addr = receiver.addr();
    assert!(addr.ip().is_unspecified());
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    sender
        .send_to(
            &rosc::encoder::encode(&OscPacket::Message(message(0))).unwrap(),
            ("127.0.0.1", addr.port()),
        )
        .unwrap();
    assert_eq!(
        receiver
            .wait_for_messages(1, Duration::from_secs(2))
            .unwrap()[0]
            .1,
        message(0)
    );
    drop(receiver);
    UdpSocket::bind(addr).unwrap();
}
