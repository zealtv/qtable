//! Live 12.1, 2026-10-11, over AbletonOSC: track Stop and Stop All suppress clip follows;
//! group Stop is inferred.
use super::*;
use crate::model::*;

const BAR: u64 = 3840;

fn setup(action: FollowAction) -> (Planner, InternalClock) {
    let mut project = super::tests::project(0.0, true);
    project.random_seed = 42;
    project.global_quantisation = GlobalQuantisation::Value(QuantisationValue::Bars(4));
    for slot in &mut project.tracks[0].slots {
        let clip = slot.clip.as_mut().unwrap();
        clip.end = BeatTime(BAR);
        clip.loop_end = BeatTime(BAR);
        clip.events.truncate(1);
        clip.events[0].position = BeatTime::ZERO;
        clip.follow_actions.actions.enabled = true;
        clip.follow_actions.actions.action_a = action;
        clip.follow_actions.linked = true;
    }
    for id in 2..=3 {
        let mut track = crate::scheduler::tests::copy_track(&project.tracks[0], TrackId(id));
        for slot in &mut track.slots {
            slot.clip.as_mut().unwrap().id.0 += id * 10;
        }
        project.tracks.push(track);
    }
    {
        let mut group = Track::new_group(TrackId(101), "band", project.scenes.len());
        group.colour = crate::model::LIVE_CLIP_PALETTE[0];
        project.insert_group(group, &[TrackId(1), TrackId(2)]);
    }
    let mut clock = InternalClock::new(project.tempo, project.time_signature);
    clock.play(Instant::now());
    let mut planner = Planner::new(project, Duration::ZERO).unwrap();
    for id in 1..=3 {
        planner.launch_at(TrackId(id), 0, BeatTime::ZERO).unwrap();
    }
    (planner, clock)
}

fn advance(planner: &mut Planner, clock: &InternalClock, beat: u64) -> Vec<DueEvent> {
    let events = planner.plan(clock, clock.instant_at(BeatTime(beat)).unwrap());
    for event in &events {
        planner.consume_action(event).unwrap();
    }
    events
}

#[test]
fn track_group_and_all_stops_suppress_follow_until_the_boundary() {
    for scope in 0..3 {
        let (mut p, clock) = setup(FollowAction::Next);
        advance(&mut p, &clock, 0);
        let now = clock.instant_at(BeatTime(1)).unwrap();
        let stopped = match scope {
            0 => {
                p.request_stop(TrackId(1), &clock, now).unwrap();
                vec![TrackId(1)]
            }
            1 => {
                p.request_stop_group(TrackId(101), &clock, now).unwrap();
                vec![TrackId(1), TrackId(2)]
            }
            _ => {
                p.request_stop_all(&clock, now).unwrap();
                vec![TrackId(1), TrackId(2), TrackId(3)]
            }
        };
        for bar in 1..4 {
            let events = advance(&mut p, &clock, bar * BAR);
            for &track in &stopped {
                let voice = p.voices.iter().find(|v| v.track == track).unwrap();
                assert_eq!((voice.slot, voice.launch), (0, BeatTime::ZERO));
                assert!(voice.follow_done);
                assert_eq!(
                    p.stops.get(&track).map(|stop| stop.beat),
                    Some(BeatTime(4 * BAR))
                );
                // The original clip still emits its loop event every bar.
                assert!(events.iter().any(|e| {
                    e.voice == voice.id && matches!(e.action, PlannedAction::Send { .. })
                }));
            }
            if scope < 2 {
                // Stopping a track/group leaves outside tracks following normally.
                assert!(
                    p.voices
                        .iter()
                        .any(|v| { v.track == TrackId(3) && v.launch == BeatTime(bar * BAR) })
                );
            }
        }
        advance(&mut p, &clock, 4 * BAR);
        for track in stopped {
            assert!(!p.voices.iter().any(|v| v.track == track));
            assert!(!p.stops.contains_key(&track));
        }
    }
}

#[test]
fn suppressed_timer_is_spent_without_drawing_or_deferring_any_action() {
    for action in [
        FollowAction::Next,
        FollowAction::PlayAgain,
        FollowAction::Stop,
        FollowAction::Other,
        FollowAction::Jump(SceneId(2)),
    ] {
        let (mut p, clock) = setup(action);
        p.stop_now(TrackId(2));
        p.stop_now(TrackId(3));
        let now = clock.instant_at(BeatTime(1)).unwrap();
        p.request_stop(TrackId(1), &clock, now).unwrap();
        let rng = p.rng;
        advance(&mut p, &clock, BAR);
        assert!(p.voices[0].follow_done);
        assert_eq!(p.rng, rng);
        // Isolate spending from voice replacement: removing the pending stop
        // cannot resurrect the suppressed timer on the original voice.
        p.stops.remove(&TrackId(1));
        let events = advance(&mut p, &clock, 2 * BAR);
        assert!(
            events
                .iter()
                .all(|e| !matches!(e.action, PlannedAction::Follow { .. }))
        );
        assert_eq!((p.voices[0].slot, p.voices[0].launch), (0, BeatTime::ZERO));
    }
}

#[test]
fn manual_launch_cancels_stop_and_arms_a_new_follow_timer() {
    let (mut p, mut clock) = setup(FollowAction::Next);
    let now = clock.instant_at(BeatTime(1)).unwrap();
    p.request_stop(TrackId(1), &clock, now).unwrap();
    advance(&mut p, &clock, BAR);
    let at = clock.instant_at(BeatTime(BAR + 1)).unwrap();
    assert_eq!(
        p.launch(TrackId(1), 1, &mut clock, at).unwrap(),
        BeatTime(4 * BAR)
    );
    assert!(!p.stops.contains_key(&TrackId(1)));
    for bar in 2..=5 {
        advance(&mut p, &clock, bar * BAR);
    }
    assert!(
        p.voices
            .iter()
            .any(|v| { v.track == TrackId(1) && v.slot == 0 && v.launch == BeatTime(5 * BAR) })
    );
}

#[test]
fn stop_requested_after_planning_still_suppresses_follow_on_consumption() {
    let (mut p, clock) = setup(FollowAction::Next);
    let at = clock.instant_at(BeatTime(BAR)).unwrap();
    let event = p
        .plan(&clock, at)
        .into_iter()
        .find(|e| e.voice == p.voices[0].id && matches!(e.action, PlannedAction::Follow { .. }))
        .unwrap();
    p.request_stop(TrackId(1), &clock, clock.instant_at(BeatTime(1)).unwrap())
        .unwrap();
    p.consume_action(&event).unwrap();
    assert!(p.voices[0].follow_done);
    assert_eq!(p.voices[0].slot, 0);
    assert_eq!(
        p.stops.get(&TrackId(1)).map(|stop| stop.beat),
        Some(BeatTime(4 * BAR))
    );
}
