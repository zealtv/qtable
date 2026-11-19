//! Value-only commands preserve cached events and their recipient delays.
use qtable_engine::{
    edit::ProjectEdit,
    engine::{Command, Engine},
    model::*,
};
use qtable_test_support::TestReceiver;
use std::time::Duration;

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn repeated_parameter_updates_keep_cached_hits_once_and_in_order() {
    for bundle in [false, true] {
        for delay in [0.0, 100.0] {
            let receiver = TestReceiver::bind().unwrap();
            let mut project = fixture::project(receiver.addr().port(), &[delay]);
            project.tempo = Tempo::new(120.0).unwrap();
            project.tracks[0].slots[0].clip.as_mut().unwrap().looping = false;
            project.outputs[0].output.as_mut().unwrap().stage.bundle = bundle;
            let field = ArgumentRef::volume().on(TrackId(1));
            let mut engine = Engine::new(project).unwrap();
            engine
                .execute(Command::LaunchClip {
                    track: TrackId(1),
                    slot: 0,
                })
                .unwrap();
            receiver
                .wait_for_messages(1, Duration::from_secs(2))
                .unwrap();
            // Keep editing across the next hit deadline so the engine
            // exercises its cached plan with the production lookahead.
            for step in 0..100 {
                engine
                    .execute(Command::Edit(ProjectEdit::SetParameter {
                        target: field.parameter.clone(),
                        values: vec![(field.field.clone(), (step % 20) as f64 / 20.0)],
                    }))
                    .unwrap();
                std::thread::sleep(Duration::from_millis(3));
            }
            let messages = receiver
                .wait_for_messages(2, Duration::from_secs(2))
                .unwrap();
            let addresses: Vec<_> = messages
                .iter()
                .map(|(_, message)| message.addr.as_str())
                .collect();
            assert_eq!(
                addresses,
                ["/t1/s0/e0", "/t1/s0/e2"],
                "bundle={bundle}, delay={delay}"
            );
            assert!(
                receiver
                    .wait_for_messages(1, Duration::from_millis(100))
                    .is_err(),
                "no duplicate or replayed cached event"
            );
        }
    }
}
