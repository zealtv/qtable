//! The snapshot publishes how far through its current pass each playing clip is, so a client
//! draws it without knowing where the clip's markers sit.
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use std::time::{Duration, Instant};

#[path = "support/scheduler_project.rs"]
mod fixture;

const BEAT: u64 = 960;

fn markers(clip: &mut Clip, start: u64, end: u64, loop_start: u64, loop_end: u64) {
    clip.events.clear();
    clip.lanes.clear();
    clip.start = BeatTime(start * BEAT);
    clip.end = BeatTime(end * BEAT);
    clip.loop_start = BeatTime(loop_start * BEAT);
    clip.loop_end = BeatTime(loop_end * BEAT);
    clip.looping = true;
}

#[test]
fn an_offset_loop_and_an_intro_publish_their_pass_progress() {
    let mut project = fixture::project(9, &[0.0, 0.0]);
    // Track 1: Start and the brace at 4..8. Track 2: an intro from 0, then the brace at 4..8.
    markers(
        project.tracks[0].slots[0].clip.as_mut().unwrap(),
        4,
        8,
        4,
        8,
    );
    markers(
        project.tracks[1].slots[0].clip.as_mut().unwrap(),
        0,
        8,
        4,
        8,
    );
    let tracks = [project.tracks[0].id, project.tracks[1].id];
    let mut engine = Engine::new(project).unwrap();
    for track in tracks {
        engine
            .execute(Command::LaunchClip { track, slot: 0 })
            .unwrap();
    }
    let (mut offset_samples, mut intro_samples, mut looped_samples) = (0, 0, 0);
    let limit = Instant::now() + Duration::from_secs(3);
    // The fixture plays at 600 BPM: one beat in 100 ms, so 1.5 s covers the intro and the loop.
    while Instant::now() < limit && (intro_samples < 5 || looped_samples < 5 || offset_samples < 20)
    {
        let snapshot = engine.snapshot();
        std::thread::sleep(Duration::from_millis(3));
        let [offset, intro] = &snapshot.tracks[..] else {
            panic!("two clip tracks")
        };
        if let (Some(position), Some(progress)) = (offset.play_position_beats, offset.pass_progress)
        {
            // Content positions run 4..8; each pass is that whole brace.
            assert!((4.0..=8.0).contains(&position), "position {position}");
            assert!(
                (progress - (position - 4.0) / 4.0).abs() < 1e-9,
                "offset loop: position {position}, progress {progress}"
            );
            offset_samples += 1;
        }
        if let (Some(position), Some(progress)) = (intro.play_position_beats, intro.pass_progress) {
            // The first pass spans 0..8; later passes span the brace 4..8 alone.
            let first = position / 8.0;
            let later = (position - 4.0) / 4.0;
            assert!(
                (progress - first).abs() < 1e-9
                    || (position >= 4.0 && (progress - later).abs() < 1e-9),
                "intro: position {position}, progress {progress}"
            );
            if position < 4.0 && (progress - first).abs() < 1e-9 {
                intro_samples += 1;
            } else if (progress - later).abs() < 1e-9 && (progress - first).abs() > 1e-6 {
                looped_samples += 1;
            }
        }
    }
    assert!(
        offset_samples >= 20,
        "offset loop sampled {offset_samples} times"
    );
    assert!(intro_samples >= 5, "intro sampled {intro_samples} times");
    assert!(
        looped_samples >= 5,
        "loop after the intro sampled {looped_samples} times"
    );
}
