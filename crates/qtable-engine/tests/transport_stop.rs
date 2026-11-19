//! Live 12 transport: the first Stop halts, a second Stop rewinds to 1.1.1
//! (manual, Arrangement "Transport and Playback", read 2026-10-11).
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

fn wait_for(engine: &Engine, what: &str, done: impl Fn(&qtable_engine::engine::Snapshot) -> bool) {
    let limit = Instant::now() + Duration::from_secs(2);
    while !done(&engine.snapshot()) {
        assert!(Instant::now() < limit, "{what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn second_stop_rewinds_and_play_starts_from_zero() {
    let mut engine = Engine::new(fixture::project(9, &[0.0])).unwrap();
    engine.execute(Command::Play).unwrap();
    wait_for(&engine, "transport did not advance", |s| {
        s.playing && s.current_beat > BeatTime(1000)
    });
    engine.execute(Command::Stop).unwrap();
    wait_for(&engine, "did not stop", |s| !s.playing);
    let halted = engine.snapshot().current_beat;
    assert!(halted > BeatTime(1000), "first stop keeps the position");
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(engine.snapshot().current_beat, halted);

    engine.execute(Command::Stop).unwrap();
    wait_for(&engine, "second stop did not rewind", |s| {
        s.current_beat == BeatTime::ZERO
    });
    assert!(!engine.snapshot().playing);

    engine.execute(Command::Play).unwrap();
    wait_for(&engine, "play did not start", |s| s.playing);
    assert!(engine.snapshot().current_beat < halted);
}

#[test]
fn global_stop_freezes_the_clip_marker_and_global_play_restarts_its_voice() {
    let mut engine = Engine::new(fixture::project(9, &[0.0])).unwrap();
    let track = engine.project().tracks[0].id;
    let clip = engine.project().tracks[0].slots[0]
        .clip
        .as_ref()
        .unwrap()
        .id;
    engine
        .execute(Command::LaunchClip { track, slot: 0 })
        .unwrap();
    wait_for(&engine, "clip did not advance", |s| {
        s.tracks[0].play_position_beats.is_some_and(|p| p > 0.2)
    });
    engine.execute(Command::Stop).unwrap();
    wait_for(&engine, "clip did not stop", |s| {
        !s.playing && s.tracks[0].playing_slot.is_none()
    });
    let stopped = engine
        .snapshot()
        .clip_positions
        .into_iter()
        .find(|p| p.clip == clip)
        .unwrap()
        .position;
    assert!(stopped > BeatTime::ZERO);
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(engine.snapshot().clip_positions[0].position, stopped);
    engine.execute(Command::Stop).unwrap();
    wait_for(&engine, "second stop did not rewind clip", |s| {
        s.clip_positions[0].position == BeatTime::ZERO
    });
    engine.execute(Command::Play).unwrap();
    wait_for(&engine, "global play did not relaunch clip", |s| {
        s.playing && s.tracks[0].playing_slot == Some(0)
    });
}
