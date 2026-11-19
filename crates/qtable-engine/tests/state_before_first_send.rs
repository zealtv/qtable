//! Regression: a client that sees the launch's first OSC packet must already
//! see the launch in the snapshot (race seen 2026-10-11 in the CLI status test).
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use std::net::UdpSocket;
use std::time::Duration;

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn snapshot_shows_launch_by_the_time_its_first_packet_arrives() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let mut packet = [0; 1024];
    for round in 0..300 {
        let mut project = fixture::project(port, &[0.0]);
        project.tempo = Tempo::new(120.0).unwrap();
        let mut engine = Engine::new(project).unwrap();
        engine
            .execute(Command::LaunchClip {
                track: TrackId(1),
                slot: 0,
            })
            .unwrap();
        socket.recv(&mut packet).expect("first packet");
        let snapshot = engine.snapshot();
        assert!(snapshot.playing, "round {round}: transport still stopped");
        assert_eq!(
            snapshot.tracks[0].playing_slot,
            Some(0),
            "round {round}: launched clip not playing"
        );
        // Drain the clip's remaining packets before the next round.
        drop(engine);
        socket.set_nonblocking(true).unwrap();
        while socket.recv(&mut packet).is_ok() {}
        socket.set_nonblocking(false).unwrap();
    }
}
