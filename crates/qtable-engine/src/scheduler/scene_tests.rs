//! Scene row decisions use the same deterministic launch and stop machinery.
use super::*;
use crate::model::{ClipId, NoteValue, QuantisationValue, Track};

fn setup() -> (Instant, InternalClock, Planner) {
    let start = Instant::now();
    let mut project = super::tests::project(0.0, true);
    for id in 2..=4 {
        let mut track = crate::scheduler::tests::copy_track(&project.tracks[0], TrackId(id));
        for (row, slot) in track.slots.iter_mut().enumerate() {
            slot.clip.as_mut().unwrap().id = ClipId(id * 2 + row as u64);
        }
        project.tracks.push(track);
    }
    project.tracks[1].slots[1].clip = None;
    project.tracks[2].slots[1].clip = None;
    project.tracks[2].slots[1].has_stop_button = false;
    project.tracks[0].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::Value(QuantisationValue::Note(NoteValue::Quarter));
    let mut clock = InternalClock::new(project.tempo, project.time_signature);
    clock.play(start);
    (
        start,
        clock,
        Planner::new(project, Duration::from_secs(3)).unwrap(),
    )
}

#[test]
fn mixed_row_replaces_clips_stops_or_keeps_tracks_on_their_own_grids() {
    let (_, mut clock, mut planner) = setup();
    for id in 1..=4 {
        planner.launch_at(TrackId(id), 0, BeatTime::ZERO).unwrap();
    }
    let now = clock.instant_at(BeatTime(123)).unwrap();
    assert!(
        planner
            .launch_scene(SceneId(2), &mut clock, now)
            .unwrap()
            .is_empty()
    );
    assert_eq!(planner.last_triggered_scene, Some(SceneId(2)));
    let trigger = |id| {
        planner
            .voices
            .iter()
            .find(|v| v.track == TrackId(id) && v.slot == 1)
            .unwrap()
            .launch
    };
    assert_eq!(trigger(1), BeatTime(960));
    assert_eq!(trigger(4), BeatTime(3840));
    assert_eq!(
        planner.stops.get(&TrackId(2)).map(|stop| stop.beat),
        Some(BeatTime(3840))
    );
    assert!(!planner.stops.contains_key(&TrackId(3)));
    let kept = planner
        .voices
        .iter()
        .find(|v| v.track == TrackId(3))
        .unwrap();
    assert_eq!(kept.slot, 0);
    assert_eq!(kept.cutoff, None);
    let replaced = planner
        .voices
        .iter()
        .find(|v| v.track == TrackId(1) && v.slot == 0)
        .unwrap();
    assert_eq!(replaced.cutoff, Some(BeatTime(960)));
}

#[test]
fn stopped_scene_launches_every_clip_at_zero_and_invalid_scene_changes_nothing() {
    let (start, mut clock, mut planner) = setup();
    clock.stop(start);
    assert!(
        planner
            .launch_scene(SceneId(99), &mut clock, start)
            .is_err()
    );
    assert!(!clock.is_playing());
    assert!(planner.voices.is_empty());
    assert_eq!(planner.last_triggered_scene, None);
    planner.launch_scene(SceneId(2), &mut clock, start).unwrap();
    assert!(clock.is_playing());
    assert_eq!(planner.voices.len(), 2);
    assert!(planner.voices.iter().all(|v| v.launch == BeatTime::ZERO));
}

#[test]
fn empty_scene_keeps_transport_stopped_and_failed_row_is_atomic() {
    let (start, mut clock, mut planner) = setup();
    clock.stop(start);
    for track in &mut Arc::make_mut(&mut planner.project).tracks {
        track.slots[1].clip = None;
    }
    planner.launch_scene(SceneId(2), &mut clock, start).unwrap();
    assert!(!clock.is_playing());
    assert_eq!(planner.last_triggered_scene, Some(SceneId(2)));

    let (start, mut clock, mut planner) = setup();
    clock.stop(start);
    // Simulate a row which cannot launch its final clip. Earlier changes roll back.
    Arc::make_mut(&mut planner.project).tracks[3].slots.pop();
    assert!(planner.launch_scene(SceneId(2), &mut clock, start).is_err());
    assert!(!clock.is_playing());
    assert!(planner.voices.is_empty());
    assert_eq!(planner.last_triggered_scene, None);
}

#[test]
fn scene_stop_cancels_trigger_and_none_stop_reports_silence() {
    let (start, mut clock, mut planner) = setup();
    planner.launch(TrackId(2), 0, &mut clock, start).unwrap();
    assert_eq!(
        planner.launch_scene(SceneId(2), &mut clock, start).unwrap(),
        vec![TrackId(2)]
    );
    assert!(!planner.voices.iter().any(|v| v.track == TrackId(2)));
    Arc::make_mut(&mut planner.project).global_quantisation = GlobalQuantisation::None;
    planner.launch_at(TrackId(2), 0, BeatTime::ZERO).unwrap();
    assert_eq!(
        planner.launch_scene(SceneId(2), &mut clock, start).unwrap(),
        vec![TrackId(2)]
    );
}

#[test]
fn scene_clip_none_is_immediate_and_repeated_triggers_keep_their_boundaries() {
    let (_, mut clock, mut planner) = setup();
    Arc::make_mut(&mut planner.project).tracks[3].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::None;
    let now = clock.instant_at(BeatTime(123)).unwrap();
    planner.launch_scene(SceneId(2), &mut clock, now).unwrap();
    assert_eq!(
        planner
            .voices
            .iter()
            .find(|v| v.track == TrackId(4))
            .unwrap()
            .launch,
        BeatTime(123)
    );
    let later = clock.instant_at(BeatTime(456)).unwrap();
    planner.launch_scene(SceneId(2), &mut clock, later).unwrap();
    assert_eq!(
        planner
            .voices
            .iter()
            .find(|v| v.track == TrackId(1))
            .unwrap()
            .launch,
        BeatTime(960)
    );
}

#[test]
fn a_group_track_does_not_change_what_a_scene_row_launches_stops_or_keeps() {
    let (_, mut clock, mut plain) = setup();
    let mut grouped = plain.clone();
    {
        let project = Arc::make_mut(&mut grouped.project);
        let group = Track::new_group(TrackId(101), "G", project.scenes.len());
        project.insert_group(group, &[TrackId(1), TrackId(2)]);
        project.validate().unwrap();
    }
    let now = clock.instant_at(BeatTime(123)).unwrap();
    let mut summaries = Vec::new();
    for planner in [&mut plain, &mut grouped] {
        for id in 1..=4 {
            planner.launch_at(TrackId(id), 0, BeatTime::ZERO).unwrap();
        }
        let immediate = planner.launch_scene(SceneId(2), &mut clock, now).unwrap();
        let mut stops: Vec<_> = planner.stops.iter().map(|(t, b)| (t.0, b.beat.0)).collect();
        stops.sort();
        let mut voices: Vec<_> = planner
            .voices
            .iter()
            .map(|v| (v.track.0, v.slot, v.launch.0))
            .collect();
        voices.sort();
        summaries.push((immediate, stops, voices));
    }
    assert_eq!(summaries[0], summaries[1]);
    assert!(!grouped.stops.contains_key(&TrackId(101)));
}
