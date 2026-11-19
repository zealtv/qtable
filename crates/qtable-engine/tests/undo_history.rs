//! The engine's undo history: which commands make steps, what
//! availability and labels read as, content identity, and how a restore plays.
use qtable_engine::{
    edit::ProjectEdit,
    engine::{Command, Engine},
    model::*,
    scheduler::AutomationState,
};
use qtable_test_support::TestReceiver;
use rosc::OscType;
use std::time::{Duration, Instant};

const OUT: TrackId = TrackId(201);

fn volume() -> ParameterRef {
    ParameterRef::volume(TrackId(1))
}
fn volume_field() -> FieldRef {
    FieldRef {
        parameter: volume(),
        field: FieldKey("value".into()),
    }
}

/// Two clip tracks and a group on one output. Track 1's clip is a looping 16-beat Volume ramp
/// (wire value `10 + 10 * v` on `/volume`); track 2 holds a looping hit clip. Tempo 120.
fn project(port: u16) -> Project {
    let mut one = Track::new(TrackId(1), "One", 2);
    one.route = Route::single(OUT);
    let out = &mut one.mixer_device.volume.output;
    out.enabled = true;
    out.address = "/volume".into();
    out.send_mode = SendMode::OnRelease;
    out.mapping = Mapping::new([0.0, 1.0], [10.0, 20.0], 1.0, 0.0).unwrap();
    let mut ramp = Clip::new(ClipId(1), "Ramp", BeatTime(16 * 960), LIVE_CLIP_PALETTE[0]).unwrap();
    ramp.lanes.push(Lane::Envelope(AutomationEnvelope {
        region: None,
        target: ArgumentRef::volume(),
        points: vec![
            Breakpoint {
                time: BeatTime(0),
                value: 0.0,
                curve: 0.0,
            },
            Breakpoint {
                time: BeatTime(16 * 960),
                value: 1.0,
                curve: 0.0,
            },
        ],
    }));
    one.slots[0].clip = Some(ramp);
    let mut two = Track::new(TrackId(2), "Two", 2);
    two.route = Route::single(OUT);
    let mut hits = Clip::new(ClipId(2), "Hits", BeatTime(960), LIVE_CLIP_PALETTE[1]).unwrap();
    hits.events = vec![two.hit(
        BeatTime(0),
        OscAddress::Relative("/hit".into()),
        vec![],
        || DeviceId(1),
    )];
    two.slots[0].clip = Some(hits);
    Project {
        tempo: Tempo::new(120.0).unwrap(),
        tracks: vec![one, two],
        outputs: vec![Track::new_output(OUT, "Out", "127.0.0.1", port)],
        scenes: vec![
            Scene {
                id: SceneId(1),
                name: "Scene".into(),
                follow_actions: Default::default(),
            },
            Scene {
                id: SceneId(2),
                name: "Scene".into(),
                follow_actions: Default::default(),
            },
        ],
        global_quantisation: GlobalQuantisation::None,
        ..Project::default()
    }
}

fn engine() -> (Engine, TestReceiver) {
    let receiver = TestReceiver::bind().unwrap();
    let engine = Engine::new(project(receiver.addr().port())).unwrap();
    (engine, receiver)
}

fn rename(engine: &mut Engine, name: &str) {
    engine
        .execute(Command::Edit(ProjectEdit::RenameClip {
            track: TrackId(1),
            slot: 0,
            name: name.into(),
        }))
        .unwrap();
}
fn clip_name(engine: &Engine) -> String {
    engine.project().clip(TrackId(1), 0).unwrap().name.clone()
}
fn set_volume(engine: &mut Engine, value: f64) {
    engine
        .execute(Command::Edit(ProjectEdit::SetParameter {
            target: volume(),
            values: vec![(FieldKey("value".into()), value)],
        }))
        .unwrap();
}
fn stored_volume(engine: &Engine) -> f64 {
    engine.project().field(&volume_field()).unwrap().value
}

#[test]
fn an_edit_makes_a_labelled_step_that_undo_and_redo_walk() {
    let (mut engine, _receiver) = engine();
    assert!(!engine.can_undo() && !engine.can_redo());
    assert_eq!((engine.undo_label(), engine.redo_label()), (None, None));
    rename(&mut engine, "A");
    rename(&mut engine, "B");
    assert_eq!(engine.undo_label(), Some("Rename Clip"));
    let report = engine.execute(Command::Undo).unwrap();
    assert_eq!(report.history_step, Some("Rename Clip"));
    assert_eq!(clip_name(&engine), "A");
    assert!(engine.can_undo() && engine.can_redo());
    assert_eq!(engine.redo_label(), Some("Rename Clip"));
    engine.execute(Command::Undo).unwrap();
    assert_eq!(clip_name(&engine), "Ramp");
    assert!(!engine.can_undo());
    // Nothing left to undo is not an error and changes nothing.
    let revision = engine.project_revision();
    let report = engine.execute(Command::Undo).unwrap();
    assert_eq!(report.history_step, None);
    assert_eq!(engine.project_revision(), revision);
    engine.execute(Command::Redo).unwrap();
    let report = engine.execute(Command::Redo).unwrap();
    assert_eq!(report.history_step, Some("Rename Clip"));
    assert_eq!(clip_name(&engine), "B");
    assert!(!engine.can_redo());
    let report = engine.execute(Command::Redo).unwrap();
    assert_eq!(report.history_step, None);
    assert!(report.is_empty());
    assert_eq!(clip_name(&engine), "B");
}

#[test]
fn project_revision_counts_every_change_including_undo_and_redo() {
    let (mut engine, _receiver) = engine();
    let start = engine.project_revision();
    rename(&mut engine, "A");
    engine.execute(Command::Undo).unwrap();
    engine.execute(Command::Redo).unwrap();
    assert_eq!(engine.project_revision(), start + 3);
}

#[test]
fn launches_stops_transport_and_fires_make_no_step() {
    let (mut engine, _receiver) = engine();
    for command in [
        Command::Play,
        Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        },
        Command::LaunchScene { scene: SceneId(1) },
        Command::StopTrack { track: TrackId(1) },
        Command::CancelSceneLaunch,
        Command::ReEnableAutomation { field: None },
        Command::StopAll,
        Command::Stop,
    ] {
        engine.execute(command).unwrap();
    }
    assert!(!engine.can_undo());
}

#[test]
fn view_state_never_makes_a_step_and_a_change_that_leaves_the_set_equal_makes_none() {
    let (mut engine, _receiver) = engine();
    engine
        .execute(Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        }))
        .unwrap();
    let group = engine
        .project()
        .tracks
        .iter()
        .find(|t| t.is_group())
        .unwrap()
        .id;
    engine
        .execute(Command::Edit(ProjectEdit::SetGroupFolded {
            group,
            folded: true,
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::SetDeviceExpanded {
            device: engine.project().track(TrackId(2)).unwrap().devices[0].id,
            expanded: true,
        }))
        .unwrap();
    assert_eq!(engine.undo_label(), Some("Group Tracks"));
    // The same name, the same value, the same tempo, quantisation and switch.
    rename(&mut engine, "Ramp");
    let stored = stored_volume(&engine);
    set_volume(&mut engine, stored);
    engine.execute(Command::SetTempo(120.0)).unwrap();
    let quantisation = engine.project().global_quantisation;
    engine
        .execute(Command::SetGlobalQuantisation(quantisation))
        .unwrap();
    engine
        .execute(Command::SetFollowActionsEnabled(
            engine.project().follow_actions_enabled,
        ))
        .unwrap();
    engine.execute(Command::Undo).unwrap();
    assert!(!engine.can_undo(), "only the group step existed");
}

#[test]
fn tempo_quantisation_and_the_follow_switch_make_steps() {
    let (mut engine, _receiver) = engine();
    let before = engine.project().clone();
    engine.execute(Command::SetTempo(90.0)).unwrap();
    assert_eq!(engine.undo_label(), Some("Set Tempo"));
    engine
        .execute(Command::SetGlobalQuantisation(GlobalQuantisation::Value(
            QuantisationValue::Bars(2),
        )))
        .unwrap();
    assert_eq!(engine.undo_label(), Some("Set Global Quantisation"));
    engine
        .execute(Command::SetFollowActionsEnabled(false))
        .unwrap();
    assert_eq!(engine.undo_label(), Some("Set Follow Actions"));
    let after = engine.project().clone();
    assert_ne!(after, before);
    for _ in 0..3 {
        engine.execute(Command::Undo).unwrap();
    }
    assert_eq!(engine.project(), &before);
    for _ in 0..3 {
        engine.execute(Command::Redo).unwrap();
    }
    assert_eq!(engine.project(), &after);
}

#[test]
fn a_new_edit_clears_redo() {
    let (mut engine, _receiver) = engine();
    rename(&mut engine, "A");
    engine.execute(Command::Undo).unwrap();
    assert!(engine.can_redo());
    rename(&mut engine, "B");
    assert!(!engine.can_redo());
    engine.execute(Command::Redo).unwrap();
    assert_eq!(clip_name(&engine), "B");
}

#[test]
fn loading_a_set_clears_the_history() {
    let (mut engine, receiver) = engine();
    rename(&mut engine, "A");
    rename(&mut engine, "B");
    engine.execute(Command::Undo).unwrap();
    engine
        .execute(Command::LoadProject(project(receiver.addr().port())))
        .unwrap();
    assert!(!engine.can_undo() && !engine.can_redo());
    let loaded = engine.project().clone();
    engine.execute(Command::Undo).unwrap();
    engine.execute(Command::Redo).unwrap();
    assert_eq!(engine.project(), &loaded);
}

#[test]
fn the_history_keeps_the_latest_two_hundred_steps() {
    let (mut engine, _receiver) = engine();
    for n in 1..=201 {
        rename(&mut engine, &format!("name {n}"));
    }
    let mut undone = 0;
    while engine.can_undo() {
        engine.execute(Command::Undo).unwrap();
        undone += 1;
    }
    assert_eq!(undone, 200);
    // The oldest step (Ramp to "name 1") was dropped.
    assert_eq!(clip_name(&engine), "name 1");
    for _ in 0..200 {
        engine.execute(Command::Redo).unwrap();
    }
    assert_eq!(clip_name(&engine), "name 201");
}

#[test]
fn content_identity_follows_the_content_through_undo_and_redo() {
    let (mut engine, _receiver) = engine();
    let start = engine.content_id();
    rename(&mut engine, "A");
    let saved = engine.content_id();
    assert_ne!(saved, start);
    rename(&mut engine, "B");
    assert_ne!(engine.content_id(), saved);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.content_id(), saved, "undo back to the saved content");
    engine.execute(Command::Redo).unwrap();
    assert_ne!(engine.content_id(), saved, "redo past it");
    engine.execute(Command::Undo).unwrap();
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.content_id(), start);
    // An edit that leaves the set equal changes nothing, so neither does the identity.
    rename(&mut engine, "Ramp");
    assert_eq!(engine.content_id(), start);
    // A different edit after an undo takes a fresh identity, never one from the redo stack.
    rename(&mut engine, "C");
    assert!(![start, saved].contains(&engine.content_id()));
}

#[test]
fn a_fold_changes_the_identity_and_an_undo_that_carries_it_takes_a_fresh_one() {
    let (mut engine, _receiver) = engine();
    engine
        .execute(Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        }))
        .unwrap();
    let group = engine
        .project()
        .tracks
        .iter()
        .find(|t| t.is_group())
        .unwrap()
        .id;
    let saved = engine.content_id();
    let fold = |engine: &mut Engine, folded| {
        engine
            .execute(Command::Edit(ProjectEdit::SetGroupFolded { group, folded }))
            .unwrap()
    };
    fold(&mut engine, true);
    assert_ne!(engine.content_id(), saved, "the saved file would differ");
    fold(&mut engine, false);
    assert_ne!(
        engine.content_id(),
        saved,
        "equal content, but not tracked back"
    );
    // save (id), rename, fold, undo the rename: the carried fold differs from the snapshot.
    let saved = engine.content_id();
    rename(&mut engine, "A");
    fold(&mut engine, true);
    engine.execute(Command::Undo).unwrap();
    assert_ne!(engine.content_id(), saved);
    assert!(engine.project().track(group).unwrap().fold_state);
    engine.execute(Command::Redo).unwrap();
    assert_eq!(clip_name(&engine), "A");
    assert!(engine.project().track(group).unwrap().fold_state);
}

#[test]
fn undo_keeps_a_fold_and_an_expand_made_after_the_step() {
    let (mut engine, _receiver) = engine();
    engine
        .execute(Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        }))
        .unwrap();
    let group = engine
        .project()
        .tracks
        .iter()
        .find(|t| t.is_group())
        .unwrap()
        .id;
    engine
        .execute(Command::Edit(ProjectEdit::AddDevice {
            track: TrackId(1),
            kind: NewDevice::Target,
            before: None,
        }))
        .unwrap();
    let device = engine.project().track(TrackId(1)).unwrap().devices[0].id;
    let was_expanded = engine.project().device(device).unwrap().1.expanded;
    engine
        .execute(Command::Edit(ProjectEdit::SetGroupFolded {
            group,
            folded: true,
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::SetDeviceExpanded {
            device,
            expanded: !was_expanded,
        }))
        .unwrap();
    rename(&mut engine, "elsewhere");
    engine.execute(Command::Undo).unwrap();
    assert_eq!(clip_name(&engine), "Ramp");
    assert!(engine.project().track(group).unwrap().fold_state);
    assert_eq!(
        engine.project().device(device).unwrap().1.expanded,
        !was_expanded
    );
    // Undoing the device's creation removes it; undoing the group's creation keeps nothing
    // for a group that no longer exists.
    engine.execute(Command::Undo).unwrap();
    assert!(engine.project().device(device).is_none());
    engine.execute(Command::Undo).unwrap();
    assert!(engine.project().tracks.iter().all(|t| !t.is_group()));
    engine.execute(Command::Redo).unwrap();
    assert!(
        engine.project().track(group).unwrap().fold_state,
        "redo keeps the current fold"
    );
}

// --- Playing while restoring ---

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn volume_state(engine: &Engine) -> Option<(f64, AutomationState)> {
    engine
        .snapshot()
        .automation
        .iter()
        .find(|v| v.field == volume_field())
        .map(|v| (v.value, v.state))
}
/// Every message received until `quiet` passes without one: (address, arguments).
fn drain(receiver: &TestReceiver, quiet: Duration) -> Vec<(String, Vec<OscType>)> {
    let mut seen = Vec::new();
    while let Ok(batch) = receiver.wait_for_messages(1, quiet) {
        let (_, message) = &batch[0];
        seen.push((message.addr.clone(), message.args.clone()));
    }
    seen
}
fn volumes(receiver: &TestReceiver, quiet: Duration) -> Vec<f32> {
    drain(receiver, quiet)
        .into_iter()
        .filter(|(address, _)| address == "/volume")
        .filter_map(|(_, args)| match args.as_slice() {
            [OscType::Float(v)] => Some(*v),
            _ => None,
        })
        .collect()
}
/// Every `/volume` value received over the next `span`, for a stream that never goes quiet.
fn volumes_for(receiver: &TestReceiver, span: Duration) -> Vec<f32> {
    let until = Instant::now() + span;
    let mut seen = Vec::new();
    while let Ok(batch) =
        receiver.wait_for_messages(1, until.saturating_duration_since(Instant::now()))
    {
        if batch[0].1.addr == "/volume"
            && let [OscType::Float(v)] = batch[0].1.args.as_slice()
        {
            seen.push(*v);
        }
    }
    seen
}
fn wire(value: f64) -> f32 {
    (10.0 + 10.0 * value) as f32
}

#[test]
fn undoing_a_value_under_an_overridden_envelope_publishes_and_sends_the_restored_value() {
    let (mut engine, receiver) = engine();
    let original = stored_volume(&engine);
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    wait_until("the envelope to drive Volume", || {
        volume_state(&engine).is_some_and(|(_, state)| state == AutomationState::Following)
    });
    set_volume(&mut engine, 0.5);
    set_volume(&mut engine, 0.25);
    wait_until("the override at 0.25", || {
        volume_state(&engine) == Some((0.25, AutomationState::Overridden))
    });
    let _ = volumes(&receiver, Duration::from_millis(150));
    engine.execute(Command::Undo).unwrap();
    assert_eq!(stored_volume(&engine), 0.5);
    wait_until("the override at 0.5", || {
        volume_state(&engine) == Some((0.5, AutomationState::Overridden))
    });
    assert_eq!(volumes(&receiver, Duration::from_millis(200)), [wire(0.5)]);
    // And back past the first override: the stored value the envelope was playing under.
    engine.execute(Command::Undo).unwrap();
    assert_eq!(stored_volume(&engine), original);
    wait_until("the override at the original value", || {
        volume_state(&engine) == Some((original, AutomationState::Overridden))
    });
    assert_eq!(
        volumes(&receiver, Duration::from_millis(200)),
        [wire(original)]
    );
    engine.execute(Command::Redo).unwrap();
    wait_until("the redo", || {
        volume_state(&engine) == Some((0.5, AutomationState::Overridden))
    });
    assert_eq!(volumes(&receiver, Duration::from_millis(200)), [wire(0.5)]);
}

#[test]
fn undoing_a_tempo_change_while_playing_retimes_the_clock() {
    let (mut engine, _receiver) = engine();
    engine.execute(Command::Play).unwrap();
    wait_until("the transport", || engine.snapshot().playing);
    engine.execute(Command::SetTempo(240.0)).unwrap();
    wait_until("240 bpm", || engine.snapshot().tempo_bpm == 240.0);
    engine.execute(Command::Undo).unwrap();
    wait_until("120 bpm", || engine.snapshot().tempo_bpm == 120.0);
    assert_eq!(engine.project().tempo.bpm(), 120.0);
    let beats = |engine: &Engine| engine.snapshot().current_beat.0 as f64 / TICKS_PER_BEAT as f64;
    let (start, at) = (beats(&engine), Instant::now());
    std::thread::sleep(Duration::from_millis(600));
    let advance = (beats(&engine) - start) / at.elapsed().as_secs_f64();
    assert!(
        (1.7..2.3).contains(&advance),
        "{advance} beats per second at 120 bpm"
    );
    engine.execute(Command::Redo).unwrap();
    wait_until("240 bpm again", || engine.snapshot().tempo_bpm == 240.0);
}

fn playing_slot(engine: &Engine, track: u64) -> Option<usize> {
    engine
        .snapshot()
        .tracks
        .iter()
        .find(|t| t.id == TrackId(track))
        .and_then(|t| t.playing_slot)
}

#[test]
fn undoing_the_deletion_of_a_playing_clip_brings_it_back_silent() {
    let (mut engine, receiver) = engine();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(2),
            slot: 0,
        })
        .unwrap();
    wait_until("the clip to play", || playing_slot(&engine, 2) == Some(0));
    receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::DeleteClip {
            track: TrackId(2),
            slot: 0,
        }))
        .unwrap();
    wait_until("the track to go silent", || {
        playing_slot(&engine, 2).is_none()
    });
    let _ = drain(&receiver, Duration::from_millis(300));
    engine.execute(Command::Undo).unwrap();
    assert!(engine.project().clip(TrackId(2), 0).is_some());
    // Three loops of the one-beat clip pass with nothing sent and nothing playing.
    assert!(drain(&receiver, Duration::from_millis(1500)).is_empty());
    assert_eq!(playing_slot(&engine, 2), None);
    assert!(engine.snapshot().playing, "the transport keeps running");
}

#[test]
fn undoing_the_creation_of_a_playing_clip_silences_the_track_and_keeps_the_transport() {
    let (mut engine, _receiver) = engine();
    engine
        .execute(Command::Edit(ProjectEdit::CreateClip {
            track: TrackId(2),
            slot: 1,
            length: BeatTime(960),
        }))
        .unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(2),
            slot: 1,
        })
        .unwrap();
    wait_until("the new clip to play", || {
        playing_slot(&engine, 2) == Some(1)
    });
    engine.execute(Command::Undo).unwrap();
    assert!(engine.project().clip(TrackId(2), 1).is_none());
    wait_until("the track to go silent", || {
        playing_slot(&engine, 2).is_none()
    });
    assert!(engine.snapshot().playing);
    // Redo brings the clip back without playing it.
    engine.execute(Command::Redo).unwrap();
    assert!(engine.project().clip(TrackId(2), 1).is_some());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(playing_slot(&engine, 2), None);
}

#[test]
fn undoing_a_rename_sends_no_parameter_packets_and_one_fader_move_sends_one_value() {
    let (mut engine, receiver) = engine();
    let original = stored_volume(&engine);
    set_volume(&mut engine, 0.3);
    rename(&mut engine, "Renamed");
    let _ = drain(&receiver, Duration::from_millis(300));
    engine.execute(Command::Undo).unwrap();
    assert_eq!(clip_name(&engine), "Ramp");
    assert_eq!(drain(&receiver, Duration::from_millis(400)), []);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(stored_volume(&engine), original);
    assert_eq!(
        volumes(&receiver, Duration::from_millis(400)),
        [wire(original)]
    );
    engine.execute(Command::Redo).unwrap();
    assert_eq!(volumes(&receiver, Duration::from_millis(400)), [wire(0.3)]);
}

#[test]
fn a_playing_clip_keeps_its_rhythm_through_an_undo_of_another_edit() {
    let (mut engine, receiver) = engine();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(2),
            slot: 0,
        })
        .unwrap();
    // Hits only: starting the transport sends every parameter value once as well.
    let hit = |receiver: &TestReceiver, wait: Duration| loop {
        match receiver.wait_for_messages(1, wait) {
            Ok(batch) if batch[0].1.addr == "/volume" => continue,
            Ok(batch) => break Some(batch[0].0),
            Err(_) => break None,
        }
    };
    let mut arrivals = vec![hit(&receiver, Duration::from_secs(2)).unwrap()];
    rename(&mut engine, "elsewhere");
    for _ in 0..2 {
        std::thread::sleep(Duration::from_millis(250));
        engine.execute(Command::Undo).unwrap();
        std::thread::sleep(Duration::from_millis(250));
        engine.execute(Command::Redo).unwrap();
    }
    while let Some(at) = hit(&receiver, Duration::from_millis(600)) {
        arrivals.push(at);
        if arrivals.len() >= 5 {
            break;
        }
    }
    assert!(arrivals.len() >= 4, "{arrivals:?}");
    // One hit per one-beat loop (0.5 s at 120 bpm): no repeat, burst or skipped beat.
    for pair in arrivals.windows(2) {
        let gap = pair[1].duration_since(pair[0]).as_secs_f64();
        assert!((0.4..0.6).contains(&gap), "gap {gap} in {arrivals:?}");
    }
}

// --- Fields whose definition changed ---

/// One track whose Target `xy` parameter has a float `x` over [0, 10] (stored 8), driven by a
/// looping 16-beat envelope; `OnRelease` sends the complete message on `/xy`.
fn target_project(port: u16) -> Project {
    let mut p = project(port);
    let track = &mut p.tracks[1];
    let device = track.first_target_mut_or_insert(|| DeviceId(1));
    let target = device.target_mut().unwrap();
    let mut x = FieldDef::new(FieldKey("x".into()), FieldType::Float, true);
    x.range = Some([0.0, 10.0]);
    x.value = Some(8.0);
    target.parameters.push(ParameterDef {
        key: EntryKey("xy".into()),
        name: String::new(),
        path: OscAddress::Relative("/xy".into()),
        fields: vec![x],
        send_mode: SendMode::OnRelease,
    });
    let clip = track.slots[0].clip.as_mut().unwrap();
    clip.end = BeatTime(16 * 960);
    clip.loop_end = clip.end;
    clip.lanes.push(Lane::Envelope(AutomationEnvelope {
        region: None,
        target: ArgumentRef {
            device: DeviceRef::Chain(DeviceId(1)),
            parameter: ParameterKey("xy".into()),
            field: FieldKey("x".into()),
        },
        points: vec![
            Breakpoint {
                time: BeatTime(0),
                value: 0.0,
                curve: 0.0,
            },
            Breakpoint {
                time: BeatTime(16 * 960),
                value: 10.0,
                curve: 0.0,
            },
        ],
    }));
    p
}

#[test]
fn undoing_a_definition_change_restores_an_overridden_value_the_change_had_clamped() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(target_project(receiver.addr().port())).unwrap();
    let field = FieldRef {
        parameter: ParameterRef {
            track: TrackId(2),
            device: DeviceRef::Chain(DeviceId(1)),
            key: ParameterKey("xy".into()),
        },
        field: FieldKey("x".into()),
    };
    let state = |engine: &Engine| {
        engine
            .snapshot()
            .automation
            .iter()
            .find(|v| v.field == field)
            .map(|v| (v.value, v.state))
    };
    let xs = |receiver: &TestReceiver| -> Vec<f32> {
        drain(receiver, Duration::from_millis(200))
            .into_iter()
            .filter(|(address, _)| address.ends_with("/xy"))
            .filter_map(|(_, args)| match args.first() {
                Some(OscType::Float(x)) => Some(*x),
                _ => None,
            })
            .collect()
    };
    engine
        .execute(Command::LaunchClip {
            track: TrackId(2),
            slot: 0,
        })
        .unwrap();
    wait_until("the envelope to drive x", || {
        state(&engine).is_some_and(|(_, s)| s == AutomationState::Following)
    });
    // Override at the stored 8, then narrow the range so the stored value clamps to 4.
    engine
        .execute(Command::Edit(ProjectEdit::SetParameter {
            target: field.parameter.clone(),
            values: vec![(field.field.clone(), 8.0)],
        }))
        .unwrap();
    wait_until("the override", || {
        state(&engine) == Some((8.0, AutomationState::Overridden))
    });
    engine
        .execute(Command::Edit(ProjectEdit::EditTarget {
            device: DeviceId(1),
            edit: qtable_engine::edit::TargetEdit::SetFieldRange {
                entry: EntryKey("xy".into()),
                field: FieldKey("x".into()),
                range: Some([0.0, 4.0]),
            },
        }))
        .unwrap();
    wait_until("the clamped override", || {
        state(&engine) == Some((4.0, AutomationState::Overridden))
    });
    let _ = xs(&receiver);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.project().field(&field).unwrap().value, 8.0);
    wait_until("the restored override", || {
        state(&engine) == Some((8.0, AutomationState::Overridden))
    });
    assert_eq!(xs(&receiver).last(), Some(&8.0));
}

#[test]
fn undoing_an_edit_while_an_envelope_plays_leaves_the_envelope_and_its_packets_alone() {
    let (mut engine, receiver) = engine();
    // The stored Volume sits high; the ramp starts at the bottom. A restore that resent the
    // stored value, or pushed the envelope aside, would show as a jump or an override.
    assert!(stored_volume(&engine) > 0.5);
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    wait_until("the envelope to drive Volume", || {
        volume_state(&engine).is_some_and(|(_, state)| state == AutomationState::Following)
    });
    rename(&mut engine, "Renamed");
    let mut seen = volumes_for(&receiver, Duration::from_millis(100));
    assert!(!seen.is_empty());
    engine.execute(Command::Undo).unwrap();
    assert_eq!(clip_name(&engine), "Ramp");
    seen.extend(volumes_for(&receiver, Duration::from_millis(400)));
    assert!(
        seen.windows(2).all(|pair| pair[0] <= pair[1]),
        "the ramp only climbs: {seen:?}"
    );
    assert!(seen.iter().all(|v| *v < wire(0.5)), "{seen:?}");
    assert_eq!(
        volume_state(&engine).map(|(_, state)| state),
        Some(AutomationState::Following)
    );
}
