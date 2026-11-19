//! Every cell of the design's table (§3.4): which settings a mapping shows for its source and
//! control, and the Min and Max the control's kind gives it. Each cell reads
//! `[Mode, Takeover, Sensitivity, Invert, Min, Max]`, one letter each: H hidden, D dimmed,
//! E enabled.
use super::*;

const REL: SourceMode = SourceMode::Relative(RelativeEncoding::TwosComplement);

fn midi(message: MidiMessage) -> Source {
    Source::Midi(MidiSource {
        channel: 1,
        message,
    })
}

fn value(kind: ControlKind, range: Option<[f64; 2]>) -> ControlValue {
    ControlValue {
        kind,
        range,
        access: ControlAccess::ReadWrite,
        value: None,
    }
}

/// One control of each target class in the table, with the facts the model gives it.
fn targets() -> [(&'static str, ControlRef, ControlValue); 6] {
    let track = TrackId(1);
    [
        (
            "number",
            ControlRef::Argument(FieldRef::volume(track)),
            value(ControlKind::Float, Some([0.0, 1.0])),
        ),
        (
            "choice",
            ControlRef::Transport(TransportControl::Quantisation),
            value(ControlKind::Enum, Some([0.0, 5.0])),
        ),
        (
            "switch",
            ControlRef::TrackSolo(track),
            value(ControlKind::Toggle, Some([0.0, 1.0])),
        ),
        (
            "activator",
            ControlRef::TrackActivator(track),
            value(ControlKind::Toggle, Some([0.0, 1.0])),
        ),
        (
            "scene select",
            ControlRef::SceneNavigation(SceneNavigation::Select),
            value(ControlKind::Int, None),
        ),
        (
            "trigger",
            ControlRef::Transport(TransportControl::Play),
            value(ControlKind::Trigger, None),
        ),
    ]
}

fn cell(facets: &Facets) -> String {
    [
        facets.mode,
        facets.takeover,
        facets.sensitivity,
        facets.invert,
        facets.min,
        facets.max,
    ]
    .map(|facet| match facet {
        Facet::Hidden => 'H',
        Facet::Dimmed => 'D',
        Facet::Enabled => 'E',
    })
    .into_iter()
    .collect()
}

fn read(source: &Source, mode: SourceMode, target: usize) -> String {
    let (_, control, value) = &targets()[target];
    let mut mapping = RemoteMapping::new(MappingId(1), source.clone(), control.clone(), 0.0, 1.0);
    mapping.mode = mode;
    cell(&mapping.facets(control, value))
}

/// Target order: number, choice, switch, activator, scene select, trigger.
fn assert_column(source: Source, mode: SourceMode, expected: [&str; 6]) {
    for (index, expected) in expected.into_iter().enumerate() {
        let name = targets()[index].0;
        assert_eq!(
            read(&source, mode, index),
            expected,
            "{source:?} {mode:?} {name}"
        );
    }
}

#[test]
fn a_cc_in_absolute_mode_offers_takeover_not_sensitivity() {
    assert_column(
        midi(MidiMessage::Cc(1)),
        SourceMode::Absolute,
        ["EEDEEE", "EEDEEE", "EEHEEE", "EHHHEE", "EHDHHH", "EHHHHH"],
    );
}

#[test]
fn a_cc_in_a_relative_mode_offers_sensitivity_not_takeover() {
    assert_column(
        midi(MidiMessage::Cc(1)),
        REL,
        ["EDEEEE", "EDEEEE", "EDHDDD", "EHHHDD", "EHEHHH", "EHHHHH"],
    );
}

#[test]
fn pitch_bend_is_always_absolute_so_it_has_no_mode() {
    assert_column(
        midi(MidiMessage::PitchBend),
        SourceMode::Absolute,
        ["HEHEEE", "HEHEEE", "HEHEEE", "HHHHEE", "HHHHHH", "HHHHHH"],
    );
}

#[test]
fn a_note_or_key_latches_by_default() {
    for source in [
        midi(MidiMessage::Note(60)),
        Source::Key(KeySource {
            key: KeyName("a".into()),
        }),
    ] {
        assert_column(
            source,
            SourceMode::Absolute,
            ["EHHEEE", "EHHDEE", "EHHDHH", "EHHDHH", "HHHHHH", "HHHHHH"],
        );
    }
}

#[test]
fn a_note_or_key_in_gate_mode_lights_invert_for_choices_switches_and_the_activator() {
    for source in [
        midi(MidiMessage::Note(60)),
        Source::Key(KeySource {
            key: KeyName("a".into()),
        }),
    ] {
        assert_column(
            source,
            SourceMode::Gate,
            ["EHHEEE", "EHHEEE", "EHHEHH", "EHHEHH", "HHHHHH", "HHHHHH"],
        );
    }
}

#[test]
fn facets_agree_with_the_model_for_the_inputs_a_mapping_can_hold() {
    // A mode, whatever its value, is only offered where `supports_mode` allows it.
    for (_, control, value) in targets() {
        for source in [
            midi(MidiMessage::Cc(1)),
            midi(MidiMessage::PitchBend),
            midi(MidiMessage::Note(60)),
            Source::Key(KeySource {
                key: KeyName("a".into()),
            }),
        ] {
            let mapping =
                RemoteMapping::new(MappingId(1), source.clone(), control.clone(), 0.0, 1.0);
            let facets = mapping.facets(&control, &value);
            if !source.supports_mode(REL) && !source.supports_mode(SourceMode::Gate) {
                assert!(facets.mode.is_hidden(), "{source:?} {control:?}");
            }
            // Sensitivity belongs to relative CCs alone.
            if !source.supports_relative() {
                assert!(facets.sensitivity.is_hidden(), "{source:?} {control:?}");
            }
            // Takeover belongs to the sources `Source::uses_takeover` names.
            if !source.uses_takeover(SourceMode::Absolute) {
                assert!(facets.takeover.is_hidden(), "{source:?} {control:?}");
            }
        }
    }
}

#[test]
fn the_activator_never_offers_takeover() {
    // Its Min and Max are an input window, so no source takes over there (design §3.4).
    let (_, control, value) = &targets()[3];
    for source in [midi(MidiMessage::Cc(1)), midi(MidiMessage::PitchBend)] {
        let mapping = RemoteMapping::new(MappingId(1), source, control.clone(), 0.0, 1.0);
        assert!(mapping.facets(control, value).takeover.is_hidden());
    }
}

#[test]
fn limits_say_what_min_and_max_mean_for_each_kind_of_control() {
    let track = TrackId(1);
    let slot = ControlRef::ClipSlot {
        track,
        scene: SceneId(1),
    };
    let limits = |control: &ControlRef, value: ControlValue| control_limits(control, &value);
    assert_eq!(
        limits(&slot, value(ControlKind::Trigger, None)),
        Limits::None
    );
    assert_eq!(limits(&slot, value(ControlKind::Bang, None)), Limits::None);
    assert_eq!(limits(&slot, value(ControlKind::Text, None)), Limits::None);
    // Scene Select spans every scene, whatever its descriptor says.
    let select = ControlRef::SceneNavigation(SceneNavigation::Select);
    for range in [None, Some([1.0, 8.0])] {
        assert_eq!(
            limits(&select, value(ControlKind::Int, range)),
            Limits::None
        );
    }
    assert_eq!(
        limits(
            &ControlRef::TrackActivator(track),
            value(ControlKind::Toggle, None)
        ),
        Limits::Window
    );
    assert_eq!(
        limits(
            &ControlRef::TrackSolo(track),
            value(ControlKind::Toggle, None)
        ),
        Limits::Switch
    );
    assert_eq!(
        limits(
            &ControlRef::Transport(TransportControl::Quantisation),
            value(ControlKind::Enum, Some([0.0, 5.0]))
        ),
        Limits::Choice { count: 6 }
    );
    let argument = ControlRef::Argument(FieldRef::volume(track));
    assert_eq!(
        limits(&argument, value(ControlKind::Int, Some([0.0, 10.0]))),
        Limits::Number {
            integer: true,
            range: Some([0.0, 10.0])
        }
    );
    assert_eq!(
        limits(&argument, value(ControlKind::Float, None)),
        Limits::Number {
            integer: false,
            range: None
        }
    );
}

#[test]
fn a_learned_mapping_starts_at_the_whole_range_of_its_control() {
    let track = TrackId(1);
    let volume = value(ControlKind::Float, Some([0.0, 1.0]));
    assert_eq!(
        learn_range(&ControlRef::TrackSolo(track), &volume),
        (0.0, 1.0)
    );
    assert_eq!(
        learn_range(&ControlRef::TrackActivator(track), &volume),
        (ACTIVATOR_WINDOW[0], ACTIVATOR_WINDOW[1])
    );
    assert_eq!(
        learn_range(
            &ControlRef::Argument(FieldRef::volume(track)),
            &value(ControlKind::Int, Some([2.0, 9.0]))
        ),
        (2.0, 9.0)
    );
    // The rule for a control with no range keys on the control: Tempo learns Live's tempo
    // range, and an unranged argument 0-1 (Float) or 0-127 (Int), as agreed.
    assert_eq!(
        learn_range(
            &ControlRef::Transport(TransportControl::Tempo),
            &value(ControlKind::Float, None)
        ),
        (20.0, 999.0)
    );
    let event_argument = ControlRef::EventArgument {
        track,
        event: EventRef {
            device: DeviceId(1),
            key: EntryKey("hit".into()),
        },
        field: FieldKey("velocity".into()),
    };
    assert_eq!(
        learn_range(&event_argument, &value(ControlKind::Float, None)),
        (0.0, 1.0)
    );
    assert_eq!(
        learn_range(&event_argument, &value(ControlKind::Int, None)),
        (0.0, 127.0)
    );
    // Controls with no Min or Max keep 0-1, as before, Scene Select included.
    for control in [
        ControlRef::TrackStop(track),
        ControlRef::SceneNavigation(SceneNavigation::Select),
    ] {
        assert_eq!(
            learn_range(&control, &value(ControlKind::Trigger, None)),
            (0.0, 1.0)
        );
    }
    assert_eq!(
        learn_range(
            &ControlRef::SceneNavigation(SceneNavigation::Select),
            &value(ControlKind::Int, None)
        ),
        (0.0, 1.0)
    );
}
