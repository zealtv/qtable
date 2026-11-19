//! Address edits and load errors through the update loop, without desktop input.
use super::update;
use crate::app::App;
use crate::drafts::DraftKey;
use crate::drafts::MixerField;
use crate::drafts::TrackField;
use crate::messages::{EditMessage, Message};
use crate::mixer_device::MixerMessage;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::{file, model::*};

fn send(app: &mut App, message: Message) {
    let _ = update(app, message);
}
fn loaded() -> Project {
    file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/loaded-invalid.ron"
    ))
    .unwrap()
}
fn app() -> App {
    let mut p = loaded();
    for t in p
        .tracks
        .iter_mut()
        .chain(&mut p.outputs)
        .chain([&mut p.main])
    {
        t.path = Path::default();
        t.mixer_device.volume.output.address = "/volume".into();
        t.mixer_device.volume.output.enabled = true;
        if t.slots.iter().all(|s| s.clip.is_none()) {
            continue;
        }
        let device = qtable_engine::model::DeviceId(1000 + t.id.0);
        let play = OscAddress::Relative("/play".into());
        let play = t.hit(qtable_engine::model::BeatTime::ZERO, play, vec![], || {
            device
        });
        for clip in t.slots.iter_mut().filter_map(|s| s.clip.as_mut()) {
            clip.lanes.clear();
            clip.events[0] = qtable_engine::model::Event {
                position: clip.events[0].position,
                ..play.clone()
            };
        }
    }
    App::new(Engine::new(p).unwrap(), "120".into(), None)
}
fn blur(app: &mut App) {
    let key = app.pending_field.unwrap();
    let value = crate::field_focus::value(app, key).unwrap();
    send(
        app,
        Message::FieldFocusChecked(key, value, app.project_generation, false),
    );
}
fn rejected(app: &App, key: DraftKey, value: &str, original: &Project) {
    assert_eq!(app.project(), original);
    assert_eq!(app.drafts.values[&key], value);
    assert!(app.drafts.errors[&key].contains("cannot contain"));
    assert!(app.status.message().unwrap().contains("cannot contain"));
    assert_eq!(app.status.severity(), crate::status::Severity::Error);
}

#[test]
fn all_address_fields_keep_rejected_drafts_and_explain_each_character() {
    let mut app = app();
    for character in [' ', '#', '*', ',', '?', '[', ']', '{', '}'] {
        let invalid = format!("/bad{character}value");
        for id in [TrackId(1), TrackId(10), TrackId(201), app.project().main.id] {
            let original = app.project().clone();
            let key = DraftKey::Track(id, TrackField::Path);
            send(
                &mut app,
                Message::Edit(EditMessage::PathChanged(id, invalid.clone())),
            );
            blur(&mut app);
            rejected(&app, key, &invalid, &original);
            send(&mut app, Message::CancelField);

            let key = DraftKey::Mixer(id, MixerField::Address);
            send(
                &mut app,
                Message::Edit(EditMessage::MixerDevice(MixerMessage::Draft(
                    key,
                    invalid.clone(),
                ))),
            );
            blur(&mut app);
            rejected(&app, key, &invalid, &original);
            send(&mut app, Message::CancelField);
        }
    }
}

fn assert_loaded_errors(app: &mut App) {
    assert!(app.status.message().unwrap().contains("cannot contain"));
    assert_eq!(app.status.severity(), crate::status::Severity::Error);
    for id in [TrackId(1), TrackId(10), TrackId(201), app.project().main.id] {
        let t = app.project().track(id).unwrap();
        assert!(
            app.drafts
                .address_error(DraftKey::Track(id, TrackField::Path), t.path.validate())
                .is_some()
        );
        assert!(
            app.drafts
                .address_error(
                    DraftKey::Mixer(id, MixerField::Address),
                    validate_address(&t.mixer_device.volume.output.address)
                )
                .is_some()
        );
        // Both views use the stored-value error even without a rejected draft.
        let _ = crate::path_band::path(app, id);
        let _ = crate::mixer_device::view(app, t, crate::control_focus::View::Track);
    }
    send(
        app,
        Message::Grid(crate::grid::Action::SelectTrack(TrackId(1), false)),
    );
    send(
        app,
        Message::Grid(crate::grid::Action::SelectTrack(TrackId(10), false)),
    );
    let key = DraftKey::Mixer(TrackId(1), MixerField::Address);
    let validation =
        validate_address(&app.project()[TrackId(1)].mixer_device.volume.output.address);
    assert!(app.drafts.address_error(key, validation).is_some());
}

#[test]
fn loaded_errors_survive_startup_replacement_selection_and_escape_until_repaired() {
    let mut app = App::new(Engine::new(loaded()).unwrap(), "120".into(), None);
    assert_loaded_errors(&mut app);
    let id = TrackId(1);
    send(
        &mut app,
        Message::Edit(EditMessage::PathChanged(id, "/draft".into())),
    );
    send(&mut app, Message::CancelField);
    assert!(
        app.drafts
            .address_error(
                DraftKey::Track(id, TrackField::Path),
                app.project()[id].path.validate()
            )
            .is_some()
    );
    send(
        &mut app,
        Message::Edit(EditMessage::PathChanged(id, "/fixed/path".into())),
    );
    blur(&mut app);
    assert!(
        app.drafts
            .address_error(
                DraftKey::Track(id, TrackField::Path),
                app.project()[id].path.validate()
            )
            .is_none()
    );
    app.engine.execute(Command::LoadProject(loaded())).unwrap();
    app.reset_for_project();
    assert_loaded_errors(&mut app);
}
