//! Group row launches are one atomic, anchored batch with member slot semantics.
use super::*;
use crate::model::{ClipId, NoteValue, QuantisationValue, Slot, Track};

fn setup() -> (Instant, InternalClock, Planner) {
    let start = Instant::now();
    let mut project = super::tests::project(0.0, true);
    for id in 2..=5 {
        let mut track = crate::scheduler::tests::copy_track(&project.tracks[0], TrackId(id));
        for (row, slot) in track.slots.iter_mut().enumerate() {
            slot.clip.as_mut().unwrap().id = ClipId(id * 2 + row as u64);
        }
        project.tracks.push(track);
    }
    {
        let mut group = Track::new_group(TrackId(101), "group", project.scenes.len());
        group.colour = crate::model::LIVE_CLIP_PALETTE[0];
        group.fold_state = true;
        project.insert_group(group, &(1..=4).map(TrackId).collect::<Vec<_>>());
    }
    // Outer(Inner(Deep(1), 2), 3, 4), 5. All group folds are independent.
    for (id, parent, before) in [(102, 101, 1), (103, 102, 1)] {
        let mut group = Track::new_group(TrackId(id), "inner", project.scenes.len());
        group.group = Some(TrackId(parent));
        group.fold_state = true;
        let at = project
            .tracks
            .iter()
            .position(|t| t.id == TrackId(before))
            .unwrap();
        project.tracks.insert(at, group);
    }
    project[TrackId(1)].group = Some(TrackId(103));
    project[TrackId(2)].group = Some(TrackId(102));
    project.follow_actions_enabled = true;
    for scene in &mut project.scenes {
        scene.follow_actions.actions.enabled = true;
        scene.follow_actions.time = BeatTime(960);
    }
    project.validate().unwrap();
    project[TrackId(2)].slots[1].clip = None;
    project[TrackId(3)].slots[1].clip = None;
    project[TrackId(3)].slots[1].has_stop_button = false;
    let clock = InternalClock::new(project.tempo, project.time_signature);
    (
        start,
        clock,
        Planner::new(project, Duration::from_secs(3)).unwrap(),
    )
}

#[test]
fn stopped_batch_starts_all_clips_at_zero_without_launching_a_scene() {
    let (start, mut clock, mut planner) = setup();
    planner
        .launch_group_slot(TrackId(101), 1, &mut clock, start)
        .unwrap();
    assert!(clock.is_playing());
    assert_eq!(clock.instant_at(BeatTime::ZERO), Some(start));
    assert_eq!(planner.voices.len(), 2);
    assert!(
        planner
            .voices
            .iter()
            .all(|v| v.slot == 1 && v.launch == BeatTime::ZERO)
    );
    assert_eq!(planner.last_triggered_scene, None);
    assert!(planner.scene_timer.is_none());
}

#[test]
fn group_row_shares_boundary_and_stops_or_preserves_only_its_members() {
    let (start, mut clock, mut planner) = setup();
    clock.play(start);
    for id in 1..=5 {
        planner.launch_at(TrackId(id), 0, BeatTime::ZERO).unwrap();
    }
    let now = clock.instant_at(BeatTime(3839)).unwrap();
    planner
        .launch_group_slot(TrackId(101), 1, &mut clock, now)
        .unwrap();
    let triggers: Vec<_> = planner.voices.iter().filter(|v| v.slot == 1).collect();
    assert_eq!(triggers.len(), 2);
    assert!(triggers.iter().all(|v| v.launch == BeatTime(3840)));
    assert_eq!(
        planner.stops.get(&TrackId(2)).map(|stop| stop.beat),
        Some(BeatTime(3840))
    );
    for id in [3, 5] {
        let kept = planner
            .voices
            .iter()
            .find(|v| v.track == TrackId(id))
            .unwrap();
        assert_eq!(kept.slot, 0);
        assert_eq!(kept.cutoff, None);
        assert!(!planner.stops.contains_key(&TrackId(id)));
    }
    let count = planner.voices.len();
    planner
        .launch_group_slot(TrackId(101), 1, &mut clock, now)
        .unwrap();
    assert_eq!(planner.voices.len(), count);
}

#[test]
fn clip_overrides_use_the_same_clock_sample() {
    let (start, mut clock, mut planner) = setup();
    clock.play(start);
    Arc::make_mut(&mut planner.project)[TrackId(1)].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .quantisation = LaunchQuantisation::Value(QuantisationValue::Note(NoteValue::Quarter));
    let now = clock.instant_at(BeatTime(959)).unwrap();
    planner
        .launch_group_slot(TrackId(101), 1, &mut clock, now)
        .unwrap();
    let launch = |id| {
        planner
            .voices
            .iter()
            .find(|v| v.track == TrackId(id))
            .unwrap()
            .launch
    };
    assert_eq!(launch(1), BeatTime(960));
    assert_eq!(launch(4), BeatTime(3840));
}

#[test]
fn invalid_batch_is_atomic_and_all_empty_batch_does_not_start_transport() {
    let (start, mut clock, mut planner) = setup();
    assert!(
        planner
            .launch_group_slot(TrackId(1099), 1, &mut clock, start)
            .is_err()
    );
    assert!(
        planner
            .launch_group_slot(TrackId(101), 99, &mut clock, start)
            .is_err()
    );
    Arc::make_mut(&mut planner.project)[TrackId(4)].slots.pop();
    assert!(
        planner
            .launch_group_slot(TrackId(101), 1, &mut clock, start)
            .is_err()
    );
    assert!(planner.voices.is_empty());
    assert!(!clock.is_playing());
    assert!(planner.stops.is_empty());
    Arc::make_mut(&mut planner.project)[TrackId(4)]
        .slots
        .push(Slot::default());
    for track in &mut Arc::make_mut(&mut planner.project).tracks {
        track.slots[1].clip = None;
    }
    planner
        .launch_group_slot(TrackId(101), 1, &mut clock, start)
        .unwrap();
    assert!(!clock.is_playing());
    assert!(planner.voices.is_empty());
}

#[test]
fn nested_inner_stop_cancels_pending_launches_only_in_its_subtree() {
    let (start, mut clock, mut planner) = setup();
    clock.play(start);
    for id in 1..=5 {
        planner.launch_at(TrackId(id), 0, BeatTime::ZERO).unwrap();
    }
    let now = clock.instant_at(BeatTime(100)).unwrap();
    planner
        .launch_group_slot(TrackId(101), 1, &mut clock, now)
        .unwrap();
    planner
        .request_stop_group(TrackId(102), &clock, now)
        .unwrap();
    assert!(
        !planner
            .voices
            .iter()
            .any(|v| v.track == TrackId(1) && v.slot == 1)
    );
    assert!(
        planner
            .voices
            .iter()
            .any(|v| v.track == TrackId(4) && v.slot == 1)
    );
    for id in [1, 2] {
        assert_eq!(planner.stops[&TrackId(id)].beat, BeatTime(3840));
    }
    assert!(!planner.stops.contains_key(&TrackId(3)));
    assert!(!planner.stops.contains_key(&TrackId(5)));
    planner
        .request_stop_group(TrackId(101), &clock, now)
        .unwrap();
    assert_eq!(planner.stops.len(), 4);
    assert!(planner.voices.iter().all(|v| v.slot == 0));
    assert!(planner.stops.keys().all(|id| (1..=4).contains(&id.0)));
}

#[test]
fn nested_inner_launch_and_scene_launch_visit_only_leaves_once() {
    let (start, mut clock, mut planner) = setup();
    planner
        .launch_group_slot(TrackId(102), 0, &mut clock, start)
        .unwrap();
    let ids: Vec<_> = planner.voices.iter().map(|v| v.track).collect();
    assert_eq!(ids, [TrackId(1), TrackId(2)]);
    assert_eq!(planner.serial, 2);
    planner.stop_all();
    planner.launch_scene(SceneId(1), &mut clock, start).unwrap();
    assert_eq!(planner.voices.len(), 5);
    assert_eq!(planner.serial, 7, "scene must not also fire group rows");
    assert!(planner.voices.iter().all(|v| (1..=5).contains(&v.track.0)));
}
