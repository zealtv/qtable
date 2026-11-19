use super::*;

// The tests below that predate sensitivity call these two with no remainder to carry, as a
// mapping's first message does. They shadow the glob-imported functions of the same names;
// the sensitivity tests call `super::numeric` and `super::translate` with their own carry.
fn numeric(
    mapping: &RemoteMapping,
    descriptor: &ControlValue,
    gesture: Gesture,
    current: f64,
) -> Option<f64> {
    super::numeric(mapping, descriptor, gesture, current, &mut 0.0)
}

fn translate(
    project: &Project,
    mapping: &RemoteMapping,
    descriptor: &ControlValue,
    gesture: Gesture,
    current: Option<f64>,
    previous: Option<f64>,
) -> Option<Effect> {
    super::translate(
        project,
        mapping,
        descriptor,
        gesture,
        current,
        previous,
        Stepping {
            carry: &mut 0.0,
            scene: None,
        },
    )
}

fn binding(control: ControlRef, min: f64, max: f64) -> RemoteMapping {
    RemoteMapping::new(
        MappingId(1),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Cc(1),
        }),
        control,
        min,
        max,
    )
}

fn volume() -> ControlRef {
    ControlRef::Argument(FieldRef::volume(TrackId(1)))
}

fn project() -> Project {
    Project {
        tracks: vec![Track::new(TrackId(1), "Test", 1)],
        scenes: vec![Scene {
            id: SceneId(5),
            name: String::new(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    }
}

#[test]
fn ranges_invert_hold_and_relative_steps_clamp_before_reversal() {
    let project = project();
    let descriptor = describe_value(&project, &volume()).unwrap();
    for (min, max, fraction, expected) in [
        (0.0, 1.0, 0.5, 0.5),
        (1.0, 0.0, 0.25, 0.75),
        (0.4, 0.4, 1.0, 0.4),
    ] {
        let mapping = binding(volume(), min, max);
        assert_eq!(
            numeric(&mapping, &descriptor, Gesture::Absolute(fraction), 0.0),
            Some(expected)
        );
    }
    let mapping = binding(volume(), 0.0, 1.0);
    let upper = numeric(&mapping, &descriptor, Gesture::Steps(50), 0.95).unwrap();
    assert_eq!(upper, 1.0);
    assert_eq!(
        numeric(&mapping, &descriptor, Gesture::Steps(-50), upper),
        Some(1.0 - 50.0 / 256.0)
    );
    let lower = numeric(&mapping, &descriptor, Gesture::Steps(-50), 0.05).unwrap();
    assert_eq!(lower, 0.0);
    assert_eq!(
        numeric(&mapping, &descriptor, Gesture::Steps(50), lower),
        Some(50.0 / 256.0)
    );
}

#[test]
fn note_toggles_continuous_endpoints_and_cycles_enums() {
    let project = project();
    let mapping = binding(volume(), 0.2, 0.8);
    let descriptor = describe_value(&project, &mapping.control).unwrap();
    assert_eq!(
        numeric(&mapping, &descriptor, Gesture::Press, 0.2),
        Some(0.8)
    );
    assert_eq!(
        numeric(&mapping, &descriptor, Gesture::Press, 0.8),
        Some(0.2)
    );
    assert_eq!(numeric(&mapping, &descriptor, Gesture::Release, 0.2), None);
    let mapping = binding(
        ControlRef::Transport(TransportControl::Quantisation),
        0.0,
        13.0,
    );
    let descriptor = describe_value(&project, &mapping.control).unwrap();
    assert_eq!(
        numeric(&mapping, &descriptor, Gesture::Press, 13.0),
        Some(0.0)
    );
    for (fraction, expected) in [(0.0, 0.0), (0.1, 1.0), (0.5, 7.0), (1.0, 13.0)] {
        assert_eq!(
            numeric(&mapping, &descriptor, Gesture::Absolute(fraction), 0.0),
            Some(expected)
        );
    }
}

#[test]
fn triggers_use_edges_release_does_nothing_and_slots_use_scene_identity() {
    let project = project();
    let mapping = binding(
        ControlRef::ClipSlot {
            track: TrackId(1),
            scene: SceneId(5),
        },
        0.0,
        1.0,
    );
    let descriptor = describe_value(&project, &mapping.control).unwrap();
    assert!(matches!(
        translate(&project, &mapping, &descriptor, Gesture::Press, None, None),
        Some(Effect::Command(Command::LaunchClip { slot: 0, .. }, None))
    ));
    assert!(
        translate(
            &project,
            &mapping,
            &descriptor,
            Gesture::Release,
            None,
            None
        )
        .is_none()
    );
    assert!(
        translate(
            &project,
            &mapping,
            &descriptor,
            Gesture::Absolute(1.0),
            None,
            Some(1.0)
        )
        .is_none()
    );
    assert!(
        translate(
            &project,
            &mapping,
            &descriptor,
            Gesture::Absolute(1.0),
            None,
            Some(0.0)
        )
        .is_some()
    );
    let mut moved = project.clone();
    moved.scenes.insert(
        0,
        Scene {
            id: SceneId(4),
            ..moved.scenes[0].clone()
        },
    );
    assert!(matches!(
        translate(&moved, &mapping, &descriptor, Gesture::Press, None, None),
        Some(Effect::Command(Command::LaunchClip { slot: 1, .. }, None))
    ));
}

#[test]
fn switches_are_set_by_direction_and_activator_uses_window() {
    let project = project();
    let mapping = binding(ControlRef::TrackActivator(TrackId(1)), 64.0 / 127.0, 1.0);
    let descriptor = describe_value(&project, &mapping.control).unwrap();
    for (gesture, expected) in [
        (Gesture::Press, 0.0),
        (Gesture::Steps(-1), 0.0),
        (Gesture::Steps(1), 1.0),
        (Gesture::Absolute(0.0), 0.0),
        (Gesture::Absolute(1.0), 1.0),
    ] {
        assert_eq!(numeric(&mapping, &descriptor, gesture, 1.0), Some(expected));
    }
}

#[test]
fn view_triggers_translate_notes_keys_and_cc_crossings_without_engine_selection() {
    let project = project();
    for (control, action) in [
        (
            ControlRef::TrackSelect(TrackId(1)),
            ViewAction::SelectTrack(TrackId(1)),
        ),
        (
            ControlRef::SceneNavigation(SceneNavigation::Launch),
            ViewAction::LaunchScene,
        ),
        (
            ControlRef::SceneNavigation(SceneNavigation::Up),
            ViewAction::MoveScene(-1),
        ),
        (
            ControlRef::SceneNavigation(SceneNavigation::Down),
            ViewAction::MoveScene(1),
        ),
    ] {
        let mapping = binding(control, 0.0, 1.0);
        let descriptor = describe_value(&project, &mapping.control).unwrap();
        for gesture in [Gesture::Press, Gesture::Steps(1), Gesture::Absolute(1.0)] {
            assert!(
                matches!(translate(&project, &mapping, &descriptor, gesture, None, None),
                Some(Effect::View(actual)) if actual == action)
            );
        }
        for (gesture, previous) in [
            (Gesture::Release, None),
            (Gesture::Steps(-1), None),
            (Gesture::Absolute(0.1), None),
            (Gesture::Absolute(1.0), Some(1.0)),
        ] {
            assert!(translate(&project, &mapping, &descriptor, gesture, None, previous).is_none());
        }
    }
    let mapping = binding(
        ControlRef::SceneNavigation(SceneNavigation::CancelLaunch),
        0.0,
        1.0,
    );
    let descriptor = describe_value(&project, &mapping.control).unwrap();
    assert!(matches!(
        translate(&project, &mapping, &descriptor, Gesture::Press, None, None),
        Some(Effect::Command(Command::CancelSceneLaunch, None))
    ));
}

#[test]
fn scene_selector_spans_every_scene_whatever_range_is_stored() {
    let mut project = project();
    project.scenes.extend((6..=12).map(|id| Scene {
        id: SceneId(id),
        name: String::new(),
        follow_actions: Default::default(),
    }));
    let descriptor = describe_value(
        &project,
        &ControlRef::SceneNavigation(SceneNavigation::Select),
    )
    .unwrap();
    assert_eq!(descriptor.range, None);
    // Stored Min and Max (a leftover from format 10 files) change nothing.
    for (min, max) in [(0.0, 1.0), (1.0, 3.0), (8.0, 1.0)] {
        for (gesture, expected) in [
            // 8 scenes: equal bands across 0–127, the top endpoint selecting the last.
            (Gesture::Absolute(0.0), ViewAction::SelectScene(0)),
            (Gesture::Absolute(15.0 / 127.0), ViewAction::SelectScene(0)),
            (Gesture::Absolute(16.0 / 127.0), ViewAction::SelectScene(1)),
            (Gesture::Absolute(64.0 / 127.0), ViewAction::SelectScene(4)),
            (Gesture::Absolute(1.0), ViewAction::SelectScene(7)),
            (Gesture::Steps(-3), ViewAction::MoveScene(-3)),
            (Gesture::Steps(5), ViewAction::MoveScene(5)),
            (Gesture::Press, ViewAction::NextScene),
        ] {
            let mapping = binding(
                ControlRef::SceneNavigation(SceneNavigation::Select),
                min,
                max,
            );
            assert!(
                matches!(translate(&project, &mapping, &descriptor, gesture, None, None),
                Some(Effect::View(actual)) if actual == expected),
                "{gesture:?} with {min}..{max}"
            );
        }
    }
    // Deleting scenes needs no edit: the same input now spans fewer.
    project.scenes.truncate(4);
    let mapping = binding(
        ControlRef::SceneNavigation(SceneNavigation::Select),
        0.0,
        1.0,
    );
    assert!(matches!(
        translate(
            &project,
            &mapping,
            &descriptor,
            Gesture::Absolute(1.0),
            None,
            None
        ),
        Some(Effect::View(ViewAction::SelectScene(3)))
    ));
}

#[test]
fn nested_group_stop_and_slot_mappings_keep_their_stable_group_scope_when_folded() {
    let mut p = project();
    p.tracks[0].group = Some(TrackId(3));
    let mut inner = Track::new_group(TrackId(3), "Inner", 1);
    inner.group = Some(TrackId(2));
    p.tracks.insert(0, inner);
    p.tracks.insert(0, Track::new_group(TrackId(2), "Outer", 1));
    for folded in [false, true] {
        p.tracks[0].fold_state = folded;
        p.tracks[1].fold_state = folded;
        p.validate().unwrap();
        for group in [TrackId(2), TrackId(3)] {
            let control = ControlRef::TrackStop(group);
            let descriptor = describe_value(&p, &control).unwrap();
            assert!(
                matches!(translate(&p, &binding(control, 0.0, 1.0), &descriptor, Gesture::Press, None, None),
                Some(Effect::Command(Command::StopGroup {group: id}, None)) if id == group)
            );
            let control = ControlRef::ClipSlot {
                track: group,
                scene: SceneId(5),
            };
            let descriptor = describe_value(&p, &control).unwrap();
            assert!(
                matches!(translate(&p, &binding(control, 0.0, 1.0), &descriptor, Gesture::Press, None, None),
                Some(Effect::Command(Command::LaunchGroupSlot {group: id, slot:0}, None)) if id == group)
            );
        }
    }
}

#[test]
fn gate_levels_read_the_ends_and_a_trigger_ignores_a_stored_gate() {
    let project = project();
    let descriptor = describe_value(&project, &volume()).unwrap();
    let mut mapping = binding(volume(), 0.2, 0.8);
    mapping.mode = SourceMode::Gate;
    assert_eq!(
        numeric(&mapping, &descriptor, Gesture::Press, 0.5),
        Some(0.8)
    );
    assert_eq!(
        numeric(&mapping, &descriptor, Gesture::Release, 0.5),
        Some(0.2)
    );
    mapping.min = 0.8;
    mapping.max = 0.2;
    assert_eq!(
        numeric(&mapping, &descriptor, Gesture::Press, 0.5),
        Some(0.2)
    );
    assert_eq!(
        numeric(&mapping, &descriptor, Gesture::Release, 0.5),
        Some(0.8)
    );

    // Validation rejects Gate on a trigger, but a stored one must act as Latch: a note-on
    // fires and a note-off does not.
    let slot = ControlRef::Scene(SceneId(5));
    let descriptor = describe_value(&project, &slot).unwrap();
    let mut mapping = binding(slot, 0.0, 1.0);
    mapping.mode = SourceMode::Gate;
    let fire = |gesture| translate(&project, &mapping, &descriptor, gesture, None, None);
    assert!(matches!(
        fire(Gesture::Press),
        Some(Effect::Command(Command::LaunchScene { .. }, None))
    ));
    assert!(fire(Gesture::Release).is_none());
}
