//! Model and project validation regression tests.
use super::*;

fn clip(id: u64) -> Clip {
    Clip::new(
        ClipId(id),
        "cue",
        BeatTime::from_beats(4).unwrap(),
        crate::model::LIVE_CLIP_PALETTE[0],
    )
    .unwrap()
}

fn project() -> Project {
    Project {
        scenes: (0..4)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("scene {id}"),
                follow_actions: SceneFollowActions::default(),
            })
            .collect(),
        tracks: (0..3)
            .map(|id| Track::new(TrackId(id), "track", 4))
            .collect(),

        outputs: vec![
            Track::new_output(TrackId(201), "main", "localhost", 9000).with_path("/main"),
            Track::new_output(TrackId(202), "override", "localhost", 9000),
        ],
        main: Track::new_main(TrackId(203)),
        ..Project::default()
    }
}

fn group(scene_count: usize) -> Track {
    let mut group = Track::new_group(TrackId(101), "group", scene_count);
    group.route = Route::single(TrackId(201));
    group
}

#[test]
fn exact_time_and_meter() {
    assert_eq!(BeatTime::from_beats(2), Some(BeatTime(1920)));
    assert_eq!(BeatTime::from_beats(u64::MAX), None);
    assert_eq!(
        TimeSignature::new(7, 8).unwrap().bar_length(),
        BeatTime(3360)
    );
    assert!(TimeSignature::new(0, 4).is_err());
    assert!(TimeSignature::new(4, 0).is_err());
    assert!(TimeSignature::new(4, 3).is_err());
    assert!(TimeSignature::new(4, 32768).is_err());
    assert_eq!(NoteValue::Sixteenth.duration(), BeatTime(240));
    assert!(QuantisationValue::Bars(0).validate().is_err());
    assert!(FollowMultiplier::new(0, 1).is_err());
    assert!(FollowMultiplier::new(1, 0).is_err());
    let multiplier = FollowMultiplier::new(3, 2).unwrap();
    assert_eq!((multiplier.numerator(), multiplier.denominator()), (3, 2));
}

#[test]
fn tempo_and_delay_reject_nonfinite_values() {
    for bpm in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(Tempo::new(bpm).is_err());
    }
    assert_eq!(Tempo::new(73.5).unwrap().bpm(), 73.5);
    let mut p = project();
    p.tracks[0].delay_ms = -12.5;
    assert!(p.validate().is_ok());
    p.tracks[0].delay_ms = f64::NAN;
    assert!(p.validate().is_err());
}

#[test]
fn clearing_slots_preserves_gaps_rows_and_stop_buttons() {
    let mut p = project();
    p.tracks[0].slots[0].clip = Some(clip(1));
    p.tracks[0].slots[2].clip = Some(clip(2));
    p.tracks[0].slots[2].has_stop_button = false;
    assert!(p.validate().is_ok());
    assert_eq!(p.tracks[0].slots[2].clear().unwrap().id, ClipId(2));
    assert_eq!(p.tracks[0].slots.len(), 4);
    assert_eq!(p.tracks[0].slots[0].clip.as_ref().unwrap().id, ClipId(1));
    assert!(p.tracks[0].slots[1].clip.is_none());
    assert!(!p.tracks[0].slots[2].has_stop_button);
    assert!(p.tracks[0].slots[3].has_stop_button);
    assert!(p.validate().is_ok());
    p.tracks[0].slots.remove(1);
    assert!(p.validate().is_err());
}

#[test]
fn group_tracks_precede_all_their_members_and_nothing_else_joins() {
    let mut p = project();
    p.insert_group(group(p.scenes.len()), &[TrackId(0), TrackId(1)]);
    assert!(p.validate().is_ok());
    let broken: [fn(&mut Project); 6] = [
        // No members left.
        |p| p[TrackId(0)].group = None,
        |p| {
            p[TrackId(0)].group = None;
            p[TrackId(1)].group = None;
        },
        // A member split from the group track by an outsider.
        |p| {
            p[TrackId(1)].group = None;
            p[TrackId(2)].group = Some(TrackId(101));
        },
        // The group track after its members.
        |p| p.tracks.swap(0, 1),
        // A group naming a track that does not exist, or one that is no group track.
        |p| p[TrackId(0)].group = Some(TrackId(99)),
        |p| p[TrackId(2)].group = Some(TrackId(0)),
    ];
    for (i, break_it) in broken.into_iter().enumerate() {
        let mut p = p.clone();
        break_it(&mut p);
        assert!(p.validate().is_err(), "case {i} should be invalid");
    }
}

#[test]
fn group_tracks_do_not_nest_hold_clips_or_inherit_a_route() {
    let mut p = project();
    p.insert_group(group(p.scenes.len()), &[TrackId(0), TrackId(1)]);
    let mut nested = p.clone();
    nested.insert_group(
        Track::new_group(TrackId(102), "outer", nested.scenes.len()),
        &[TrackId(2)],
    );
    nested[TrackId(101)].group = Some(TrackId(102));
    assert!(nested.validate().is_err());
    let mut inherits = p.clone();
    inherits[TrackId(101)].route = Route::Group;
    assert!(inherits.validate().is_err());
    let mut folded = p.clone();
    folded[TrackId(2)].fold_state = true;
    assert!(folded.validate().is_err());
}

#[test]
fn routed_outputs_prefer_track_then_group() {
    let mut p = project();
    p.insert_group(group(p.scenes.len()), &[TrackId(0), TrackId(1)]);
    p[TrackId(0)].route = Route::Group;
    p[TrackId(1)].route = Route::Group;
    assert_eq!(
        p.routed_outputs(TrackId(0)).next().unwrap().id,
        TrackId(201)
    );
    p[TrackId(0)].route = Route::single(TrackId(202));
    assert_eq!(
        p.routed_outputs(TrackId(0)).next().unwrap().id,
        TrackId(202)
    );
    assert_eq!(
        p.routed_outputs(TrackId(1)).next().unwrap().id,
        TrackId(201)
    );
    assert!(p.routed_outputs(TrackId(2)).next().is_none());
    assert!(p.routed_outputs(TrackId(99)).next().is_none());
    p[TrackId(101)].route = Route::empty();
    assert!(p.routed_outputs(TrackId(1)).next().is_none());
}

#[test]
fn dummy_clips_and_timeline_boundaries() {
    let mut c = clip(1);
    assert!(c.validate().is_ok());
    assert!(
        Clip::new(
            ClipId(1),
            "zero",
            BeatTime::ZERO,
            crate::model::LIVE_CLIP_PALETTE[0]
        )
        .is_err()
    );
    let mut track = Track::new(TrackId(1), "t", 1);
    let arguments = vec![
        OscArgument::Int(7),
        OscArgument::Float(1.5),
        OscArgument::String("0".into()),
        OscArgument::Bool(true),
    ];
    let play = OscAddress::Relative("/play".into());
    c.events
        .push(track.hit(BeatTime::ZERO, play, arguments, || DeviceId(1)));
    let stop = OscAddress::Absolute("/other/stop".into());
    let end = BeatTime(c.length().ticks() - 1);
    c.events.push(track.hit(end, stop, vec![], || DeviceId(1)));
    assert!(c.validate().is_ok());
    c.events[1].position = c.length();
    assert!(c.validate().is_ok()); // content outside markers is retained
    c.events[1].position = BeatTime::ZERO;
    assert!(c.validate().is_ok()); // equal-time events remain distinct
    assert_eq!(c.events.len(), 2);
    c.events[0]
        .arguments
        .push(OscArgument::String("bad\0string".into()));
    assert!(c.validate().is_err());
}

#[test]
fn references_and_ids_are_checked_even_in_disabled_follow_actions() {
    let mut p = project();
    p.tracks[0].slots[0].clip = Some(clip(1));
    p.tracks[0].slots[2].clip = Some(clip(1));
    assert!(p.validate().is_err());
    p.tracks[0].slots[2].clear();
    p.tracks[0].route = Route::single(TrackId(299));
    assert!(p.validate().is_err());
    p.tracks[0].route = Route::Group;
    p.outputs[0].destination_mut().unwrap().port = 0;
    assert!(p.validate().is_err());
    p.outputs[0].destination_mut().unwrap().port = 9000;
    p.scenes[0].follow_actions.actions.action_a = FollowAction::Jump(SceneId(99));
    assert!(p.validate().is_err());
    p.scenes[0].follow_actions.actions.action_a = FollowAction::Jump(SceneId(3));
    assert!(p.validate().is_ok());
    let c = p.tracks[0].slots[0].clip.as_mut().unwrap();
    c.follow_actions.actions.action_b = FollowAction::Jump(SceneId(99));
    assert!(p.validate().is_err());
    p.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .follow_actions
        .actions
        .action_b = FollowAction::Jump(SceneId(2));
    assert!(p.validate().is_ok());
    p.scenes.swap(0, 3); // stable Jump references survive row reordering
    assert!(p.validate().is_ok());
    p.scenes[0].id = p.scenes[1].id;
    assert!(p.validate().is_err());
}

#[test]
fn full_follow_settings_and_quantisation_are_retained() {
    let mut p = project();
    let mut c = clip(1);
    c.quantisation = LaunchQuantisation::Value(QuantisationValue::Triplet(NoteValue::Eighth));
    c.follow_actions = ClipFollowActions {
        actions: FollowActions {
            enabled: true,
            action_a: FollowAction::Next,
            action_b: FollowAction::Jump(SceneId(3)),
            chance_a: 7,
            chance_b: 3,
        },
        linked: false,
        multiplier: FollowMultiplier::new(3, 2).unwrap(),
        time: BeatTime(1234),
    };
    c.follow_actions.linked = true;
    assert_eq!(c.follow_actions.time, BeatTime(1234));
    assert_eq!(c.follow_actions.multiplier.numerator(), 3);
    p.tracks[0].slots[0].clip = Some(c);
    p.scenes[0].follow_actions = SceneFollowActions {
        actions: FollowActions {
            enabled: true,
            action_a: FollowAction::Any,
            action_b: FollowAction::Stop,
            chance_a: 2,
            chance_b: 1,
        },
        time: BeatTime(6000),
    };
    p.follow_actions_enabled = false;
    assert!(p.validate().is_ok());
    p.global_quantisation = GlobalQuantisation::Value(QuantisationValue::Bars(0));
    assert!(p.validate().is_err());
    p.global_quantisation = GlobalQuantisation::None;
    p.tracks[0].slots[0].clip.as_mut().unwrap().quantisation =
        LaunchQuantisation::Value(QuantisationValue::Bars(0));
    assert!(p.validate().is_err());
}

#[test]
fn destinations_and_paths_are_receiver_independent() {
    let mut p = project();
    p.outputs[0].destination_mut().unwrap().mode = UdpMode::Broadcast;
    p.outputs[0].destination_mut().unwrap().host = "255.255.255.255".into();
    p.outputs[0].path = Path::new("/Custom_Receiver/p").unwrap();
    assert!(p.validate().is_ok());
    assert!(Path::new("/Custom_Receiver/p/").is_err());
    p.outputs[0].path = Path::default();
    assert!(p.validate().is_ok());
    p.outputs[0].destination_mut().unwrap().port = 0;
    assert!(p.validate().is_err());
    p.outputs[0].destination_mut().unwrap().port = 9000;
    p.outputs[0].destination_mut().unwrap().host.clear();
    assert!(p.validate().is_err());
}

#[test]
fn deserialization_enforces_value_constructors_and_preserves_wire_format() {
    for invalid in ["(0.0)", "(-1.0)", "(inf)", "(NaN)"] {
        assert!(ron::from_str::<Tempo>(invalid).is_err(), "{invalid}");
    }
    for invalid in ["(numerator:0,denominator:4)", "(numerator:4,denominator:3)"] {
        assert!(ron::from_str::<TimeSignature>(invalid).is_err());
    }
    for invalid in ["(numerator:0,denominator:1)", "(numerator:1,denominator:0)"] {
        assert!(ron::from_str::<FollowMultiplier>(invalid).is_err());
    }
    assert_eq!(ron::from_str::<Tempo>("Tempo(123.5)").unwrap().bpm(), 123.5);
    assert!(ron::from_str::<Tempo>("Tempo(0.0)").is_err());
    assert!(ron::from_str::<TimeSignature>("TimeSignature(numerator:4,denominator:3)").is_err());
    assert!(
        ron::from_str::<FollowMultiplier>("FollowMultiplier(numerator:1,denominator:0)").is_err()
    );
    let tempo = Tempo::new(123.5).unwrap();
    let signature = TimeSignature::new(7, 8).unwrap();
    let multiplier = FollowMultiplier::new(3, 2).unwrap();
    assert_eq!(
        ron::from_str::<Tempo>(&ron::to_string(&tempo).unwrap()).unwrap(),
        tempo
    );
    assert_eq!(
        ron::from_str::<TimeSignature>(&ron::to_string(&signature).unwrap()).unwrap(),
        signature
    );
    assert_eq!(
        ron::from_str::<FollowMultiplier>(&ron::to_string(&multiplier).unwrap()).unwrap(),
        multiplier
    );
}

#[test]
fn legato_defaults_off_and_round_trips_without_cluttering_old_files() {
    let mut clip = Clip::new(ClipId(1), "c", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap();
    assert!(!clip.legato);
    assert_eq!(clip.launch_mode, LaunchMode::Trigger);
    let plain = ron::to_string(&clip).unwrap();
    assert!(
        !plain.contains("legato") && !plain.contains("launch_mode"),
        "{plain}"
    );
    assert_eq!(ron::from_str::<Clip>(&plain).unwrap(), clip);
    clip.legato = true;
    let text = ron::to_string(&clip).unwrap();
    assert!(text.contains("legato"), "{text}");
    assert!(ron::from_str::<Clip>(&text).unwrap().legato);
}
