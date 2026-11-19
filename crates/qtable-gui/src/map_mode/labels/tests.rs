use super::*;
use qtable_engine::model::{
    ACTIVATOR_WINDOW, ControlAccess, ControlKind, KeyName, KeySource, MidiSource, SceneId,
    SceneNavigation, TrackId, TransportControl,
};

fn cc(channel: u8, number: u8) -> Source {
    Source::Midi(MidiSource {
        channel,
        message: MidiMessage::Cc(number),
    })
}

fn descriptor(kind: ControlKind, range: Option<[f64; 2]>) -> ControlDescriptor {
    ControlDescriptor {
        label: "Control".into(),
        path: "Track / Mixer".into(),
        kind,
        range,
        choices: vec!["None".into(), "1 Bar".into()],
        access: ControlAccess::ReadWrite,
        value: None,
    }
}

#[test]
fn live_names_note_sixty_c3() {
    assert_eq!(note_name(60), "C3");
    assert_eq!(note_name(0), "C-2");
    assert_eq!(note_name(61), "C#3");
    assert_eq!(note_name(127), "G8");
}

#[test]
fn sources_read_as_live_writes_them() {
    assert_eq!(channel(&cc(3, 21)), "3");
    assert_eq!(control_cell(&cc(1, 21)), "CC 21");
    let note = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Note(60),
    });
    assert_eq!(control_cell(&note), "Note C3");
    assert_eq!(short(&note), "C3");
    assert_eq!(short(&cc(1, 21)), "CC21");
    let bend = Source::Midi(MidiSource {
        channel: 2,
        message: MidiMessage::PitchBend,
    });
    assert_eq!(control_cell(&bend), "Pitch Bend");
    let key = Source::Key(KeySource {
        key: KeyName("a".into()),
    });
    assert_eq!(
        (channel(&key).as_str(), control_cell(&key).as_str()),
        ("", "Key a")
    );
    // Typed characters keep their case and shifted symbols, so each reads as itself.
    for text in ["A", "1", "!", "/", "?", "~", "[", "|", "\\"] {
        let source = Source::Key(KeySource {
            key: KeyName(text.into()),
        });
        assert_eq!(control_cell(&source), format!("Key {text}"));
        assert_eq!(short(&source), text);
    }
}

#[test]
fn every_mode_has_its_own_label() {
    let source = cc(1, 21);
    let labels: std::collections::HashSet<_> =
        MODES.iter().map(|m| mode_label(&source, *m)).collect();
    assert_eq!(labels.len(), MODES.len());
    assert_eq!(
        mode_label(
            &source,
            SourceMode::Relative(RelativeEncoding::TwosComplement)
        ),
        "Relative (2's Comp.)"
    );
    assert_eq!(mode_label(&source, SourceMode::Absolute), "Absolute");
}

#[test]
fn a_note_or_key_offers_latch_and_gate_and_reads_absolute_as_latch() {
    let note = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Note(60),
    });
    let key = Source::Key(KeySource {
        key: KeyName("a".into()),
    });
    for source in [&note, &key] {
        assert_eq!(modes(source), [SourceMode::Absolute, SourceMode::Gate]);
        let names: Vec<_> = modes(source)
            .iter()
            .map(|mode| mode_label(source, *mode))
            .collect();
        assert_eq!(names, ["Latch", "Gate"]);
    }
    // A CC: Absolute and the eight relatives, no Gate.
    let cc_modes = modes(&cc(1, 21));
    assert_eq!(cc_modes.len(), 9);
    assert_eq!(cc_modes[0], SourceMode::Absolute);
    assert!(
        cc_modes[1..]
            .iter()
            .all(|m| matches!(m, SourceMode::Relative(_)))
    );
    let bend = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::PitchBend,
    });
    assert_eq!(modes(&bend), [SourceMode::Absolute]);
    // Every offered mode is one the model accepts for the source.
    for source in [&note, &key, &bend, &cc(1, 21)] {
        assert!(modes(source).iter().all(|mode| source.supports_mode(*mode)));
    }
}

/// The chooser offers exactly the modes the model accepts, for every kind of source: the
/// model's `supports_mode` decides, and the GUI's list lags behind nothing.
#[test]
fn the_offered_modes_are_the_ones_the_model_supports() {
    // An exhaustive match, so a new mode cannot join `SourceMode` without joining `MODES`.
    fn place(mode: SourceMode) -> usize {
        use RelativeEncoding::*;
        match mode {
            SourceMode::Absolute => 0,
            SourceMode::Gate => 1,
            SourceMode::Relative(SignedBit) => 2,
            SourceMode::Relative(SignedBit2) => 3,
            SourceMode::Relative(BinOffset) => 4,
            SourceMode::Relative(TwosComplement) => 5,
            SourceMode::Relative(SignedBitLinear) => 6,
            SourceMode::Relative(SignedBit2Linear) => 7,
            SourceMode::Relative(BinOffsetLinear) => 8,
            SourceMode::Relative(TwosComplementLinear) => 9,
        }
    }
    let places: Vec<usize> = MODES.iter().map(|mode| place(*mode)).collect();
    assert_eq!(places, (0..MODES.len()).collect::<Vec<_>>());
    let sources = [
        cc(1, 21),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Note(60),
        }),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::PitchBend,
        }),
        Source::Key(KeySource {
            key: KeyName("a".into()),
        }),
    ];
    for source in &sources {
        let accepted: Vec<SourceMode> = MODES
            .into_iter()
            .filter(|mode| source.supports_mode(*mode))
            .collect();
        assert_eq!(modes(source), accepted, "{source:?}");
        assert!(!accepted.is_empty());
    }
}

#[test]
fn takeovers_use_lives_names() {
    let names: Vec<_> = TAKEOVERS.iter().map(|t| takeover_label(*t)).collect();
    assert_eq!(names, ["None", "Pick-Up", "Value Scaling"]);
}

#[test]
fn the_inspector_writes_the_source_out() {
    assert_eq!(source_line(&cc(1, 22)), "MIDI Ch 1 · CC 22");
    let note = Source::Midi(MidiSource {
        channel: 3,
        message: MidiMessage::Note(62),
    });
    assert_eq!(source_line(&note), "MIDI Ch 3 · Note D3");
    let key = Source::Key(KeySource {
        key: KeyName("a".into()),
    });
    assert_eq!(source_line(&key), "Key a");
}

#[test]
fn sensitivity_reads_and_types_as_a_percentage_from_1_to_1000() {
    assert_eq!(format_sensitivity(1.0), "100 %");
    assert_eq!(format_sensitivity(2.5), "250 %");
    assert_eq!(format_sensitivity(0.01), "1 %");
    assert_eq!(parse_sensitivity("250"), Ok(2.5));
    assert_eq!(parse_sensitivity(" 50 % "), Ok(0.5));
    assert_eq!(parse_sensitivity("1000%"), Ok(10.0));
    assert_eq!(parse_sensitivity("1"), Ok(0.01));
    for refused in ["0", "0.5", "1001", "fast", "", "NaN", "inf"] {
        assert_eq!(
            parse_sensitivity(refused),
            Err("Enter a percentage between 1 and 1000".into()),
            "{refused:?}"
        );
    }
}

#[test]
fn paths_use_lives_separator() {
    assert_eq!(path(&descriptor(ControlKind::Float, None)), "Track | Mixer");
}

#[test]
fn scene_select_has_no_min_or_max_whatever_its_descriptor_says() {
    let select = ControlRef::SceneNavigation(SceneNavigation::Select);
    for range in [None, Some([1.0, 8.0])] {
        assert_eq!(
            limits(&select, &descriptor(ControlKind::Int, range)),
            Limits::None
        );
    }
}

#[test]
fn a_slot_row_has_no_limits_and_the_other_kinds_present_their_policy() {
    let track = TrackId(1);
    let trigger = descriptor(ControlKind::Trigger, None);
    let slot = ControlRef::ClipSlot {
        track,
        scene: SceneId(1),
    };
    assert_eq!(limits(&slot, &trigger), Limits::None);
    assert_eq!(
        limits(
            &ControlRef::TrackActivator(track),
            &descriptor(ControlKind::Toggle, None)
        ),
        Limits::Window
    );
    assert_eq!(
        limits(
            &ControlRef::TrackSolo(track),
            &descriptor(ControlKind::Toggle, None)
        ),
        Limits::Switch
    );
    let quantisation = ControlRef::Transport(TransportControl::Quantisation);
    assert_eq!(
        limits(
            &quantisation,
            &descriptor(ControlKind::Enum, Some([0.0, 1.0]))
        ),
        Limits::Choice(vec!["None".into(), "1 Bar".into()])
    );
}

#[test]
fn limits_format_and_parse_in_their_own_units() {
    let window = Limits::Window;
    assert_eq!(format_limit(&window, 1.0), "100%");
    assert_eq!(format_limit(&window, ACTIVATOR_WINDOW[0]), "50.4%");
    assert_eq!(parse_limit(&window, "50%"), Ok(0.5));
    assert_eq!(parse_limit(&window, " 25 "), Ok(0.25));
    assert!(parse_limit(&window, "120").is_err());

    let choices = Limits::Choice(vec!["None".into(), "1 Bar".into()]);
    assert_eq!(format_limit(&choices, 1.0), "1 Bar");
    assert!(parse_limit(&choices, "1").is_err());
    assert_eq!(format_limit(&Limits::Switch, 0.0), "Off");
    assert_eq!(format_limit(&Limits::Switch, 1.0), "On");

    let volume = Limits::Number {
        integer: false,
        range: Some([0.0, 1.0]),
    };
    assert_eq!(format_limit(&volume, 0.5), "0.50");
    assert_eq!(parse_limit(&volume, "0.25"), Ok(0.25));
    assert!(parse_limit(&volume, "2").unwrap_err().contains("between"));
    assert!(parse_limit(&volume, "nan").is_err());

    let steps = Limits::Number {
        integer: true,
        range: Some([0.0, 10.0]),
    };
    assert_eq!(format_limit(&steps, 4.0), "4");
    assert_eq!(parse_limit(&steps, "3.6"), Ok(4.0));
}

#[test]
fn learned_ranges_start_from_the_whole_control() {
    let track = TrackId(1);
    let volume = descriptor(ControlKind::Float, Some([0.0, 1.0]));
    assert_eq!(
        learn_range(&ControlRef::TrackSolo(track), &volume),
        (0.0, 1.0)
    );
    assert_eq!(
        learn_range(&ControlRef::TrackActivator(track), &volume),
        (ACTIVATOR_WINDOW[0], ACTIVATOR_WINDOW[1])
    );
    let tempo = descriptor(ControlKind::Float, None);
    assert_eq!(
        learn_range(&ControlRef::Transport(TransportControl::Tempo), &tempo),
        (20.0, 999.0)
    );
    assert_eq!(
        learn_range(
            &ControlRef::TrackStop(track),
            &descriptor(ControlKind::Trigger, None)
        ),
        (0.0, 1.0)
    );
}
