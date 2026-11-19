//! One hold, keyed by `FieldRef`, serving the Mixer fader and the Target face.
use super::*;
use crate::device_chain::target_face::{Action, FaceField};
use crate::messages::Message;
use crate::mixer_band::MixerMessage;
use crate::mixer_device::{shown_volume, volume_field};
use qtable_engine::edit::{ProjectEdit, TargetEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{
    DeviceId, EntryKey, FieldKey, NewDevice, ParameterKey, ParameterRef, TrackId,
};
use std::time::Duration;

/// A new set whose first track has a Target with one parameter; both it and Mixer Volume
/// send On Release.
fn app() -> (App, TrackId, FaceField) {
    let mut app = App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        "120".into(),
        None,
    );
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(track, false)),
    );
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::AddDevice {
            track,
            kind: NewDevice::Target,
            before: None,
        }),
    );
    crate::device_chain::reconcile(&mut app);
    let device = app.project().track(track).unwrap().devices[0].id;
    face(&mut app, Action::Add(device, true));
    let entry = target(&app, device).parameters[0].key.clone();
    face(
        &mut app,
        Action::Definition(
            device,
            TargetEdit::SetParameterSendMode {
                key: entry.clone(),
                mode: SendMode::OnRelease,
            },
        ),
    );
    let reference = ParameterRef::volume(track);
    let mut output = app
        .project()
        .parameter(&reference)
        .unwrap()
        .output
        .into_owned();
    output.enabled = true;
    output.send_mode = SendMode::OnRelease;
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::SetParameterOutput {
            target: reference,
            output,
        }),
    );
    let field = target(&app, device).parameters[0].fields[0].key.clone();
    let key = FaceField {
        device,
        entry,
        field,
        parameter: true,
    };
    (app, track, key)
}

fn target(app: &App, device: DeviceId) -> &qtable_engine::model::TargetDevice {
    app.project().device(device).unwrap().1.target().unwrap()
}

fn face(app: &mut App, action: Action) {
    let _ = crate::update::update(
        app,
        Message::Device(crate::device_chain::Action::Target(action)),
    );
}

fn mixer(app: &mut App, message: MixerMessage) {
    let _ = crate::update::update(app, Message::Mixer(message));
}

/// The Target argument behind a face control, as the gesture keys it.
fn argument(app: &App, track: TrackId, key: &FaceField) -> FieldRef {
    let field = FieldRef {
        parameter: ParameterRef {
            track,
            device: DeviceRef::Chain(key.device),
            key: ParameterKey(key.entry.0.clone()),
        },
        field: key.field.clone(),
    };
    assert!(app.project().field(&field).is_some());
    field
}

fn stored(app: &App, field: &FieldRef) -> f64 {
    app.project().field(field).unwrap().value
}

#[test]
fn one_hold_serves_the_mixer_fader_and_the_target_face_at_once() {
    let (mut app, track, key) = app();
    let volume = volume_field(track);
    let target = argument(&app, track, &key);
    let revision = app.project_revision();
    mixer(&mut app, MixerMessage::DragVolume(track, 0.3));
    face(&mut app, Action::Value(key.clone(), 0.6, false));
    assert_eq!(app.parameter_gesture.held(&volume), Some(0.3));
    assert_eq!(app.parameter_gesture.held(&target), Some(0.6));
    assert_eq!(shown_value(&app, &volume), Some(0.3));
    assert_eq!(shown_volume(&app, app.project().track(track).unwrap()), 0.3);
    assert_eq!(shown_value(&app, &target), Some(0.6));
    assert_eq!(
        app.project_revision(),
        revision,
        "On Release drags send nothing"
    );

    // Each release edits its own argument once and ends only its own hold.
    face(&mut app, Action::Value(key.clone(), 0.6, true));
    assert_eq!(app.project_revision(), revision + 1);
    assert_eq!(stored(&app, &target), 0.6);
    assert_eq!(app.parameter_gesture.held(&target), None);
    assert_eq!(app.parameter_gesture.held(&volume), Some(0.3));
    mixer(&mut app, MixerMessage::ReleaseVolume(track, 0.3));
    assert_eq!(app.project_revision(), revision + 2);
    assert_eq!(stored(&app, &volume), 0.3);
    assert!(app.parameter_gesture.is_empty());

    // A release at the stored value (Continuous steps already sent it) edits nothing.
    mixer(&mut app, MixerMessage::DragVolume(track, 0.3));
    mixer(&mut app, MixerMessage::ReleaseVolume(track, 0.3));
    assert_eq!(app.project_revision(), revision + 2);
}

#[test]
fn shown_value_prefers_the_drag_then_the_effective_value_then_the_model() {
    let (mut app, track, key) = app();
    let now = Instant::now();
    for field in [volume_field(track), argument(&app, track, &key)] {
        let model = stored(&app, &field);
        let curve = |_: &App, _: &FieldRef| {
            Some(Effective {
                value: 0.25,
                state: AutomationState::Following,
            })
        };
        let nothing = |_: &App, _: &FieldRef| None;
        assert_eq!(shown_from(&app, &field, now, nothing), Some(model));
        assert_eq!(shown_from(&app, &field, now, curve), Some(0.25));
        assert_eq!(
            shown_value(&app, &field),
            Some(model),
            "no source drives it yet"
        );
        app.parameter_gesture.hold(&field, 0.75);
        assert_eq!(shown_from(&app, &field, now, curve), Some(0.75));
        app.parameter_gesture.end(&field);
        let gone = FieldRef {
            field: FieldKey("gone".into()),
            ..field.clone()
        };
        assert_eq!(shown_from(&app, &gone, now, nothing), None);
        assert_eq!(automation_state(&app, &field), AutomationState::None);
    }
}

#[test]
fn an_overridden_or_just_submitted_argument_shows_its_stored_value() {
    let (mut app, track, key) = app();
    let now = Instant::now();
    let driven = |state| move |_: &App, _: &FieldRef| Some(Effective { value: 0.25, state });
    for field in [volume_field(track), argument(&app, track, &key)] {
        let model = stored(&app, &field);
        // The engine's published value lags a manual edit, so an override shows the base.
        assert_eq!(
            shown_from(&app, &field, now, driven(AutomationState::Overridden)),
            Some(model)
        );
        assert_eq!(
            shown_from(&app, &field, now, driven(AutomationState::Following)),
            Some(0.25)
        );
        assert_eq!(
            shown_from(&app, &field, now, driven(AutomationState::None)),
            Some(0.25),
            "a held value shows as the engine reports it"
        );
        // Just submitted: the published value still shows the curve or the previous step.
        app.parameter_gesture.submitted(&field, now);
        for state in [AutomationState::Following, AutomationState::None] {
            assert_eq!(
                shown_from(
                    &app,
                    &field,
                    now + Duration::from_millis(100),
                    driven(state)
                ),
                Some(model)
            );
        }
        assert_eq!(
            shown_from(
                &app,
                &field,
                now + SUBMITTED_FOR,
                driven(AutomationState::Following)
            ),
            Some(0.25),
            "the engine's value returns once publication has had time to catch up"
        );
    }
}

#[test]
fn every_edit_that_sends_a_parameter_marks_it_and_polls_promptly() {
    let (mut app, track, key) = app();
    let volume = volume_field(track);
    let target = argument(&app, track, &key);
    app.fast_poll_until = None;
    mixer(&mut app, MixerMessage::SetVolume(track, 0.3));
    assert!(app.fast_poll_until.is_some(), "a drag trails without it");
    assert!(
        app.parameter_gesture
            .just_submitted(&volume, Instant::now())
    );
    assert!(
        !app.parameter_gesture
            .just_submitted(&target, Instant::now())
    );
    face(&mut app, Action::Value(key, 0.6, true));
    assert!(
        app.parameter_gesture
            .just_submitted(&target, Instant::now())
    );

    // Other edits send no parameter: they leave the idle poll rate alone.
    app.fast_poll_until = None;
    let _ = crate::update::update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::AddTrack),
    );
    assert!(app.fast_poll_until.is_none());
}

#[test]
fn one_reading_of_the_published_automation_serves_every_reader() {
    use crate::inspector::lane_model::live_value;
    use qtable_engine::scheduler::{AutomationState as Engine, AutomationValue};
    let (mut app, track, key) = app();
    let target = argument(&app, track, &key);
    let reported = |state| {
        vec![AutomationValue {
            field: target.clone(),
            value: 0.8,
            state,
        }]
    };
    for (state, expected, overridden) in [
        (Engine::Following, AutomationState::Following, false),
        (Engine::Overridden, AutomationState::Overridden, true),
        // A held value drives nothing: no LED, no override, but its value still shows.
        (Engine::Held, AutomationState::None, false),
    ] {
        app.snapshot.automation = reported(state);
        assert_eq!(automation_state(&app, &target), expected);
        assert_eq!(crate::transport::any_overridden(&app), overridden);
        let live = live_value(app.project(), &app.snapshot.automation, &target).unwrap();
        assert_eq!((live.value, live.state), (0.8, expected));
        assert_eq!(effective(&app, &target), Some(live));
    }
}

#[test]
fn a_hold_survives_target_selection_and_edits_to_other_arguments() {
    let (mut app, track, key) = app();
    let volume = volume_field(track);
    let target = argument(&app, track, &key);
    face(&mut app, Action::Add(key.device, true));
    let other = target_entry(&app, key.device, 1);

    // The Mixer's hold through Target entry selection and a Target drag and release.
    mixer(&mut app, MixerMessage::DragVolume(track, 0.3));
    face(&mut app, Action::Select(key.device, other.clone()));
    face(&mut app, Action::Select(key.device, key.entry.clone()));
    face(&mut app, Action::Value(key.clone(), 0.5, false));
    face(&mut app, Action::Value(key.clone(), 0.5, true));
    assert_eq!(app.parameter_gesture.held(&volume), Some(0.3));
    assert_eq!(stored(&app, &target), 0.5);

    // The face's hold through selection, a typed Volume and another Target argument's edit.
    face(&mut app, Action::Value(key.clone(), 0.75, false));
    face(&mut app, Action::Select(key.device, other.clone()));
    mixer(&mut app, MixerMessage::SetVolume(track, 0.9));
    let neighbour = FaceField {
        entry: other,
        field: target_field(&app, key.device, 1),
        ..key.clone()
    };
    face(&mut app, Action::Value(neighbour, 0.2, true));
    assert_eq!(app.parameter_gesture.held(&target), Some(0.75));
    assert_eq!(shown_value(&app, &target), Some(0.75));
    assert_eq!(stored(&app, &target), 0.5);
    assert_eq!(
        app.parameter_gesture.held(&volume),
        None,
        "SetVolume ends its own hold"
    );
}

fn target_entry(app: &App, device: DeviceId, index: usize) -> EntryKey {
    target(app, device).parameters[index].key.clone()
}

fn target_field(app: &App, device: DeviceId, index: usize) -> FieldKey {
    target(app, device).parameters[index].fields[0].key.clone()
}

#[test]
fn release_to_saved_base_overrides_a_moving_effective_value_on_the_face() {
    let (mut app, track, key) = app();
    let field = FieldRef {
        parameter: ParameterRef {
            track,
            device: DeviceRef::Chain(key.device),
            key: ParameterKey(key.entry.0.clone()),
        },
        field: key.field.clone(),
    };
    let base = app.project().field(&field).unwrap().value;
    app.snapshot
        .automation
        .push(qtable_engine::scheduler::AutomationValue {
            field,
            value: 0.75,
            state: qtable_engine::scheduler::AutomationState::Following,
        });
    let revision = app.engine.project_revision();
    face(&mut app, Action::Value(key, base, true));
    assert_eq!(app.engine.project_revision(), revision + 1);
}

/// The Target argument's engine-held value: its clip stopped, so the engine keeps the curve's
/// last value (0.8) while the stored base stays where it was.
fn hold_at(app: &mut App, field: &FieldRef, value: f64) {
    app.snapshot.automation = vec![qtable_engine::scheduler::AutomationValue {
        field: field.clone(),
        value,
        state: qtable_engine::scheduler::AutomationState::Held,
    }];
}

#[test]
fn a_reset_to_the_base_replaces_a_held_value_on_the_face() {
    let (mut app, track, key) = app();
    let field = argument(&app, track, &key);
    let base = stored(&app, &field);
    hold_at(&mut app, &field, 0.8);
    assert_eq!(shown_value(&app, &field), Some(0.8));
    assert_eq!(automation_state(&app, &field), AutomationState::None);
    let revision = app.engine.project_revision();
    face(&mut app, Action::Reset(key.clone()));
    assert_eq!(
        app.engine.project_revision(),
        revision + 1,
        "the reset sends the base though the model already stores it"
    );
    assert_eq!(stored(&app, &field), base);

    // With nothing held, the same reset changes nothing.
    app.snapshot.automation.clear();
    face(&mut app, Action::Reset(key));
    assert_eq!(app.engine.project_revision(), revision + 1);
}

#[test]
fn a_drag_or_release_landing_on_the_base_replaces_a_held_value_on_the_face() {
    let (mut app, track, key) = app();
    let field = argument(&app, track, &key);
    let entry = key.entry.clone();
    face(
        &mut app,
        Action::Definition(
            key.device,
            TargetEdit::SetParameterSendMode {
                key: entry,
                mode: SendMode::Continuous,
            },
        ),
    );
    let base = stored(&app, &field);
    for release in [false, true] {
        hold_at(&mut app, &field, 0.8);
        let revision = app.engine.project_revision();
        face(&mut app, Action::Value(key.clone(), base, release));
        assert_eq!(
            app.engine.project_revision(),
            revision + 1,
            "release {release}"
        );
    }
    // A Continuous drag step that repeats the stored value, with nothing held, sends nothing.
    app.snapshot.automation.clear();
    let revision = app.engine.project_revision();
    face(&mut app, Action::Value(key, base, false));
    assert_eq!(app.engine.project_revision(), revision);
}

fn drive(app: &mut App, field: &FieldRef, state: qtable_engine::scheduler::AutomationState) {
    app.snapshot
        .automation
        .retain(|value| value.field != *field);
    app.snapshot
        .automation
        .push(qtable_engine::scheduler::AutomationValue {
            field: field.clone(),
            value: 0.5,
            state,
        });
}

fn target_subject(app: &App, track: TrackId, key: &FaceField) -> crate::info::Subject {
    let device = app.project().track(track).unwrap().devices[0].id;
    assert_eq!(key.device, device);
    let field = argument(app, track, key);
    crate::info::Subject::TargetField(
        crate::info::EntryRef::Parameter(field.parameter),
        field.field,
    )
}

#[test]
fn marks_follow_the_state_of_each_argument_independently() {
    use qtable_engine::scheduler::AutomationState as Engine;
    let (mut app, track, key) = app();
    let volume = volume_field(track);
    let face = argument(&app, track, &key);
    let mark = |app: &App, field: &FieldRef| mark(automation_state(app, field));
    assert_eq!(mark(&app, &volume), None);
    assert_eq!(mark(&app, &face), None);

    drive(&mut app, &volume, Engine::Following);
    drive(&mut app, &face, Engine::Overridden);
    use crate::control::automation_mark::Mark;
    assert_eq!(mark(&app, &volume), Some(Mark::Following));
    assert_eq!(mark(&app, &face), Some(Mark::Overridden));

    // A held value after a stop is not a driven argument: no LED.
    drive(&mut app, &volume, Engine::Held);
    assert_eq!(mark(&app, &volume), None);
}

#[test]
fn re_enable_is_offered_on_face_and_mixer_menus_only_while_overridden() {
    use crate::context_menu::{Entry, MenuTarget, current_entries};
    use qtable_engine::scheduler::AutomationState as Engine;
    let (mut app, track, key) = app();
    let revision = app.engine.project_revision();
    let entry = |app: &App, subject: &crate::info::Subject| {
        current_entries(
            app,
            MenuTarget::Argument {
                subject: subject.clone(),
                revision,
            },
        )
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Command {
                label: "Re-Enable Automation",
                enabled,
                message,
                ..
            } => Some((enabled, message)),
            _ => None,
        })
        .expect("the entry is always listed")
    };
    let subjects = [
        (crate::info::Subject::Volume(track), volume_field(track)),
        (
            target_subject(&app, track, &key),
            argument(&app, track, &key),
        ),
    ];
    for (subject, field) in subjects {
        assert!(!entry(&app, &subject).0, "no envelope");
        drive(&mut app, &field, Engine::Following);
        assert!(!entry(&app, &subject).0, "following needs nothing");
        drive(&mut app, &field, Engine::Overridden);
        let (enabled, message) = entry(&app, &subject);
        assert!(enabled, "overridden");
        assert!(matches!(
            message,
            Message::ReEnableAutomation(Some(ref sent)) if *sent == field
        ));
    }
}

#[test]
fn the_control_bar_button_lights_while_any_argument_is_overridden() {
    use qtable_engine::scheduler::AutomationState as Engine;
    let (mut app, track, _) = app();
    let volume = volume_field(track);
    assert!(!crate::transport::any_overridden(&app));
    drive(&mut app, &volume, Engine::Following);
    assert!(!crate::transport::any_overridden(&app));
    drive(&mut app, &volume, Engine::Overridden);
    assert!(crate::transport::any_overridden(&app));
}

#[test]
fn choosing_re_enable_closes_the_menu_and_sends_the_command() {
    let (mut app, track, _) = app();
    app.open_overlay(crate::overlay::Overlay::Context(
        crate::context_menu::OpenMenu {
            at: iced::Point::ORIGIN,
            target: crate::context_menu::MenuTarget::Argument {
                subject: crate::info::Subject::Volume(track),
                revision: app.engine.project_revision(),
            },
        },
    ));
    let _ = crate::update::update(
        &mut app,
        Message::ReEnableAutomation(Some(volume_field(track))),
    );
    assert!(app.overlay.is_none());
    let _ = crate::update::update(&mut app, Message::ReEnableAutomation(None));
}
