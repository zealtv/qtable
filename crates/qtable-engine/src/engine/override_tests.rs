//! Pins the reading behind the undo restore:
//! what a full `UpdateProject` does to a stored value under an overridden envelope.
use super::*;
use crate::edit::ProjectEdit;
use crate::model::*;
use crate::scheduler::{AutomationState, SchedulerCommand};
use qtable_test_support::TestReceiver;
use rosc::OscType;

fn volume_argument() -> ArgumentRef {
    ArgumentRef::volume()
}

fn volume_field() -> FieldRef {
    volume_argument().on(TrackId(1))
}

/// One looping 16-beat clip with a 0..1 Volume ramp; Volume sends `10 + 10 * value`.
fn project(receiver: &TestReceiver) -> Project {
    let mut track = Track::new(TrackId(1), "Automated", 1);
    track.route = Route::single(TrackId(2));
    let volume = &mut track.mixer_device.volume.output;
    volume.enabled = true;
    volume.address = "/volume".into();
    volume.send_mode = SendMode::OnRelease;
    volume.mapping = Mapping::new([0.0, 1.0], [10.0, 20.0], 1.0, 0.0).unwrap();
    let mut clip = Clip::new(ClipId(1), "Ramp", BeatTime(16 * 960), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.lanes.push(Lane::Envelope(AutomationEnvelope {
        region: None,
        target: volume_argument(),
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
    track.slots[0].clip = Some(clip);
    Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(
            TrackId(2),
            "Out",
            "127.0.0.1",
            receiver.addr().port(),
        )],
        scenes: vec![Scene {
            id: SceneId(1),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    }
}

fn automation(engine: &Engine) -> Option<(f64, AutomationState)> {
    engine
        .snapshot()
        .automation
        .iter()
        .find(|v| v.field == volume_field())
        .map(|v| (v.value, v.state))
}

fn wait_for_state(engine: &Engine, wanted: AutomationState) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while automation(engine).is_none_or(|(_, state)| state != wanted) {
        assert!(Instant::now() < deadline, "never reached {wanted:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Every `/volume` value received until `quiet` passes without one.
fn volumes(receiver: &TestReceiver, quiet: Duration) -> Vec<f32> {
    let mut seen = Vec::new();
    while let Ok(batch) = receiver.wait_for_messages(1, quiet) {
        if batch[0].1.addr == "/volume"
            && let [OscType::Float(v)] = batch[0].1.args.as_slice()
        {
            seen.push(*v);
        }
    }
    seen
}

#[test]
fn a_full_update_keeps_the_old_override_value_so_restores_send_changed_values_separately() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(project(&receiver)).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    wait_for_state(&engine, AutomationState::Following);
    // Override by hand: the stored value becomes 0.5 and the envelope steps aside.
    let field = volume_field();
    engine
        .execute(Command::Edit(ProjectEdit::SetParameter {
            target: field.parameter.clone(),
            values: vec![(field.field.clone(), 0.5)],
        }))
        .unwrap();
    wait_for_state(&engine, AutomationState::Overridden);
    let overridden = volumes(&receiver, Duration::from_millis(150));
    assert_eq!(overridden.last(), Some(&15.0), "{overridden:?}");
    // A full update whose only difference is the stored value, as an undo would send.
    let mut restored = engine.project.clone();
    restored
        .set_parameter_values(&field.parameter, vec![(field.field.clone(), 0.25)])
        .unwrap();
    engine
        .scheduler
        .send(SchedulerCommand::UpdateProject(restored.clone()))
        .unwrap();
    engine.project = restored;
    std::thread::sleep(Duration::from_millis(100));
    let published = automation(&engine);
    let sent = volumes(&receiver, Duration::from_millis(150));
    println!("FIRST ACTION: published {published:?}, packets after the full update {sent:?}");
    // What `automation::sync` does: it carries the old override value (0.5, wire 15) across a
    // changed stored value, so the engine would show 0.25 while the receiver kept 15.
    assert_eq!(published, Some((0.5, AutomationState::Overridden)));
    assert!(sent.is_empty(), "{sent:?}");
}

#[test]
fn a_restore_that_differs_only_in_tempo_sends_the_tempo_and_no_full_update() {
    use crate::scheduler::SchedulerEvent;
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(project(&receiver)).unwrap();
    engine.scheduler =
        crate::scheduler::spawn_with_marks(engine.project.clone(), Duration::from_millis(5))
            .unwrap();
    engine.published = Arc::clone(&engine.scheduler.snapshot);
    engine.execute(Command::SetTempo(90.0)).unwrap();
    let settle = |engine: &Engine| {
        let mut labels = Vec::new();
        let until = Instant::now() + Duration::from_millis(400);
        while let Ok(event) = engine
            .scheduler_events()
            .recv_timeout(until.saturating_duration_since(Instant::now()))
        {
            if let SchedulerEvent::Marks(batch) = event {
                labels.extend(batch.into_iter().map(|(label, _)| label));
            }
        }
        labels
    };
    let _ = settle(&engine);
    engine.execute(Command::Undo).unwrap();
    assert_eq!(engine.project.tempo.bpm(), 120.0);
    let labels = settle(&engine);
    let commands = labels.iter().filter(|l| **l == "command.begin").count();
    assert_eq!(commands, 1, "only SetTempo: {labels:?}");
}
