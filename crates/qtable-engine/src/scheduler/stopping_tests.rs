//! Slot buttons and group stops share the track-stop planner.
use super::*;
use crate::model::*;

fn setup() -> (Instant, InternalClock, Planner) {
    let start = Instant::now();
    let mut project = super::tests::project(0.0, true);
    let mut second = crate::scheduler::tests::copy_track(&project.tracks[0], TrackId(2));
    for slot in &mut second.slots {
        slot.clip.as_mut().unwrap().id.0 += 10;
    }
    let mut outside = crate::scheduler::tests::copy_track(&second, TrackId(3));
    for slot in &mut outside.slots {
        slot.clip.as_mut().unwrap().id.0 += 10;
    }
    project.tracks.extend([second, outside]);
    {
        let mut group = Track::new_group(TrackId(107), "Group", project.scenes.len());
        group.colour = crate::model::LIVE_CLIP_PALETTE[0];
        group.route = project.tracks[0].route.clone();
        project.insert_group(group, &[TrackId(1), TrackId(2)]);
    }
    let mut clock = InternalClock::new(project.tempo, project.time_signature);
    clock.play(start);
    (
        start,
        clock,
        Planner::new(project, Duration::from_secs(3)).unwrap(),
    )
}

#[test]
fn empty_slot_buttons_stop_on_global_grid_and_no_button_preserves_triggers() {
    let (_, mut clock, mut planner) = setup();
    Arc::make_mut(&mut planner.project)[TrackId(1)].slots[1].clip = None;
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let now = clock.instant_at(BeatTime(123)).unwrap();
    planner
        .trigger_slot(TrackId(1), 1, &mut clock, now)
        .unwrap();
    assert_eq!(
        planner.stops.get(&TrackId(1)).map(|stop| stop.beat),
        Some(BeatTime(3840))
    );
    Arc::make_mut(&mut planner.project)[TrackId(1)].slots[1].has_stop_button = false;
    planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
    let before = planner.voices.len();
    planner
        .trigger_slot(TrackId(1), 1, &mut clock, now)
        .unwrap();
    assert_eq!(planner.voices.len(), before);
    assert!(planner.stops.is_empty());
    Arc::make_mut(&mut planner.project)[TrackId(1)].slots[1].has_stop_button = true;
    planner
        .trigger_slot(TrackId(1), 1, &mut clock, now)
        .unwrap();
    assert!(planner.voices.iter().all(|v| v.launch == BeatTime::ZERO));
    Arc::make_mut(&mut planner.project).global_quantisation = GlobalQuantisation::None;
    assert!(
        planner
            .trigger_slot(TrackId(1), 1, &mut clock, now)
            .unwrap()
    );
    assert!(planner.voices.is_empty());
    clock.stop(now);
    planner
        .trigger_slot(TrackId(1), 1, &mut clock, now)
        .unwrap();
    assert!(!clock.is_playing());
    assert!(
        planner
            .trigger_slot(TrackId(1), 99, &mut clock, now)
            .is_err()
    );
}

#[test]
fn group_stop_cancels_trigger_only_tracks_and_publishes_active_track_stop() {
    let (_, mut clock, mut planner) = setup();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    planner.launch_at(TrackId(3), 0, BeatTime::ZERO).unwrap();
    let now = clock.instant_at(BeatTime(123)).unwrap();
    planner.launch(TrackId(2), 0, &mut clock, now).unwrap();
    assert_eq!(
        planner
            .request_stop_group(TrackId(107), &clock, now)
            .unwrap(),
        vec![TrackId(2)]
    );
    assert!(!planner.voices.iter().any(|v| v.track == TrackId(2)));
    assert!(planner.voices.iter().any(|v| v.track == TrackId(3)));
    assert_eq!(
        planner.stops.get(&TrackId(1)).map(|stop| stop.beat),
        Some(BeatTime(3840))
    );
    let mut state = PublishedState {
        playing: false,
        tempo: planner.project.tempo,
        signature: planner.project.time_signature,
        global_quantisation: planner.project.global_quantisation,
        pending_stops: vec![],
        pending_stop_slots: Vec::new(),
        last_triggered_scene: None,
        queued_follow_scene: None,
        beat: BeatTime::ZERO,
        anchor: now,
        tracks: vec![],
        clip_positions: vec![],
        lane_positions: vec![],
        automation: vec![],
        failure: None,
    };
    state.publish(&planner, &clock, now);
    assert_eq!(state.pending_stops, vec![(TrackId(1), BeatTime(3840))]);
    assert!(
        planner
            .request_stop_group(TrackId(1099), &clock, now)
            .is_err()
    );
    let boundary = clock.instant_at(BeatTime(3840)).unwrap();
    for event in planner.plan(&clock, boundary) {
        if matches!(event.action, PlannedAction::StopTrack(_)) {
            planner.consume(&event);
        }
    }
    assert!(!planner.voices.iter().any(|v| v.track == TrackId(1)));
    assert!(planner.voices.iter().any(|v| v.track == TrackId(3)));
}

#[test]
fn scheduler_packets_use_group_target_and_track_override() {
    let (start, clock, mut planner) = setup();
    let mut inherited = planner.project.outputs[0].clone();
    inherited.id = TrackId(277);
    inherited = inherited.with_path("/group");
    Arc::make_mut(&mut planner.project).outputs.push(inherited);
    Arc::make_mut(&mut planner.project)[TrackId(107)].route =
        crate::model::Route::single(TrackId(277));
    Arc::make_mut(&mut planner.project)[TrackId(1)].route = Route::Group;
    for track in Arc::make_mut(&mut planner.project)
        .tracks
        .iter_mut()
        .filter(|t| !t.is_group())
        .take(2)
    {
        let mut clip = track.slots[0].clip.take().unwrap();
        for event in &mut clip.events {
            let path = OscAddress::Relative("cue".into());
            let id = DeviceId(1000 + track.id.0);
            *event = track.hit(event.position, path, vec![], || id);
        }
        track.slots[0].clip = Some(clip);
    }
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    planner.launch_at(TrackId(2), 0, BeatTime::ZERO).unwrap();
    let addresses: Vec<_> = planner
        .plan(&clock, start)
        .into_iter()
        .filter_map(|event| match event.action {
            PlannedAction::Send {
                packet: Some(OscPacket::Message(message)),
                ..
            } => Some(message.addr),
            _ => None,
        })
        .collect();
    assert!(addresses.iter().any(|addr| addr == "/group/cue"));
    let override_prefix = planner.project.outputs[0].path.as_str();
    assert!(
        addresses
            .iter()
            .any(|addr| addr == &format!("{override_prefix}/cue"))
    );
}

fn published(planner: &Planner, clock: &InternalClock, now: Instant) -> PublishedState {
    let mut state = PublishedState {
        playing: false,
        tempo: planner.project.tempo,
        signature: planner.project.time_signature,
        global_quantisation: planner.project.global_quantisation,
        pending_stops: vec![],
        pending_stop_slots: vec![],
        last_triggered_scene: None,
        queued_follow_scene: None,
        beat: BeatTime::ZERO,
        anchor: now,
        tracks: vec![],
        clip_positions: vec![],
        lane_positions: vec![],
        automation: vec![],
        failure: None,
    };
    state.publish(planner, clock, now);
    state
}

#[test]
fn stop_source_follows_slot_scene_and_group_requests_and_clears_with_the_stop() {
    let (_, mut clock, mut planner) = setup();
    for id in [TrackId(1), TrackId(2)] {
        Arc::make_mut(&mut planner.project)[id].slots[1].clip = None;
        planner.launch_at(id, 0, BeatTime::ZERO).unwrap();
    }
    let now = clock.instant_at(BeatTime(123)).unwrap();
    planner
        .trigger_slot(TrackId(1), 1, &mut clock, now)
        .unwrap();
    let state = published(&planner, &clock, now);
    assert_eq!(state.pending_stop_slots, vec![(TrackId(1), 1)]);
    assert_eq!(state.pending_stops, vec![(TrackId(1), BeatTime(3840))]);
    // Publication still shows the old voice playing, without a follow-icon pulse.
    assert!(state.tracks.iter().any(|v| v.track == TrackId(1)
        && v.slot == 0
        && v.launch == BeatTime::ZERO
        && v.queued_follow_slot.is_none()));

    // A track-level stop has no destination slot. Repeated requests keep the boundary.
    planner.request_stop(TrackId(1), &clock, now).unwrap();
    assert!(
        published(&planner, &clock, now)
            .pending_stop_slots
            .is_empty()
    );
    planner.launch_scene(SceneId(2), &mut clock, now).unwrap();
    assert_eq!(
        published(&planner, &clock, now).pending_stop_slots,
        vec![(TrackId(1), 1), (TrackId(2), 1)]
    );
    planner
        .launch_group_slot(TrackId(107), 1, &mut clock, now)
        .unwrap();
    assert_eq!(published(&planner, &clock, now).pending_stop_slots.len(), 2);
    // A replacement launch cancels both the deadline and the slot indication.
    planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
    assert_eq!(
        published(&planner, &clock, now).pending_stop_slots,
        vec![(TrackId(2), 1)]
    );
    let boundary = clock.instant_at(BeatTime(3840)).unwrap();
    for event in planner.plan(&clock, boundary) {
        if matches!(event.action, PlannedAction::StopTrack(_)) {
            planner.consume(&event);
        }
    }
    let state = published(&planner, &clock, boundary);
    assert!(state.pending_stops.is_empty());
    assert!(state.pending_stop_slots.is_empty());
}

#[test]
fn stop_source_uses_scene_identity_and_hides_when_its_stop_button_disappears() {
    let (_, mut clock, mut planner) = setup();
    Arc::make_mut(&mut planner.project)[TrackId(1)].slots[1].clip = None;
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let now = clock.instant_at(BeatTime(123)).unwrap();
    planner
        .trigger_slot(TrackId(1), 1, &mut clock, now)
        .unwrap();
    let project = Arc::make_mut(&mut planner.project);
    project.scenes.swap(0, 1);
    for track in &mut project.tracks {
        track.slots.swap(0, 1);
    }
    assert_eq!(
        published(&planner, &clock, now).pending_stop_slots,
        vec![(TrackId(1), 0)]
    );
    Arc::make_mut(&mut planner.project)[TrackId(1)].slots[0].has_stop_button = false;
    let state = published(&planner, &clock, now);
    assert!(state.pending_stop_slots.is_empty());
    assert_eq!(state.pending_stops.len(), 1);
    Arc::make_mut(&mut planner.project).scenes.remove(0);
    assert!(
        published(&planner, &clock, now)
            .pending_stop_slots
            .is_empty()
    );
}
