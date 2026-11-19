//! Real worker/UDP regressions for the independent live volume controller.
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use rosc::OscType;
use std::time::{Duration, Instant};
fn project(receiver: &TestReceiver) -> Project {
    let mut track = Track::new(TrackId(1), "Track", 1);
    track.route = Route::single(TrackId(2));
    track.mixer_device.volume.output.enabled = true;
    let mut clip = Clip::new(ClipId(1), "Clip", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.looping = false;
    let path = OscAddress::Relative("/clip".into());
    let arguments = vec![OscArgument::Float(0.75)];
    clip.events
        .push(track.hit(BeatTime::ZERO, path, arguments, || DeviceId(1)));
    track.slots[0].clip = Some(clip);
    Project {
        tracks: vec![track],
        outputs: vec![
            Track::new_output(TrackId(2), "Out", "127.0.0.1", receiver.addr().port())
                .with_path("/out"),
        ],
        scenes: vec![Scene {
            id: SceneId(1),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    }
}
fn receive(receiver: &TestReceiver, address: &str, value: f32) {
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(1))
        .unwrap();
    assert_eq!(messages[0].1.addr, address);
    assert_eq!(messages[0].1.args, vec![OscType::Float(value)]);
}
fn silent(receiver: &TestReceiver) {
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(40))
            .is_err()
    );
}
#[test]
fn live_edits_start_and_stop_obey_send_triggers() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(project(&receiver)).unwrap();
    silent(&receiver);
    engine
        .execute(Command::Edit(ProjectEdit::set_value(
            qtable_engine::model::ParameterRef::volume(TrackId(1)),
            0.4,
        )))
        .unwrap();
    receive(&receiver, "/out/volume", 0.4);
    assert!(!engine.snapshot().playing);
    engine
        .execute(Command::Edit(ProjectEdit::set_value(
            qtable_engine::model::ParameterRef::volume(TrackId(1)),
            0.4,
        )))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::RenameTrack {
            track: TrackId(1),
            name: "Renamed".into(),
        }))
        .unwrap();
    silent(&receiver);
    engine.execute(Command::Play).unwrap();
    receive(&receiver, "/out/volume", 0.4);
    engine.execute(Command::Play).unwrap();
    silent(&receiver);
    engine.execute(Command::Stop).unwrap();
    silent(&receiver);
    let output = engine.project().outputs[0].id;
    let active = false;
    engine
        .execute(Command::Edit(ProjectEdit::SetActive {
            strips: vec![output],
            active,
        }))
        .unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::set_value(
            qtable_engine::model::ParameterRef::volume(TrackId(1)),
            0.6,
        )))
        .unwrap();
    silent(&receiver);
    let output = engine.project().outputs[0].id;
    let active = true;
    engine
        .execute(Command::Edit(ProjectEdit::SetActive {
            strips: vec![output],
            active,
        }))
        .unwrap();
    receive(&receiver, "/out/volume", 0.6);
    let mut settings = engine.project().tracks[0]
        .mixer_device
        .volume
        .output
        .clone();
    settings.enabled = false;
    engine
        .execute(Command::Edit(ProjectEdit::SetParameterOutput {
            target: qtable_engine::model::ParameterRef::volume(TrackId(1)),
            output: settings,
        }))
        .unwrap();
    silent(&receiver);
}
#[test]
fn launch_initializes_before_equal_deadline_clip_and_does_not_scale_arguments() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver);
    p.tracks[0].mixer_device.volume.value = 0.25;
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    receive(&receiver, "/out/volume", 0.25);
    receive(&receiver, "/out/clip", 0.75);
    silent(&receiver);
    engine.execute(Command::Stop).unwrap();
    engine
        .execute(Command::LaunchScene { scene: SceneId(1) })
        .unwrap();
    receive(&receiver, "/out/volume", 0.25);
    receive(&receiver, "/out/clip", 0.75);
}
#[test]
fn output_controls_self_send_and_endpoint_edits_send_only_to_new_receiver() {
    let a = TestReceiver::bind().unwrap();
    let b = TestReceiver::bind().unwrap();
    let mut p = project(&a);
    p.tracks[0].mixer_device.volume.output.enabled = false;
    p.outputs[0].mixer_device.volume.output.enabled = true;
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::set_value(
            qtable_engine::model::ParameterRef::volume(TrackId(2)),
            0.3,
        )))
        .unwrap();
    receive(&a, "/out/volume", 0.3);
    let mut output = engine.project().outputs[0].clone();
    output.destination_mut().unwrap().port = b.addr().port();
    engine
        .execute(Command::Edit(ProjectEdit::SetDestination {
            output: output.id,
            destination: output.output,
        }))
        .unwrap();
    receive(&b, "/out/volume", 0.3);
    silent(&a);
}
#[test]
fn on_release_mode_changes_send_nothing_and_each_value_edit_or_start_sends_once() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(project(&receiver)).unwrap();
    silent(&receiver);
    let mut output = engine.project().tracks[0]
        .mixer_device
        .volume
        .output
        .clone();
    output.send_mode = SendMode::OnRelease;
    engine
        .execute(Command::Edit(ProjectEdit::SetParameterOutput {
            target: qtable_engine::model::ParameterRef::volume(TrackId(1)),
            output,
        }))
        .unwrap();
    silent(&receiver);
    // The GUI sends one SetParameter when a drag ends, a typed value or a Delete reset.
    for value in [0.35, 1.0] {
        engine
            .execute(Command::Edit(ProjectEdit::set_value(
                qtable_engine::model::ParameterRef::volume(TrackId(1)),
                value,
            )))
            .unwrap();
        receive(&receiver, "/out/volume", value as f32);
        silent(&receiver);
    }
    engine.execute(Command::Play).unwrap();
    receive(&receiver, "/out/volume", 1.0);
    silent(&receiver);
}
fn volume(engine: &mut Engine, track: TrackId, value: f64) {
    engine
        .execute(Command::Edit(ProjectEdit::set_value(
            ParameterRef::volume(track),
            value,
        )))
        .unwrap();
}
fn edit(engine: &mut Engine, edit: ProjectEdit) {
    engine.execute(Command::Edit(edit)).unwrap();
}
#[test]
fn closed_valves_send_no_volume_and_reopening_sends_the_current_value_once() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver);
    // The valve opens without waiting for the track's delay.
    p.tracks[0].delay_ms = 600.0;
    let mut group = Track::new_group(TrackId(10), "Group", 1);
    group.route = Route::single(TrackId(2));
    p.tracks.insert(0, group);
    p.tracks[1].group = Some(TrackId(10));
    let mut other = Track::new(TrackId(3), "Other", 1);
    other.route = Route::single(TrackId(2));
    p.tracks.push(other);
    let mut engine = Engine::new(p).unwrap();
    let active = |strips: Vec<TrackId>, active| ProjectEdit::SetActive { strips, active };
    let solo = |solo| ProjectEdit::SetSolo {
        strips: vec![TrackId(3)],
        solo,
        exclusive: false,
    };
    let closes: [(ProjectEdit, ProjectEdit); 3] = [
        (
            active(vec![TrackId(1)], false),
            active(vec![TrackId(1)], true),
        ),
        (
            active(vec![TrackId(10)], false),
            active(vec![TrackId(10)], true),
        ),
        (solo(true), solo(false)),
    ];
    for (round, (close, reopen)) in closes.into_iter().enumerate() {
        edit(&mut engine, close);
        silent(&receiver);
        // A typed value and a reset store the value and send nothing, nor does transport start.
        volume(&mut engine, TrackId(1), 0.2 + round as f64 / 10.0);
        engine.execute(Command::Play).unwrap();
        engine.execute(Command::Stop).unwrap();
        silent(&receiver);
        let value = engine.project().tracks[1].mixer_device.volume.value as f32;
        let reopened = Instant::now();
        edit(&mut engine, reopen);
        let messages = receiver
            .wait_for_messages(1, Duration::from_millis(400))
            .unwrap();
        assert_eq!(messages[0].1.addr, "/out/volume");
        assert_eq!(messages[0].1.args, vec![OscType::Float(value)]);
        assert!(
            messages[0].0 - reopened < Duration::from_millis(300),
            "round {round} reopened {:?} late",
            messages[0].0 - reopened
        );
        silent(&receiver);
    }
    // Reopening an unchanged value still resends it.
    edit(&mut engine, active(vec![TrackId(1)], false));
    edit(&mut engine, active(vec![TrackId(1)], true));
    receive(&receiver, "/out/volume", 0.4);
    silent(&receiver);
}
#[test]
fn reopening_one_destination_resends_only_there_and_outputs_keep_their_own_switches() {
    let a = TestReceiver::bind().unwrap();
    let b = TestReceiver::bind().unwrap();
    let mut p = project(&a);
    p.outputs
        .push(Track::new_output(TrackId(4), "Other", "127.0.0.1", b.addr().port()).with_path("/b"));
    p.tracks[0].route = Route::Outputs(vec![TrackId(2), TrackId(4)]);
    p.outputs[0].mixer_device.volume.output.enabled = true;
    let mut engine = Engine::new(p).unwrap();
    let output_active = |engine: &Engine, active| ProjectEdit::SetActive {
        strips: vec![engine.project().outputs[0].id],
        active,
    };
    let closed = output_active(&engine, false);
    edit(&mut engine, closed);
    let reopened = output_active(&engine, true);
    edit(&mut engine, reopened);
    // Output 2 resends its own volume and the track's; output 4 stayed open and hears nothing.
    let mut messages: Vec<_> = a
        .wait_for_messages(2, Duration::from_millis(500))
        .unwrap()
        .into_iter()
        .map(|(_, message)| message.addr)
        .collect();
    messages.sort();
    assert_eq!(messages, ["/out/volume", "/out/volume"]);
    silent(&a);
    silent(&b);
    // An output's Solo selects sources; its own volume still sends while another output solos.
    edit(
        &mut engine,
        ProjectEdit::SetOutputSolo {
            output: TrackId(4),
            solo: true,
            exclusive: false,
        },
    );
    volume(&mut engine, TrackId(2), 0.5);
    receive(&a, "/out/volume", 0.5);
    silent(&b);
}
