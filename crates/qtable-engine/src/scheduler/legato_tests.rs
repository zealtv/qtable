//! Legato take-over of the playing clip's position (Live 12.1 via AbletonOSC, 2026-10-12).
use super::*;
use crate::model::*;

const BEAT: u64 = 960;

/// Slot 0: a 4-beat looping clip. Slot 1: a legato clip with events at beats 0, 1 and 2.
fn legato_project(second_length: u64, looping: bool) -> Project {
    let mut project = super::tests::project(0.0, true);
    let track = &mut project.tracks[0];
    let mut events = |prefix: &str, beats: &[u64]| -> Vec<Event> {
        beats
            .iter()
            .map(|&beat| {
                let path = OscAddress::Absolute(format!("/{prefix}{beat}"));
                track.hit(BeatTime(beat * BEAT), path, vec![], || DeviceId(1))
            })
            .collect()
    };
    let (a, b) = (events("a", &[0, 1, 2, 3]), events("b", &[0, 1, 2]));
    let first = track.slots[0].clip.as_mut().unwrap();
    first.end = BeatTime(4 * BEAT);
    first.loop_end = BeatTime(4 * BEAT);
    first.events = a;
    first.quantisation = LaunchQuantisation::None;
    let second = track.slots[1].clip.as_mut().unwrap();
    second.end = BeatTime(second_length * BEAT);
    second.loop_end = BeatTime(second_length * BEAT);
    second.looping = looping;
    second.legato = true;
    second.quantisation = LaunchQuantisation::None;
    second.events = b;
    let length = second.length();
    second.events.retain(|e| e.position < length);
    project
}

fn planner(second_length: u64, looping: bool) -> Planner {
    Planner::new(
        legato_project(second_length, looping),
        Duration::from_secs(10),
    )
    .unwrap()
}

fn clock(start: Instant) -> InternalClock {
    let mut clock = InternalClock::default();
    clock.play(start);
    clock
}

/// Addresses the voice would send, in order, as planned from `now`.
fn sends(p: &Planner, clock: &InternalClock, now: Instant, voice: u64) -> Vec<(Instant, String)> {
    p.plan(clock, now)
        .iter()
        .filter(|e| e.voice == voice)
        .filter_map(|e| match &e.action {
            PlannedAction::Send {
                packet: Some(OscPacket::Message(m)),
                ..
            } => Some((e.at, m.addr.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn takes_over_the_unwrapped_time_since_the_playing_clips_origin() {
    let start = Instant::now();
    let clock = clock(start);
    let mut p = planner(3, true);
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    // 7 beats after A's start: B (3 beats) is at 7 mod 3 = 1, not A's in-loop 3 mod 3 = 0.
    p.launch_at(TrackId(1), 1, BeatTime(7 * BEAT)).unwrap();
    let b = &p.voices[1];
    assert_eq!(b.launch, BeatTime(7 * BEAT));
    assert_eq!(b.origin, BeatTime(6 * BEAT));
    let planned = sends(&p, &clock, start, 2);
    // The event exactly at the take-over position fires at the launch; earlier ones are skipped.
    assert_eq!(
        planned[0],
        (start + Duration::from_millis(3500), "/b1".into())
    );
    assert_eq!(planned[1].1, "/b2");
    assert_eq!(planned[2].1, "/b0");
    // The old clip stops at the launch.
    assert!(
        sends(&p, &clock, start, 1)
            .iter()
            .all(|(at, _)| *at < start + Duration::from_millis(3500))
    );
}

#[test]
fn without_a_playing_clip_it_starts_from_the_beginning() {
    let start = Instant::now();
    let clock = clock(start);
    let mut p = planner(3, true);
    p.launch_at(TrackId(1), 1, BeatTime(7 * BEAT)).unwrap();
    assert_eq!(p.voices[0].origin, BeatTime(7 * BEAT));
    assert_eq!(sends(&p, &clock, start, 1)[0].1, "/b0");
}

#[test]
fn a_plain_clip_ignores_the_playing_position() {
    let start = Instant::now();
    let clock = clock(start);
    let mut p = planner(3, true);
    Arc::make_mut(&mut p.project).tracks[0].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .legato = false;
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    p.launch_at(TrackId(1), 1, BeatTime(7 * BEAT)).unwrap();
    assert_eq!(p.voices[1].origin, BeatTime(7 * BEAT));
    assert_eq!(sends(&p, &clock, start, 2)[0].1, "/b0");
}

#[test]
fn a_nonlooping_clip_past_its_end_starts_finished_and_the_track_goes_silent() {
    let start = Instant::now();
    let clock = clock(start);
    let mut p = planner(2, false);
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    p.launch_at(TrackId(1), 1, BeatTime(7 * BEAT)).unwrap();
    assert!(sends(&p, &clock, start, 2).is_empty());
    assert!(
        p.plan(&clock, start)
            .iter()
            .any(|e| e.voice == 2 && matches!(e.action, PlannedAction::TrackSilent(_)))
    );
}

#[test]
fn a_nonlooping_clip_before_its_end_plays_its_remainder() {
    let start = Instant::now();
    let clock = clock(start);
    let mut p = planner(3, false);
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    p.launch_at(TrackId(1), 1, BeatTime(BEAT + BEAT / 2))
        .unwrap();
    // 1.5 beats in: /b2 at beat 2 is the only event left.
    let planned = sends(&p, &clock, start, 2);
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0], (start + Duration::from_secs(1), "/b2".into()));
}

#[test]
fn manual_launch_takes_over_at_the_clip_boundary() {
    let start = Instant::now();
    let mut clock = clock(start);
    let mut p = planner(3, true);
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let now = start + Duration::from_millis(3500);
    let launch = p.launch(TrackId(1), 1, &mut clock, now).unwrap();
    assert_eq!(launch, BeatTime(7 * BEAT));
    assert_eq!(p.voices.last().unwrap().origin, BeatTime(6 * BEAT));
}

#[test]
fn a_legato_launch_cancels_a_pending_stop_and_the_old_clip_plays_until_the_boundary() {
    let start = Instant::now();
    let mut clock = clock(start);
    let mut p = planner(3, true);
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let now = start + Duration::from_millis(3500);
    p.request_stop(TrackId(1), &clock, now).unwrap();
    assert!(!p.stops.is_empty());
    p.launch(TrackId(1), 1, &mut clock, now).unwrap();
    assert!(p.stops.is_empty());
    assert_eq!(p.voices.last().unwrap().origin, BeatTime(6 * BEAT));
}

#[test]
fn follow_time_counts_from_the_launch_not_the_origin() {
    let mut p = planner(3, true);
    {
        let clip = Arc::make_mut(&mut p.project).tracks[0].slots[1]
            .clip
            .as_mut()
            .unwrap();
        clip.follow_actions.actions.enabled = true;
        clip.follow_actions.linked = false;
        clip.follow_actions.time = BeatTime(2 * BEAT);
    }
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    p.launch_at(TrackId(1), 1, BeatTime(7 * BEAT)).unwrap();
    assert_eq!(p.follow_beat(&p.voices[1]), Some(BeatTime(9 * BEAT)));
}

#[test]
fn published_position_follows_the_origin() {
    let mut p = planner(3, true);
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    p.launch_at(TrackId(1), 1, BeatTime(7 * BEAT)).unwrap();
    let voice = &p.voices[1];
    let published = published::PublishedTrack {
        region: super::Region::of(p.project.clip(voice.track, voice.slot).unwrap()),
        continuation: voice.continuation,
        track: voice.track,
        slot: voice.slot,
        clip: voice.clip,
        passes: voice.passes,
        launch: voice.launch,
        origin: voice.origin,
        length: BeatTime(3 * BEAT),
        looping: true,
        cutoff: None,
        next_follow_beat: None,
        queued_follow_slot: None,
        lanes: vec![],
    };
    assert_eq!(published.position_at(BeatTime(7 * BEAT)), BeatTime(BEAT));
    assert_eq!(
        published.position_at(BeatTime(8 * BEAT)),
        BeatTime(2 * BEAT)
    );
}

#[test]
fn an_edit_to_the_playing_legato_clip_keeps_its_position() {
    let start = Instant::now();
    let clock = clock(start);
    let mut p = planner(3, true);
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    p.launch_at(TrackId(1), 1, BeatTime(7 * BEAT)).unwrap();
    let now = start + Duration::from_millis(3750); // beat 7.5, content position 1.5
    let mut project = (*p.project).clone();
    project.tracks[0].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .events
        .pop(); // drop /b2
    p.update_project(project, &clock, now).unwrap();
    let b = &p.voices[1];
    assert_eq!(b.origin, BeatTime(6 * BEAT));
    // Event 1 was dispatched at the launch; the remaining one is /b0 of the next pass.
    assert_eq!(sends(&p, &clock, now, 2)[0].1, "/b0");
}
