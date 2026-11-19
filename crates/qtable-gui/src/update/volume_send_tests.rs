//! Volume send modes through the GUI message router, engine and loopback UDP.
use super::update;
use crate::app::App;
use crate::drafts::DraftKey;
use crate::drafts::MixerField;
use crate::messages::{EditMessage, Message};
use crate::mixer_band::MixerMessage;
use crate::mixer_device::{MixerMessage as DeviceMessage, shown_volume};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{Project, Route, SendMode, Track, TrackId};
use qtable_test_support::TestReceiver;
use std::time::Duration;

const TRACK: TrackId = TrackId(1);

fn app(mode: SendMode) -> App {
    let mut track = Track::new(TRACK, "Track", 0);
    track.route = Route::single(TrackId(2));
    track.mixer_device.volume.output.enabled = true;
    track.mixer_device.volume.output.send_mode = mode;
    let project = Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(TrackId(2), "Out", "127.0.0.1", 9000)],
        ..Project::default()
    };
    App::new(Engine::new(project).unwrap(), "120".into(), None)
}

/// Sends the message and returns how many project edits reached the engine.
fn edits(app: &mut App, message: Message) -> u64 {
    let before = app.project_revision();
    let _ = update(app, message);
    app.project_revision().wrapping_sub(before)
}

fn mixer(app: &mut App, message: MixerMessage) -> u64 {
    edits(app, Message::Mixer(message))
}

fn model_volume(app: &App) -> f64 {
    app.project()
        .track(TRACK)
        .unwrap()
        .mixer_device
        .volume
        .value
}

fn shown(app: &App) -> f64 {
    shown_volume(app, app.project().track(TRACK).unwrap())
}

/// Drive the strip's actual messages through the top-level router, including the
/// Detail Mixer dropdown, with the real scheduler and an ephemeral UDP receiver.
/// No GUI, pointer events or keyboard events are needed.
#[test]
fn strip_drag_udp_packets_obey_the_detail_mixer_send_mode() {
    for kind in ["track", "group", "output", "main"] {
        for mode in SendMode::ALL {
            let receiver = TestReceiver::bind().unwrap();
            let mut project = Project::default();
            let output = Track::new_output(TrackId(2), "Out", "127.0.0.1", receiver.addr().port());
            let id = if kind == "main" {
                project.main.id
            } else {
                TRACK
            };
            let mut track = match kind {
                "group" => Track::new_group(id, "Group", 0),
                "output" => Track::new_output(id, "Out", "127.0.0.1", receiver.addr().port()),
                "main" => {
                    let mut main = Track::new_main(id);
                    main.destination_mut().unwrap().port = receiver.addr().port();
                    main
                }
                _ => Track::new(id, "Track", 0),
            };
            if !track.capabilities().destination {
                track.route = Route::single(TrackId(2));
            }
            track.mixer_device.volume.output.enabled = true;
            match kind {
                "main" => project.main = track,
                "output" => project.outputs.push(track),
                _ => {
                    project.tracks.push(track);
                    if kind == "group" {
                        let mut member = Track::new(TrackId(3), "Member", 0);
                        member.group = Some(id);
                        member.route = Route::Group;
                        project.tracks.push(member);
                    }
                    project.outputs.push(output);
                }
            }
            let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
            edits(
                &mut app,
                Message::Edit(EditMessage::MixerDevice(DeviceMessage::SendMode(id, mode))),
            );
            assert_eq!(
                app.project()
                    .track(id)
                    .unwrap()
                    .mixer_device
                    .volume
                    .output
                    .send_mode,
                mode
            );
            assert!(
                receiver
                    .wait_for_messages(1, Duration::from_millis(80))
                    .is_err()
            );
            // Two drags verify that the held value is cleared between gestures.
            for values in [[0.8, 0.6, 0.4], [0.5, 0.7, 0.9]] {
                let mut volume_packets = 0;
                for value in values {
                    mixer(&mut app, MixerMessage::DragVolume(id, value));
                    if mode != SendMode::OnRelease {
                        let packets = receiver
                            .wait_for_messages(1, Duration::from_secs(1))
                            .unwrap();
                        assert_eq!(packets[0].1.addr, "/volume");
                        assert_eq!(packets[0].1.args.len(), 1);
                        assert_eq!(packets[0].1.args[0].clone().float(), Some(value as f32));
                        volume_packets += packets.len();
                    } else {
                        assert!(
                            receiver
                                .wait_for_messages(1, Duration::from_millis(80))
                                .is_err(),
                            "{kind} {mode}: a drag step sent OSC"
                        );
                    }
                }
                let final_value = values[2];
                mixer(&mut app, MixerMessage::ReleaseVolume(id, final_value));
                if mode == SendMode::OnRelease {
                    let packets = receiver
                        .wait_for_messages(1, Duration::from_secs(1))
                        .unwrap();
                    assert_eq!(packets[0].1.addr, "/volume");
                    assert_eq!(packets[0].1.args.len(), 1);
                    assert_eq!(
                        packets[0].1.args[0].clone().float(),
                        Some(final_value as f32)
                    );
                    volume_packets += packets.len();
                }
                assert!(
                    receiver
                        .wait_for_messages(1, Duration::from_millis(80))
                        .is_err(),
                    "{kind} {mode}: extra volume packet after release"
                );
                assert_eq!(
                    volume_packets,
                    if mode == SendMode::OnRelease { 1 } else { 3 }
                );
                eprintln!("{kind} {mode}: {volume_packets} volume packets per drag");
            }
            // Detail Mixer has a number field, not another fader. Its draft is
            // silent and its submission sends once in either mode.
            let key = DraftKey::Mixer(id, MixerField::Volume);
            edits(
                &mut app,
                Message::Edit(EditMessage::MixerDevice(DeviceMessage::Draft(
                    key,
                    "0.25".into(),
                ))),
            );
            assert!(
                receiver
                    .wait_for_messages(1, Duration::from_millis(80))
                    .is_err()
            );
            edits(
                &mut app,
                Message::Edit(EditMessage::MixerDevice(DeviceMessage::Submit(key))),
            );
            let packets = receiver
                .wait_for_messages(1, Duration::from_secs(1))
                .unwrap();
            assert_eq!(packets[0].1.addr, "/volume");
            assert_eq!(packets[0].1.args.len(), 1);
            assert_eq!(packets[0].1.args[0].clone().float(), Some(0.25));
            assert!(
                receiver
                    .wait_for_messages(1, Duration::from_millis(80))
                    .is_err()
            );
        }
    }
}

#[test]
fn continuous_edits_on_every_drag_step_and_the_release_adds_none() {
    let mut app = app(SendMode::Continuous);
    assert_eq!(mixer(&mut app, MixerMessage::DragVolume(TRACK, 0.8)), 1);
    assert_eq!(mixer(&mut app, MixerMessage::DragVolume(TRACK, 0.6)), 1);
    assert_eq!(mixer(&mut app, MixerMessage::ReleaseVolume(TRACK, 0.6)), 0);
    assert_eq!(model_volume(&app), 0.6);
    assert_eq!(shown(&app), 0.6);
}

#[test]
fn on_release_shows_the_drag_but_edits_once_when_it_ends() {
    let mut app = app(SendMode::OnRelease);
    assert_eq!(mixer(&mut app, MixerMessage::DragVolume(TRACK, 0.8)), 0);
    assert_eq!(mixer(&mut app, MixerMessage::DragVolume(TRACK, 0.6)), 0);
    assert_eq!(shown(&app), 0.6);
    assert_eq!(model_volume(&app), 1.0);
    assert_eq!(mixer(&mut app, MixerMessage::ReleaseVolume(TRACK, 0.6)), 1);
    assert_eq!(model_volume(&app), 0.6);
    assert_eq!(shown(&app), 0.6);
}

#[test]
fn on_release_ending_where_it_started_makes_no_edit_and_clears_the_held_value() {
    let mut app = app(SendMode::OnRelease);
    mixer(&mut app, MixerMessage::DragVolume(TRACK, 0.5));
    assert_eq!(shown(&app), 0.5);
    assert_eq!(mixer(&mut app, MixerMessage::ReleaseVolume(TRACK, 1.0)), 0);
    assert_eq!(shown(&app), 1.0);
}

#[test]
fn on_release_typed_values_and_unity_reset_edit_at_once() {
    let mut app = app(SendMode::OnRelease);
    let key = DraftKey::Mixer(TRACK, MixerField::Volume);
    let draft = DeviceMessage::Draft(key, "0.25".into());
    assert_eq!(
        edits(&mut app, Message::Edit(EditMessage::MixerDevice(draft))),
        0
    );
    let submit = DeviceMessage::Submit(key);
    assert_eq!(
        edits(&mut app, Message::Edit(EditMessage::MixerDevice(submit))),
        1
    );
    assert_eq!(model_volume(&app), 0.25);
    // Delete on a touched fader dispatches SetVolume(track, 1.0).
    assert_eq!(mixer(&mut app, MixerMessage::SetVolume(TRACK, 1.0)), 1);
    assert_eq!(model_volume(&app), 1.0);
}

#[test]
fn a_held_drag_value_shows_only_in_on_release() {
    let mut app = app(SendMode::OnRelease);
    mixer(&mut app, MixerMessage::DragVolume(TRACK, 0.4));
    let mut output = app
        .project()
        .track(TRACK)
        .unwrap()
        .mixer_device
        .volume
        .output
        .clone();
    output.send_mode = SendMode::Continuous;
    app.engine
        .execute(Command::Edit(ProjectEdit::SetParameterOutput {
            target: qtable_engine::model::ParameterRef::volume(TRACK),
            output,
        }))
        .unwrap();
    app.snapshot = app.engine.snapshot();
    assert_eq!(shown(&app), 1.0);
}

/// track-valve-build: the strip's Activator closes everything the track sends while its
/// envelope keeps moving; a fader drag while muted overrides without sending, and the
/// Activator's reopen sends the current (overridden) value at once.
#[test]
fn the_strip_activator_closes_envelope_and_fader_sends_and_reopens_the_current_value() {
    use qtable_engine::model::{
        ArgumentRef, AutomationEnvelope, BeatTime, Breakpoint, Clip, ClipId, DeviceRef,
        LIVE_CLIP_PALETTE, Lane,
    };
    let receiver = TestReceiver::bind().unwrap();
    let mut track = Track::new(TRACK, "Track", 1);
    track.route = Route::single(TrackId(2));
    track.mixer_device.volume.output.enabled = true;
    let field = crate::mixer_device::volume_field(TRACK);
    let mut clip = Clip::new(ClipId(1), "Ramp", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.lanes.push(Lane::Envelope(AutomationEnvelope {
        region: None,
        target: ArgumentRef {
            device: DeviceRef::Mixer,
            parameter: field.parameter.key.clone(),
            field: field.field.clone(),
        },
        points: [(0, 0.0), (3840, 1.0)]
            .map(|(time, value)| Breakpoint {
                time: BeatTime(time),
                value,
                curve: 0.0,
            })
            .into(),
    }));
    track.slots[0].clip = Some(clip);
    let project = Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(
            TrackId(2),
            "Out",
            "127.0.0.1",
            receiver.addr().port(),
        )],
        scenes: vec![qtable_engine::model::Scene {
            id: qtable_engine::model::SceneId(1),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    };
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let automated = |app: &App| {
        app.engine
            .snapshot()
            .automation
            .iter()
            .find(|value| value.field == field)
            .map(|value| value.value)
    };
    let silent = |ms| {
        receiver
            .wait_for_messages(1, Duration::from_millis(ms))
            .is_err()
    };
    let _ = update(
        &mut app,
        Message::Grid(crate::grid::Action::LaunchClip(TRACK, 0)),
    );
    let ramp = receiver
        .wait_for_messages(3, Duration::from_secs(2))
        .unwrap();
    assert!(ramp.iter().all(|(_, message)| message.addr == "/volume"));
    mixer(&mut app, MixerMessage::Activator(TRACK));
    // A frame already owed before the click may still go; then nothing does.
    std::thread::sleep(Duration::from_millis(100));
    while !silent(1) {}
    let before = automated(&app);
    assert!(silent(300), "a muted envelope sent OSC");
    assert_ne!(
        automated(&app),
        before,
        "the envelope keeps moving while muted"
    );
    for value in [0.2, 0.25, 0.3] {
        mixer(&mut app, MixerMessage::DragVolume(TRACK, value));
    }
    mixer(&mut app, MixerMessage::ReleaseVolume(TRACK, 0.3));
    assert!(silent(200), "a muted fader sent OSC");
    let reopened = std::time::Instant::now();
    mixer(&mut app, MixerMessage::Activator(TRACK));
    let first = receiver
        .wait_for_messages(1, Duration::from_millis(500))
        .unwrap();
    assert!(first[0].0 - reopened < Duration::from_millis(200));
    assert_eq!(first[0].1.addr, "/volume");
    // The drag overrode the envelope while muted, so the reopen sends its value.
    assert_eq!(first[0].1.args[0].clone().float(), Some(0.3));
    assert!(silent(200), "an overridden value stays put");
}
