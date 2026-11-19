//! A controller sets an event argument's value and sends nothing; only the Bang sends OSC
//! (design 4.4, Option A), checked against a real UDP receiver.
use qtable_engine::engine::Engine;
use qtable_engine::model::*;
use qtable_remote::{Consumer, Endpoint, Envelope, Input, MappingResolver};
use qtable_test_support::TestReceiver;
use rosc::OscType;
use std::time::{Duration, Instant};

fn project(receiver: &TestReceiver) -> (Project, EventRef) {
    let mut track = Track::new(TrackId(1), "Track", 0);
    track.route = Route::single(TrackId(2));
    track.path = Path::new("/track").unwrap();
    let hit = track.hit(
        BeatTime::ZERO,
        OscAddress::Relative("/cue".into()),
        vec![OscArgument::Float(0.25), OscArgument::Int(7)],
        || DeviceId(1),
    );
    let definition = track.event_def_mut(&hit.definition).unwrap();
    for (field, value) in definition.fields.iter_mut().zip(hit.arguments) {
        field.default = value;
        field.value = None;
        field.seed_current();
    }
    definition.fields[0].control = Control::Slider;
    definition.fields[0].range = Some([0.0, 1.0]);
    track.devices[0].target_mut().unwrap().path = Path::new("/target").unwrap();
    let mut output = Track::new_output(TrackId(2), "Out", "127.0.0.1", receiver.addr().port());
    output.path = Path::new("/out").unwrap();
    let event = hit.definition;
    let control = |field: &str| ControlRef::EventArgument {
        track: TrackId(1),
        event: event.clone(),
        field: FieldKey(field.into()),
    };
    let cc = |number| {
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Cc(number),
        })
    };
    let mapping = |id, source, control, max| {
        let mut mapping = RemoteMapping::new(MappingId(id), source, control, 0.0, max);
        mapping.takeover = Takeover::None;
        mapping
    };
    let fields = &track.event_def(&event).unwrap().1.fields;
    let (first, second) = (fields[0].key.0.clone(), fields[1].key.0.clone());
    let mappings = vec![
        mapping(1, cc(1), control(&first), 1.0),
        mapping(2, cc(2), control(&second), 127.0),
        mapping(
            3,
            cc(3),
            ControlRef::Event {
                track: TrackId(1),
                event: event.clone(),
            },
            1.0,
        ),
    ];
    (
        Project {
            tracks: vec![track],
            outputs: vec![output],
            mappings,
            ..Project::default()
        },
        event,
    )
}

fn drain(engine: &mut Engine, resolver: &mut MappingResolver, messages: &[[u8; 3]]) {
    let events: Vec<_> = messages
        .iter()
        .enumerate()
        .map(|(sequence, bytes)| Envelope {
            endpoint: Endpoint::Midi {
                id: "port".into(),
                name: "Port".into(),
            },
            received_at: Instant::now(),
            sequence: sequence as u64 + 1,
            input: Input::Midi(bytes.as_slice().into()),
        })
        .collect();
    let mut commands = Vec::new();
    resolver.receive(&events, engine.project(), &engine.snapshot(), &mut commands);
    for command in commands {
        engine.execute(command.value).unwrap();
    }
}

#[test]
fn turning_a_mapped_argument_sends_nothing_and_the_bang_sends_the_new_values() {
    let receiver = TestReceiver::bind().unwrap();
    let (project, event) = project(&receiver);
    let mut engine = Engine::new(project).unwrap();
    let mut resolver = MappingResolver::default();
    let revision = engine.project_revision();

    drain(&mut engine, &mut resolver, &[[0xb0, 1, 127], [0xb0, 2, 42]]);
    assert!(
        engine.project_revision() > revision,
        "the face value changed"
    );
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(150))
            .is_err(),
        "a mapped argument sent OSC"
    );
    let (_, definition) = engine.project().tracks[0].event_def(&event).unwrap();
    let current: Vec<_> = definition
        .fields
        .iter()
        .map(FieldDef::current_argument)
        .collect();
    assert_eq!(current, [OscArgument::Float(1.0), OscArgument::Int(42)]);

    // Knob and Bang in one drain: the Bang sends the value set just before it.
    drain(
        &mut engine,
        &mut resolver,
        &[[0xb0, 1, 0], [0xb0, 2, 127], [0xb0, 3, 127]],
    );
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages[0].1.addr, "/out/track/target/cue");
    assert_eq!(
        messages[0].1.args,
        vec![OscType::Float(0.0), OscType::Int(127)]
    );
    assert!(
        receiver
            .wait_for_messages(2, Duration::from_millis(150))
            .is_err(),
        "one Bang, one message"
    );
}
