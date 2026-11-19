//! Nested playback uses structural ancestry even with explicit destination sets.
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_engine::scheduler::SchedulerEvent;
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn nested_group_udp_paths_routes_and_delays_include_main_once() {
    let main = TestReceiver::bind().unwrap();
    let output = TestReceiver::bind().unwrap();
    let disabled = TestReceiver::bind().unwrap();
    let mut p = fixture::project(output.addr().port(), &[1.0]);
    p.main.output = p.outputs[0].output.clone();
    p.main.destination_mut().unwrap().host = "127.0.0.1".into();
    p.main.destination_mut().unwrap().port = main.addr().port();
    p.main.path = Path::new("/main").unwrap();
    p.main.delay_ms = 7.0;
    p.outputs[0].path = Path::new("/out").unwrap();
    p.outputs[0].delay_ms = 5.0;
    let mut off = Track::new_output(
        TrackId(202),
        "disabled",
        "127.0.0.1",
        disabled.addr().port(),
    );
    off.mixer.active = false;
    p.outputs.push(off);
    let mut outer = Track::new_group(TrackId(101), "outer", 2).with_path("/outer");
    outer.fold_state = true;
    outer.delay_ms = 3.0;
    outer.route = Route::Outputs(vec![p.main.id, TrackId(201), TrackId(202)]);
    let mut inner = Track::new_group(TrackId(102), "inner", 2).with_path("/inner");
    inner.group = Some(outer.id);
    inner.route = Route::Group;
    inner.fold_state = true;
    inner.delay_ms = 2.0;
    p[TrackId(1)].group = Some(inner.id);
    p[TrackId(1)].route = Route::Group;
    p[TrackId(1)].path = Path::new("/leaf").unwrap();
    // Both entries are at beat zero so exact scheduler deadlines reveal delay
    // composition without relying on wall-clock UDP jitter.
    let events = [
        OscAddress::Relative("/hit".into()),
        OscAddress::Absolute("/absolute".into()),
    ]
    .into_iter()
    .map(|address| p.hit(TrackId(1), BeatTime::ZERO, address, vec![]).unwrap())
    .collect();
    let clip = p[TrackId(1)].slots[0].clip.as_mut().unwrap();
    clip.events = events;
    clip.looping = false;
    p.tracks.insert(0, inner);
    p.tracks.insert(0, outer);
    p.validate().unwrap();
    let mut engine = Engine::with_diagnostics(p, true).unwrap();
    engine
        .execute(Command::LaunchGroupSlot {
            group: TrackId(101),
            slot: 0,
        })
        .unwrap();
    for (receiver, prefix) in [(&main, "/main"), (&output, "/out")] {
        let messages = receiver
            .wait_for_messages(2, Duration::from_secs(2))
            .unwrap();
        let addresses: Vec<_> = messages.iter().map(|(_, msg)| msg.addr.as_str()).collect();
        assert_eq!(
            addresses,
            [format!("{prefix}/outer/inner/leaf/hit"), "/absolute".into()]
        );
    }
    assert!(
        disabled
            .wait_for_messages(1, Duration::from_millis(50))
            .is_err()
    );
    let limit = Instant::now() + Duration::from_secs(1);
    let mut anchor = None;
    let mut due = Vec::new();
    while anchor.is_none() || due.len() < 4 {
        match engine
            .scheduler_events()
            .recv_timeout(limit.saturating_duration_since(Instant::now()))
            .unwrap()
        {
            SchedulerEvent::Playing(at) => anchor = Some(at),
            SchedulerEvent::Sent { due: at, .. } => due.push(at),
            SchedulerEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    }
    due.sort();
    let anchor = anchor.unwrap();
    assert_eq!(
        due,
        [
            anchor + Duration::from_millis(13),
            anchor + Duration::from_millis(13),
            anchor + Duration::from_millis(18),
            anchor + Duration::from_millis(18)
        ]
    );
}
