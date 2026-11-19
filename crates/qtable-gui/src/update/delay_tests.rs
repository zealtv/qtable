//! Track Delay for groups and Main (2026-10-13).
use super::update;
use crate::app::App;
use crate::messages::{EditMessage, FileMessage, Message, OutputMessage};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{Project, TrackId};

fn send(app: &mut App, message: OutputMessage) {
    let _ = update(app, Message::Outputs(message));
}

fn app() -> (App, TrackId) {
    let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
    let _ = update(&mut app, Message::Files(FileMessage::New));
    let _ = update(&mut app, Message::Edit(EditMessage::AddTrack));
    let tracks: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
    app.engine
        .execute(Command::Edit(ProjectEdit::CreateGroup {
            tracks: tracks[..2].to_vec(),
        }))
        .unwrap();
    let group = app.project().group_tracks().next().unwrap().id;
    (app, group)
}

#[test]
fn group_and_main_delay_commit_and_show_in_track_facts() {
    let (mut app, group) = app();
    let main = app.project().main.id;
    send(
        &mut app,
        OutputMessage::TrackDelayChanged(group, "-2.5".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelaySubmitted(group, "-2.5".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelayChanged(main, "10".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelaySubmitted(main, "10".into()),
    );
    assert_eq!(app.project().group_tracks().next().unwrap().delay_ms, -2.5);
    assert_eq!(app.project().main.delay_ms, 10.0);
    let facts = app.project().track_facts(group).unwrap();
    assert_eq!(facts.delay_ms, Some(-2.5));
    assert_eq!(
        app.project()
            .track_facts(app.project().main.id)
            .unwrap()
            .delay_ms,
        Some(10.0)
    );
    let track = app.project().tracks[0].id;
    assert_eq!(app.project().effective_delay_ms(track), 7.5);
}

#[test]
fn untouched_group_and_main_display_keeps_the_precise_value() {
    let (mut app, group) = app();
    let main = app.project().main.id;
    send(
        &mut app,
        OutputMessage::TrackDelayChanged(main, "0.1234".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelaySubmitted(main, "0.1234".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelayChanged(group, "0.1234".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelaySubmitted(group, "0.1234".into()),
    );
    let revision = app.project_revision();
    send(
        &mut app,
        OutputMessage::TrackDelaySubmitted(main, "0.1234".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelaySubmitted(group, "0.1234".into()),
    );
    assert_eq!(app.project().main.delay_ms, 0.1234);
    assert_eq!(
        app.project().group_tracks().next().unwrap().delay_ms,
        0.1234
    );
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn delays_beyond_the_limit_are_rejected() {
    let (mut app, group) = app();
    let main = app.project().main.id;
    send(
        &mut app,
        OutputMessage::TrackDelayChanged(main, "5000".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelaySubmitted(main, "5000".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelayChanged(group, "2000".into()),
    );
    send(
        &mut app,
        OutputMessage::TrackDelaySubmitted(group, "2000".into()),
    );
    assert_eq!(app.project().main.delay_ms, 0.0);
    assert_eq!(app.project().group_tracks().next().unwrap().delay_ms, 0.0);
}
