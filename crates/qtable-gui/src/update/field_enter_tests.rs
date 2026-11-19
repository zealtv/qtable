//! Enter in a text field commits it and lets go of the keyboard, unless the value was rejected
//! (decided 2026-11-15). The field publishes `FieldEntered` right after its own
//! submit message; the release is the one task that follows, as the shortcuts' `release_field`.
use super::update;
use crate::app::App;
use crate::drafts::{DraftKey, TrackField};
use crate::messages::{EditMessage, Message};
use qtable_engine::engine::Engine;
use qtable_engine::model::{Project, TrackId};

fn app() -> App {
    let engine = Engine::new(Project::default()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    let _ = update(&mut app, Message::Edit(EditMessage::AddTrack));
    app
}

/// What Enter sends: the field's submit message, then `FieldEntered`. Returns how many tasks
/// the second one started (one means the keyboard was released).
fn enter(app: &mut App, submit: Message) -> usize {
    let _ = update(app, submit);
    update(app, Message::FieldEntered).units()
}

fn track(app: &App) -> TrackId {
    app.project().tracks[0].id
}

#[test]
fn an_accepted_tempo_releases_the_field() {
    let mut app = app();
    let _ = update(&mut app, Message::TempoChanged("93".into()));
    assert_eq!(enter(&mut app, Message::TempoSubmitted), 1);
    assert_eq!(app.pending_field, None);
}

#[test]
fn a_rejected_tempo_keeps_the_keyboard_in_the_field() {
    let mut app = app();
    let _ = update(&mut app, Message::TempoChanged("fast".into()));
    assert_eq!(enter(&mut app, Message::TempoSubmitted), 0);
    assert_eq!(app.pending_field, Some(DraftKey::Tempo));
    // Put right, the next Enter releases.
    let _ = update(&mut app, Message::TempoChanged("93".into()));
    assert_eq!(enter(&mut app, Message::TempoSubmitted), 1);
}

#[test]
fn a_path_field_releases_when_accepted_and_holds_when_rejected() {
    let mut app = app();
    let id = track(&app);
    let key = DraftKey::Track(id, TrackField::Path);
    let _ = update(
        &mut app,
        Message::Edit(EditMessage::PathChanged(id, "/bad/".into())),
    );
    assert_eq!(
        enter(
            &mut app,
            Message::Edit(EditMessage::PathSubmitted(id, "/bad/".into()))
        ),
        0,
        "a path ending in a slash is refused"
    );
    assert_eq!(app.pending_field, Some(key));
    let _ = update(
        &mut app,
        Message::Edit(EditMessage::PathChanged(id, "/drums".into())),
    );
    assert_eq!(
        enter(
            &mut app,
            Message::Edit(EditMessage::PathSubmitted(id, "/drums".into()))
        ),
        1
    );
}

#[test]
fn the_detail_view_routes_its_enter_the_same_way() {
    let mut app = app();
    let _ = update(&mut app, Message::TempoChanged("93".into()));
    let _ = update(&mut app, Message::TempoSubmitted);
    let released = update(
        &mut app,
        Message::Inspector(crate::inspector::Message::FieldEntered),
    );
    assert_eq!(released.units(), 1);
}
