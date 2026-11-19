use qtable_engine::engine::{Command, Engine};
use qtable_engine::file;
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use rosc::OscType;
use std::time::Duration;

fn load() -> Project {
    file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v6-hit-arguments.ron"
    ))
    .unwrap()
}

#[test]
fn migrated_event_defaults_come_from_the_first_hit() {
    let project = load();
    let hit = &project.tracks[0].slots[0].clip.as_ref().unwrap().events[0];
    let (_, event) = project.tracks[0].event_def(&hit.definition).unwrap();
    let defaults: Vec<_> = event.fields.iter().map(|f| f.default.clone()).collect();
    assert_eq!(defaults, hit.arguments);
    for field in &event.fields {
        field.validate(false).unwrap();
    }
}

#[test]
fn migrated_trigger_fires_the_values_its_hit_holds() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = load();
    project.outputs[0].output.as_mut().unwrap().destination.port = receiver.addr().port();
    let event = project.tracks[0].slots[0].clip.as_ref().unwrap().events[0]
        .definition
        .clone();
    let mut engine = Engine::new(project).unwrap();
    engine
        .execute(Command::FireEvent {
            track: TrackId(1),
            event,
        })
        .unwrap();
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        messages[0].1.args,
        vec![
            OscType::Int(0),
            OscType::Float(0.5),
            OscType::String("hello".into()),
            OscType::Bool(true),
            OscType::Int(7),
            OscType::Float(-1.25),
            OscType::String("wrapped".into()),
        ]
    );
}
