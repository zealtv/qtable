use super::tests::{event_argument, project};
use super::*;

#[test]
fn stored_mapping_types_reject_unknown_fields_and_variants() {
    let good = "(id:(1),source:Midi((channel:1,message:Cc(21))),control:Transport(Play),min:0.0,max:1.0,mode:Absolute,takeover:PickUp)";
    assert!(ron::from_str::<RemoteMapping>(good).is_ok());
    for bad in [
        good.replace("id:", "future:true,id:"),
        good.replace("channel:", "future:true,channel:"),
        good.replace("Transport(Play)", "ClipSlot(track:(1),scene:(1),future:true)"),
        good.replace("source:Midi((channel:1,message:Cc(21)))", "source:Key((key:(\"a\"),future:true))"),
        good.replace("takeover:PickUp", "takeover:Future"),
        good.replace("mode:Absolute", "mode:Future"),
        good.replace("takeover:PickUp", "takeover:PickUp,future:true"),
        good.replace("takeover:PickUp", "takeover:PickUp,sensitivity:2.5,sensitivity:3.0"),
        good.replace("message:Cc(21)", "message:Future"),
        good.replace("source:Midi((channel:1,message:Cc(21)))", "source:Osc(\"/value\")"),
        good.replace("Transport(Play)", "Argument((parameter:(track:(1),device:Mixer,key:\"volume\",future:true),field:(\"value\")))"),
        good.replace("Transport(Play)", "Argument((parameter:(track:(1),device:Mixer,key:\"volume\"),field:(\"value\"),future:true))"),
    ] {
        assert!(ron::from_str::<RemoteMapping>(&bad).is_err(), "{bad}");
    }
    let key = "Key((key:(\"A\")))";
    assert!(ron::from_str::<Source>(key).is_ok());
    assert!(ron::from_str::<Source>(&key.replace("key:", "future:true,key:")).is_err());
    for mode in [
        "Relative(SignedBit)",
        "Relative(SignedBitLinear)",
        "Relative(TwosComplement)",
    ] {
        assert!(ron::from_str::<SourceMode>(mode).is_ok());
    }
    assert!(ron::from_str::<SourceMode>("Relative(Future)").is_err());
    assert!(ron::from_str::<Project>("(future:true)").is_err());
}

#[test]
fn takeover_applies_only_to_absolute_continuous_midi_sources() {
    for message in [
        MidiMessage::Cc(0),
        MidiMessage::PitchBend,
        MidiMessage::Note(60),
    ] {
        let source = Source::Midi(MidiSource {
            channel: 1,
            message,
        });
        assert_eq!(
            source.uses_takeover(SourceMode::Absolute),
            message != MidiMessage::Note(60)
        );
        assert!(!source.uses_takeover(SourceMode::Relative(RelativeEncoding::TwosComplement)));
    }
    assert!(
        !Source::Key(KeySource {
            key: KeyName("a".into())
        })
        .uses_takeover(SourceMode::Absolute)
    );
}

#[test]
fn key_sources_are_exactly_the_typed_characters_live_maps() {
    let key = |text: &str| {
        Source::Key(KeySource {
            key: KeyName(text.into()),
        })
    };
    // Letters in both cases, digits, shifted digit symbols and punctuation, case kept.
    let letters = ('a'..='z').chain('A'..='Z').map(String::from);
    let digits = ('0'..='9').map(String::from);
    let symbols = "!@#$%^&*()_+-=.,<>/?~`[]{}|\\;:'\""
        .chars()
        .map(String::from);
    for text in letters
        .chain(digits)
        .chain(symbols)
        .chain(["£".into(), "é".into()])
    {
        assert!(key(&text).validate().is_ok(), "{text:?}");
        assert_eq!(KeyName::from_typed(&text), Some(KeyName(text.clone())));
    }
    assert_ne!(KeyName("a".into()), KeyName("A".into()));
    assert_ne!(KeyName("1".into()), KeyName("!".into()));
    // Named keys, whitespace, controls, platform function-key code points and strings.
    for text in [
        "", " ", "\t", "\n", "\r", "\u{7f}", "\u{1b}", "\u{f700}", "ab", "a ", "Space", "Enter",
        "Escape", "ArrowUp", "Delete", "Shift", "F1", "x\0",
    ] {
        assert!(key(text).validate().is_err(), "{text:?}");
        assert_eq!(KeyName::from_typed(text), None, "{text:?}");
    }
}

#[test]
fn new_mappings_default_to_absolute_with_no_takeover_for_every_source() {
    for source in [
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Cc(1),
        }),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::PitchBend,
        }),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Note(60),
        }),
        Source::Key(KeySource {
            key: KeyName("a".into()),
        }),
    ] {
        let mapping = RemoteMapping::new(
            MappingId(1),
            source,
            ControlRef::Transport(TransportControl::Play),
            0.0,
            1.0,
        );
        assert_eq!(mapping.mode, SourceMode::Absolute);
        assert_eq!(mapping.takeover, Takeover::None);
    }
}

#[test]
fn format_12_spellings_are_accepted_and_older_rows_read_as_before() {
    // Point 5 of the golden plan: these two were rejected before format 12.
    let good = "(id:(1),source:Midi((channel:1,message:Note(60))),control:Transport(Play),min:0.0,max:1.0,mode:Absolute,takeover:PickUp)";
    let gate = good.replace("mode:Absolute", "mode:Gate");
    assert_eq!(
        ron::from_str::<RemoteMapping>(&gate).unwrap().mode,
        SourceMode::Gate
    );
    let scaling = good.replace("takeover:PickUp", "takeover:ValueScaling");
    assert_eq!(
        ron::from_str::<RemoteMapping>(&scaling).unwrap().takeover,
        Takeover::ValueScaling
    );
    // A format-11 row has no sensitivity and reads 1.0; an explicit one is read as written.
    assert_eq!(
        ron::from_str::<RemoteMapping>(good).unwrap().sensitivity,
        1.0
    );
    let sensitive = good.replace("takeover:PickUp", "takeover:PickUp,sensitivity:2.5");
    assert_eq!(
        ron::from_str::<RemoteMapping>(&sensitive)
            .unwrap()
            .sensitivity,
        2.5
    );
}

#[test]
fn sensitivity_is_written_only_when_it_is_not_one_and_after_takeover() {
    let mut row = RemoteMapping::new(
        MappingId(1),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Cc(7),
        }),
        ControlRef::Transport(TransportControl::Tempo),
        20.0,
        999.0,
    );
    assert_eq!(row.sensitivity, DEFAULT_SENSITIVITY);
    let plain = ron::to_string(&row).unwrap();
    assert!(!plain.contains("sensitivity"), "{plain}");
    row.sensitivity = 2.5;
    let written = ron::to_string(&row).unwrap();
    assert!(
        written.ends_with("takeover:r#None,sensitivity:2.5)"),
        "{written}"
    );
    assert_eq!(ron::from_str::<RemoteMapping>(&written).unwrap(), row);
}

fn mapping(source: Source, control: ControlRef) -> RemoteMapping {
    RemoteMapping::new(MappingId(1), source, control, 0.0, 1.0)
}

fn midi(message: MidiMessage) -> Source {
    Source::Midi(MidiSource {
        channel: 1,
        message,
    })
}

fn key() -> Source {
    Source::Key(KeySource {
        key: KeyName("a".into()),
    })
}

fn volume() -> ControlRef {
    ControlRef::Argument(FieldRef::volume(TrackId(2)))
}

#[test]
fn sensitivity_must_be_finite_and_between_one_and_one_thousand_percent() {
    let project = project();
    let valid = |sensitivity: f64| {
        let mut row = mapping(midi(MidiMessage::Cc(1)), volume());
        row.sensitivity = sensitivity;
        row.validate(&project).is_ok()
    };
    for ok in [0.01, 0.25, 1.0, 2.5, 10.0] {
        assert!(valid(ok), "{ok}");
    }
    for bad in [
        0.0,
        0.009,
        10.001,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert!(!valid(bad), "{bad}");
    }
    // Any source keeps a stored value, active or not: only the range is checked.
    let mut note = mapping(key(), volume());
    note.sensitivity = 4.0;
    assert!(note.validate(&project).is_ok());
}

#[test]
fn gate_needs_a_note_or_key_and_any_control_takes_it() {
    let project = project();
    let gated = |source: Source, control: ControlRef| {
        let mut row = mapping(source, control);
        row.mode = SourceMode::Gate;
        row.validate(&project)
    };
    let note = || midi(MidiMessage::Note(60));
    // Numbers, choices, switches and the Activator hold a value.
    for control in [
        volume(),
        ControlRef::Transport(TransportControl::Tempo),
        ControlRef::Transport(TransportControl::Quantisation),
        ControlRef::TrackSolo(TrackId(2)),
        ControlRef::TrackActivator(TrackId(2)),
        ControlRef::Transport(TransportControl::FollowActions),
        super::tests::field("cutoff", "f1"),
    ] {
        assert!(gated(note(), control.clone()).is_ok(), "{control:?}");
        assert!(gated(key(), control.clone()).is_ok(), "{control:?}");
    }
    // Triggers and Scene Select hold none: the inspector offers no Gate there, but a stored one
    // is valid (it acts as Latch), so retyping an argument or loading a file never fails on it.
    for control in [
        ControlRef::Transport(TransportControl::Play),
        ControlRef::TrackStop(TrackId(2)),
        ControlRef::Scene(SceneId(10)),
        ControlRef::SceneNavigation(SceneNavigation::Select),
        ControlRef::SceneNavigation(SceneNavigation::Up),
        ControlRef::ClipSlot {
            track: TrackId(2),
            scene: SceneId(11),
        },
        super::tests::event(),
    ] {
        assert!(gated(note(), control.clone()).is_ok(), "{control:?}");
        assert!(gated(key(), control.clone()).is_ok(), "{control:?}");
    }
    // A CC and pitch bend cannot Gate, whatever the control.
    for source in [midi(MidiMessage::Cc(1)), midi(MidiMessage::PitchBend)] {
        assert!(gated(source, volume()).is_err());
    }
    // Relative still needs a CC; Absolute and Value Scaling are accepted anywhere.
    let mut relative = mapping(note(), volume());
    relative.mode = SourceMode::Relative(RelativeEncoding::SignedBit);
    assert!(relative.validate(&project).is_err());
    relative.source = midi(MidiMessage::Cc(1));
    assert!(relative.validate(&project).is_ok());
    let mut scaling = mapping(note(), volume());
    scaling.takeover = Takeover::ValueScaling;
    assert!(scaling.validate(&project).is_ok());
}

#[test]
fn modes_belong_to_their_sources() {
    let relative = SourceMode::Relative(RelativeEncoding::TwosComplement);
    for (source, absolute, rel, gate) in [
        (midi(MidiMessage::Cc(1)), true, true, false),
        (midi(MidiMessage::PitchBend), true, false, false),
        (midi(MidiMessage::Note(60)), true, false, true),
        (key(), true, false, true),
    ] {
        assert_eq!(
            source.supports_mode(SourceMode::Absolute),
            absolute,
            "{source:?}"
        );
        assert_eq!(source.supports_mode(relative), rel, "{source:?}");
        assert_eq!(source.supports_mode(SourceMode::Gate), gate, "{source:?}");
    }
}

#[test]
fn an_event_argument_mapping_validates_and_a_deleted_one_does_not() {
    // The row, its descriptor and the address share one identity (details in `tests`).
    let row = mapping(midi(MidiMessage::Cc(1)), event_argument("gone"));
    assert!(row.validate(&project()).is_err());
}
