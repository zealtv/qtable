//! Regression: `SetTempo` once left the scheduler's copy of the project at the old tempo, so
//! the next value-only update tripped the scheduler's `debug_assert` (debug builds) and stopped it.
use qtable_engine::{
    edit::ProjectEdit,
    engine::{Command, Engine},
    model::*,
};
use qtable_test_support::TestReceiver;
use rosc::OscType;
use std::time::Duration;

fn project(port: u16) -> Project {
    let mut track = Track::new(TrackId(1), "One", 1);
    track.route = Route::single(TrackId(201));
    let out = &mut track.mixer_device.volume.output;
    out.enabled = true;
    out.address = "/volume".into();
    out.send_mode = SendMode::OnRelease;
    out.mapping = Mapping::new([0.0, 1.0], [10.0, 20.0], 1.0, 0.0).unwrap();
    Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(TrackId(201), "Out", "127.0.0.1", port)],
        scenes: vec![Scene {
            id: SceneId(1),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    }
}

#[test]
fn a_value_edit_after_a_tempo_change_is_sent_and_the_scheduler_keeps_running() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(project(receiver.addr().port())).unwrap();
    engine.execute(Command::SetTempo(100.0)).unwrap();
    for value in [0.3, 0.4] {
        engine
            .execute(Command::Edit(ProjectEdit::SetParameter {
                target: ParameterRef::volume(TrackId(1)),
                values: vec![(FieldKey("value".into()), value)],
            }))
            .unwrap();
        let sent = receiver
            .wait_for_messages(1, Duration::from_secs(2))
            .unwrap();
        assert_eq!(sent[0].1.addr, "/volume");
        assert_eq!(
            sent[0].1.args,
            [OscType::Float((10.0 + 10.0 * value) as f32)]
        );
    }
    assert_eq!(engine.snapshot().status.scheduler_failure, None);
    assert_eq!(engine.snapshot().tempo_bpm, 100.0);
}
